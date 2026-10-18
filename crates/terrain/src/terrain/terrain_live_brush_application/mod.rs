use bevy::prelude::*;

use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_zoo_state::construction::construction_interaction_types::CancelConstruction;
use openzt2_zoo_state::construction::construction_interaction_types::ConstructionPreview;

use crate::terrain::terrain_brush_sample_mutation::prepare_terrain_chunk_sample_delta_from_brush_dabs;
use crate::terrain::terrain_change_tracking_types::TerrainDirty;
use crate::terrain::terrain_edit_application::apply_validated_terrain_edit_delta_and_publish_changed_region;
use openzt2_zoo_state::terrain::terrain_brush_interaction_state::ActiveTerrainBrush;
use openzt2_zoo_state::terrain::terrain_brush_interaction_state::TerrainBrushDab;
use openzt2_zoo_state::terrain::terrain_brush_types::TerrainBrushPreview;
use openzt2_zoo_state::terrain::terrain_brush_types::TerrainStroke;
use openzt2_zoo_state::terrain::terrain_chunk_identity_type::TerrainChunkId;
use openzt2_zoo_state::terrain::terrain_chunk_types::EditedTerrainSamples;
use openzt2_zoo_state::terrain::terrain_chunk_types::TerrainChunk;
use openzt2_zoo_state::terrain::terrain_chunk_types::TerrainIndex;
use openzt2_zoo_state::terrain::terrain_edit_types::TerrainChanged;
use openzt2_zoo_state::terrain::terrain_edit_types::TerrainEdit;
use openzt2_zoo_state::terrain::terrain_edit_types::TerrainWaterChanged;
use openzt2_zoo_state::terrain::terrain_navigation_change_types::TerrainNavigationChanged;

/// Applies each held pointer dab to the terrain immediately while
/// retaining the ordered before/after deltas as one draft undo transaction.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn apply_held_terrain_brush_dabs_and_accumulate_one_draft_edit(
    mut commands: Commands,
    mut strokes: MessageReader<TerrainStroke>,
    index: Res<TerrainIndex>,
    assets: Res<Assets<TerrainAsset>>,
    water_regions: openzt2_zoo_state::terrain::terrain_water_presentation_types::TerrainWaterRegionQuery,
    water_membership: Query<
        &openzt2_zoo_state::terrain::terrain_water_presentation_types::TerrainWaterMembershipIndex,
    >,
    mut chunks: Query<(
        Entity,
        &TerrainChunk,
        Option<&mut EditedTerrainSamples>,
        Option<&mut TerrainDirty>,
        &TerrainChunkId,
    )>,
    mut previews: Query<Option<&mut TerrainEdit>, Or<(With<ActiveTerrainBrush>, With<openzt2_zoo_state::terrain::terrain_brush_interaction_state::PendingTerrainBrushCommit>)>>,
    mut changed: MessageWriter<TerrainChanged>,
    mut navigation_changed: MessageWriter<TerrainNavigationChanged>,
    mut water_changed: MessageWriter<TerrainWaterChanged>,
    mut missing_membership: Local<Option<(Entity, usize)>>,
) {
    for stroke in strokes.read() {
        let Ok(draft_edit) = previews.get_mut(stroke.preview) else {
            continue;
        };
        let preview = TerrainBrushPreview {
            center: stroke.center,
            radius_m: stroke.radius_m,
            strength_per_s: stroke.strength_per_s,
            seconds: stroke.seconds,
            kind: stroke.kind,
            falloff: stroke.falloff,
            paint_trees: stroke.paint_trees,
            paint_foliage: stroke.paint_foliage,
            paint_rocks: stroke.paint_rocks,
            automatic_placement_variation: stroke.automatic_placement_variation,
        };
        let dab = TerrainBrushDab {
            center: stroke.center,
            seconds: stroke.seconds,
        };
        if !index.chunk_span_m.is_finite() || index.chunk_span_m <= 0.0 {
            continue;
        }
        let minimum = stroke.center - Vec2::splat(stroke.radius_m);
        let maximum = stroke.center + Vec2::splat(stroke.radius_m);
        let minimum_chunk = ((minimum - index.origin) / index.chunk_span_m)
            .floor()
            .as_ivec2();
        let maximum_chunk = ((maximum - index.origin) / index.chunk_span_m)
            .floor()
            .as_ivec2();
        let mut candidate_entities = Vec::new();
        for z in minimum_chunk.y..=maximum_chunk.y {
            for x in minimum_chunk.x..=maximum_chunk.x {
                if let Some(entity) = index.chunks.get(&IVec2::new(x, z)).copied() {
                    candidate_entities.push(entity);
                }
            }
        }
        candidate_entities.sort_unstable();

        let mut water_surface_height_cm =
            crate::terrain::terrain_brush_sample_mutation::resolve_painted_water_surface_height_centimetres(
                &preview,
                &dab,
                candidate_entities.iter().filter_map(|entity| {
                    let (_, chunk, edited, _, _) = chunks.get(*entity).ok()?;
                    Some((chunk, assets.get(&chunk.asset)?, edited))
                }),
            );

        let water_plan = if matches!(
            preview.kind,
            openzt2_zoo_state::terrain::terrain_brush_types::TerrainBrushKind::PaintWater { .. }
                | openzt2_zoo_state::terrain::terrain_brush_types::TerrainBrushKind::AddWater
                | openzt2_zoo_state::terrain::terrain_brush_types::TerrainBrushKind::RemoveWater
        ) {
            let Some((root, chunk, _, _, _)) = chunks
                .iter()
                .find(|(_, chunk, _, _, _)| chunk.source_coord == IVec2::ZERO)
            else {
                continue;
            };
            let Some(asset) = assets.get(&chunk.asset) else {
                continue;
            };
            let Ok(membership) = water_membership.single() else {
                let count = water_membership.iter().count();
                if *missing_membership != Some((stroke.preview, count)) {
                    warn!(target: "openzt2_water_brush", preview = ?stroke.preview, membership_indexes = count, "water brush dab rejected: expected one resolved terrain membership index");
                    *missing_membership = Some((stroke.preview, count));
                }
                continue;
            };
            *missing_membership = None;
            let samples: Vec<_> = chunks.iter().filter(|(entity, candidate, _, _, _)| candidate.asset == chunk.asset && candidate_entities.contains(entity)).flat_map(|(_, candidate, edited, _, _)| {
                (0..usize::from(candidate.side)).flat_map(move |z| (0..usize::from(candidate.side)).filter_map(move |x| {
                    Some((candidate.origin + Vec2::new(x as f32, z as f32) * candidate.spacing_m, openzt2_zoo_state::terrain::terrain_sample_grid_queries::read_materialized_terrain_sample(candidate, asset, edited, x, z)?))
                }))
            }).collect();
            let Some(initial_height) = water_surface_height_cm else {
                continue;
            };
            let plan = crate::terrain::terrain_brush_sample_mutation::water_affiliation::plan_water_brush_region(&mut commands, &preview, &chunk.asset, asset, root, samples.into_iter(), &water_regions, membership, initial_height, dab.seconds);
            let Some(plan) = plan else {
                continue;
            };
            water_surface_height_cm = plan.ground_reference_height_cm(asset);
            candidate_entities.extend(chunks.iter().filter_map(|(entity, candidate, _, _, _)| {
                (candidate.asset == chunk.asset && plan.affects_chunk(candidate, membership))
                    .then_some(entity)
            }));
            candidate_entities.sort_unstable();
            candidate_entities.dedup();
            Some(plan)
        } else {
            None
        };

        let mut applied_deltas = Vec::with_capacity(candidate_entities.len());
        for entity in candidate_entities {
            let delta = {
                let Ok((_, chunk, edited, _, persistent)) = chunks.get_mut(entity) else {
                    continue;
                };
                let Some(asset) = assets.get(&chunk.asset) else {
                    continue;
                };
                prepare_terrain_chunk_sample_delta_from_brush_dabs(
                    *persistent,
                    chunk,
                    asset,
                    edited.as_deref(),
                    &preview,
                    core::slice::from_ref(&dab),
                    &[water_surface_height_cm],
                    water_plan
                        .as_ref()
                        .zip(water_membership.single().ok())
                        .map(|(plan, membership)| (plan, &water_regions, membership)),
                )
            };
            let Some(delta) = delta else {
                continue;
            };
            applied_deltas.push(delta);
        }
        if applied_deltas.is_empty() {
            continue;
        }
        let mut owner_changes = water_plan.map_or_else(Vec::new, |plan| plan.changes);
        if let Ok(membership) = water_membership.single() {
            let Some(asset) = assets.get(membership.terrain()) else {
                continue;
            };
            let Some((parent, _, _, _, _)) = chunks.iter().find(|(_, chunk, _, _, _)| {
                chunk.source_coord == IVec2::ZERO && chunk.asset.id() == membership.terrain()
            }) else {
                continue;
            };
            if crate::terrain::terrain_brush_sample_mutation::water_affiliation::splitting::split_disconnected_water_affiliations(
                &mut commands, asset, parent, membership, &water_regions,
                chunks.iter().map(|(_, chunk, edited, _, persistent)| (chunk, edited, *persistent)),
                &mut applied_deltas, &mut owner_changes,
            ).is_none() { continue; }
        }
        for delta in &applied_deltas {
            let Some(entity) = chunks.iter().find_map(|(entity, _, _, _, persistent)| {
                (*persistent == delta.chunk).then_some(entity)
            }) else {
                continue;
            };
            apply_validated_terrain_edit_delta_and_publish_changed_region(
                &mut commands,
                &mut chunks,
                &assets,
                stroke.preview,
                entity,
                delta,
                true,
                &mut changed,
                &mut navigation_changed,
                &mut water_changed,
            );
        }
        crate::terrain::terrain_edit_application::apply_water_region_edit_changes(
            &mut commands,
            owner_changes
                .iter()
                .map(|delta| (delta.owner, delta.after.clone(), delta.after_active))
                .collect(),
        );
        if let Some(mut draft_edit) = draft_edit {
            draft_edit.deltas.extend(applied_deltas);
            draft_edit.water_regions.extend(owner_changes);
        } else {
            commands.entity(stroke.preview).insert(TerrainEdit {
                deltas: applied_deltas,
                water_regions: owner_changes,
            });
        }
    }
}

/// A cancelled held stroke never enters history, so restore its ordered
/// before-images before the generic construction owner despawns the preview.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn roll_back_cancelled_live_terrain_brush_edit(
    mut commands: Commands,
    mut cancellations: MessageReader<CancelConstruction>,
    previews: Query<(Entity, &TerrainEdit), With<ConstructionPreview>>,
    assets: Res<Assets<TerrainAsset>>,
    mut chunks: Query<(
        Entity,
        &TerrainChunk,
        Option<&mut EditedTerrainSamples>,
        Option<&mut TerrainDirty>,
        &TerrainChunkId,
    )>,
    mut changed: MessageWriter<TerrainChanged>,
    mut navigation_changed: MessageWriter<TerrainNavigationChanged>,
    mut water_changed: MessageWriter<TerrainWaterChanged>,
) {
    if cancellations.read().next().is_none() {
        return;
    }
    for (preview_entity, edit) in &previews {
        crate::terrain::terrain_edit_application::apply_water_region_edit_changes(
            &mut commands,
            edit.water_regions
                .iter()
                .rev()
                .map(|delta| (delta.owner, delta.before.clone(), delta.before_active))
                .collect(),
        );
        for delta in edit.deltas.iter().rev() {
            let Some(chunk_entity) = chunks.iter_mut().find_map(|(entity, _, _, _, persistent)| {
                (*persistent == delta.chunk).then_some(entity)
            }) else {
                continue;
            };
            apply_validated_terrain_edit_delta_and_publish_changed_region(
                &mut commands,
                &mut chunks,
                &assets,
                preview_entity,
                chunk_entity,
                delta,
                false,
                &mut changed,
                &mut navigation_changed,
                &mut water_changed,
            );
        }
        commands.entity(preview_entity).remove::<TerrainEdit>();
    }
}
