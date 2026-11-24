use bevy::prelude::*;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;

use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_zoo_state::construction::construction_interaction_types::CancelConstruction;
use openzt2_zoo_state::construction::construction_interaction_types::ConstructionPreview;

use super::authored_terrain_brush_modes::AuthoredTerrainBrushModes;
use crate::terrain::terrain_brush_sample_mutation::prepare_terrain_chunk_sample_delta_from_brush_dabs;
use crate::terrain::terrain_brush_sample_mutation::TerrainSmoothing;
use crate::terrain::terrain_chunk_index_cleanup::TerrainChunkIdLookup;
use crate::terrain::terrain_edit_application::apply_validated_terrain_edit_delta_and_publish_changed_region;
use openzt2_zoo_state::terrain::terrain_brush_interaction_state::ActiveTerrainBrush;
use openzt2_zoo_state::terrain::terrain_brush_interaction_state::{
    PendingTerrainBrushCommit, TerrainBrushDab, TerrainBrushPath,
};
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

use super::terrain_sample_change_calculations::TerrainRenderInvalidated;
use openzt2_zoo_state::terrain::terrain_water_presentation_types::{
    TerrainWaterRegionEdit, TerrainWaterRegionQuery, TerrainWaterRegionRetired,
};

#[derive(Event)]
pub(super) struct ApplyHeldTerrainStroke(TerrainStroke);

pub(super) fn dispatch_held_terrain_strokes(
    mut commands: Commands,
    mut strokes: MessageReader<TerrainStroke>,
) {
    for &stroke in strokes.read() {
        commands.trigger(ApplyHeldTerrainStroke(stroke));
    }
}

#[derive(Component, Default)]
pub(super) struct TerrainBrushDraft(pub(super) TerrainEdit);

/// Applies each held pointer dab to the terrain immediately while
/// retaining the ordered before/after deltas as one draft undo transaction.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn apply_held_terrain_brush_dabs_and_accumulate_one_draft_edit(
    stroke: On<ApplyHeldTerrainStroke>,
    mut commands: Commands,
    roots: Query<(&TerrainIndex, &TerrainChunkIdLookup)>,
    assets: Res<Assets<TerrainAsset>>,
    modes: AuthoredTerrainBrushModes,
    mut region_data: ParamSet<(
        TerrainWaterRegionQuery,
        Query<(
            Option<&mut TerrainWaterRegionEdit>,
            Has<TerrainWaterRegionRetired>,
        )>,
    )>,
    water_membership: Query<
        &openzt2_zoo_state::terrain::terrain_water_presentation_types::TerrainWaterMembershipIndex,
    >,
    mut chunks: Query<(
        Entity,
        &TerrainChunk,
        Option<&mut EditedTerrainSamples>,
        &TerrainChunkId,
    )>,
    mut previews: Query<
        (
            &mut TerrainBrushPreview,
            &WorldMember,
            Option<&mut TerrainBrushPath>,
            Option<&mut TerrainBrushDraft>,
            Has<ActiveTerrainBrush>,
            Has<PendingTerrainBrushCommit>,
        ),
        With<ConstructionPreview>,
    >,
    mut changed: MessageWriter<TerrainChanged>,
    mut navigation_changed: MessageWriter<TerrainNavigationChanged>,
    mut water_changed: MessageWriter<TerrainWaterChanged>,
    mut render_changed: MessageWriter<TerrainRenderInvalidated>,
) {
    let stroke = stroke.0;
    let Ok((mut current, member, path, draft, active, pending)) = previews.get_mut(stroke.preview)
    else {
        return;
    };
    let Ok((index, chunk_ids)) = roots.get(member.root) else {
        return;
    };
    let read_neighbour = |entity, x, z| {
        let (_, chunk, edited, _) = chunks.get(entity).ok()?;
        let asset = assets.get(&chunk.asset)?;
        openzt2_zoo_state::terrain::terrain_sample_grid_queries::read_terrain_sample_height_centimetres(
            chunk, asset, edited, x, z,
        )
    };
    let smoothing = modes
        .smooth_samples()
        .map(|radius_samples| TerrainSmoothing {
            radius_samples,
            terrain: index,
            read_height: &read_neighbour,
        });
    if !(stroke.center.is_finite()
        && stroke.radius_m.is_finite()
        && stroke.radius_m > 0.0
        && stroke.strength_per_s.is_finite()
        && stroke.seconds.is_finite()
        && stroke.seconds >= 0.0)
    {
        return;
    }
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
    let elapsed = current.seconds + stroke.seconds;
    current.set_if_neq(TerrainBrushPreview {
        seconds: elapsed,
        ..preview
    });
    let live = draft.is_some() && (active || pending);
    if !live || stroke.paint_trees || stroke.paint_foliage || stroke.paint_rocks {
        if let Some(mut path) = path {
            if let Some(previous) = path
                .0
                .last_mut()
                .filter(|previous| previous.center == dab.center)
            {
                previous.seconds += dab.seconds;
            } else {
                path.0.push(dab);
            }
        } else {
            commands
                .entity(stroke.preview)
                .insert(TerrainBrushPath(vec![dab]));
        }
    }
    let Some(mut draft_edit) = draft.filter(|_| live) else {
        return;
    };
    let water_regions = region_data.p0();
    if !index.chunk_span_m.is_finite() || index.chunk_span_m <= 0.0 {
        return;
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
                    let (_, chunk, edited, _) = chunks.get(*entity).ok()?;
                    Some((chunk, assets.get(&chunk.asset)?, edited))
                }),
            );

    let water_plan = if matches!(
        preview.kind,
        openzt2_zoo_state::terrain::terrain_brush_types::TerrainBrushKind::PaintWater { .. }
            | openzt2_zoo_state::terrain::terrain_brush_types::TerrainBrushKind::AddWater
            | openzt2_zoo_state::terrain::terrain_brush_types::TerrainBrushKind::RemoveWater
    ) {
        let Some((_, chunk, _, _)) = chunks
            .iter()
            .find(|(_, chunk, _, _)| chunk.source_coord == IVec2::ZERO)
        else {
            return;
        };
        let Some(asset) = assets.get(&chunk.asset) else {
            return;
        };
        let Ok(membership) = water_membership.single() else {
            warn!(target: "openzt2_water_brush", preview = ?stroke.preview, "water brush dab rejected: terrain membership unavailable");
            return;
        };
        let samples: Vec<_> = chunks.iter().filter(|(entity, candidate, _, _)| candidate.asset == chunk.asset && candidate_entities.contains(entity)).flat_map(|(_, candidate, edited, _)| {
                (0..usize::from(candidate.side)).flat_map(move |z| (0..usize::from(candidate.side)).filter_map(move |x| {
                    Some((candidate.origin + Vec2::new(x as f32, z as f32) * candidate.spacing_m, openzt2_zoo_state::terrain::terrain_sample_grid_queries::read_materialized_terrain_sample(candidate, asset, edited, x, z)?))
                }))
            }).collect();
        let Some(initial_height) = water_surface_height_cm else {
            return;
        };
        let plan = crate::terrain::terrain_brush_sample_mutation::water_affiliation::plan_water_brush_region(&preview, &chunk.asset, asset, samples.into_iter(), &water_regions, membership, initial_height, dab.seconds);
        let Some(plan) = plan else {
            return;
        };
        water_surface_height_cm = plan.ground_reference_height_cm(asset);
        candidate_entities.extend(chunks.iter().filter_map(|(entity, candidate, _, _)| {
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
            let Ok((_, chunk, edited, persistent)) = chunks.get_mut(entity) else {
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
                smoothing.as_ref(),
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
        return;
    }
    let mut owner_changes = water_plan.map_or_else(Vec::new, |plan| plan.changes);
    if let Ok(membership) = water_membership.single() {
        let Some(asset) = assets.get(membership.terrain()) else {
            return;
        };
        let Some((parent, _, _, _)) = chunks.iter().find(|(_, chunk, _, _)| {
            chunk.source_coord == IVec2::ZERO && chunk.asset.id() == membership.terrain()
        }) else {
            return;
        };
        if crate::terrain::terrain_brush_sample_mutation::water_affiliation::splitting::split_disconnected_water_affiliations(
                &mut commands, asset, parent, membership, &water_regions,
                chunks.iter().map(|(_, chunk, edited, persistent)| (chunk, edited, *persistent)),
                &mut applied_deltas, &mut owner_changes,
            ).is_none() { return; }
    }
    let mut pending_overlays = bevy::ecs::entity::EntityHashMap::default();
    for delta in &applied_deltas {
        let Some(&entity) = chunk_ids.0.get(&delta.chunk) else {
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
            &mut render_changed,
            &mut pending_overlays,
        );
    }
    for (chunk, overlay) in pending_overlays {
        commands.entity(chunk).insert(overlay);
    }
    crate::terrain::terrain_edit_application::apply_water_region_edit_changes(
        &mut commands,
        &mut region_data.p1(),
        owner_changes
            .iter()
            .map(|delta| (delta.owner, delta.after.clone(), delta.after_active)),
    );
    let owners: bevy::ecs::entity::EntityHashSet =
        owner_changes.iter().map(|change| change.owner).collect();
    for owner in owners {
        if !draft_edit
            .0
            .water_regions
            .iter()
            .any(|change| change.owner == owner)
        {
            commands.spawn((
                super::water_regions::edit_retention::WaterRegionRetainedByEdit(owner),
                ChildOf(stroke.preview),
            ));
        }
    }
    draft_edit.0.deltas.extend(applied_deltas);
    draft_edit.0.water_regions.extend(owner_changes);
}

/// A cancelled held stroke never enters history, so restore its ordered
/// before-images before the generic construction owner despawns the preview.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn roll_back_cancelled_live_terrain_brush_edit(
    mut commands: Commands,
    mut cancellations: MessageReader<CancelConstruction>,
    mut previews: Query<(Entity, &WorldMember, &mut TerrainBrushDraft), With<ConstructionPreview>>,
    roots: Query<&TerrainChunkIdLookup>,
    assets: Res<Assets<TerrainAsset>>,
    mut owners: Query<(
        Option<&mut TerrainWaterRegionEdit>,
        Has<TerrainWaterRegionRetired>,
    )>,
    mut chunks: Query<(
        Entity,
        &TerrainChunk,
        Option<&mut EditedTerrainSamples>,
        &TerrainChunkId,
    )>,
    mut changed: MessageWriter<TerrainChanged>,
    mut navigation_changed: MessageWriter<TerrainNavigationChanged>,
    mut water_changed: MessageWriter<TerrainWaterChanged>,
    mut render_changed: MessageWriter<TerrainRenderInvalidated>,
) {
    if cancellations.read().next().is_none() {
        return;
    }
    for (preview_entity, member, mut draft) in &mut previews {
        let Ok(chunk_ids) = roots.get(member.root) else {
            continue;
        };
        let edit = std::mem::take(&mut draft.0);
        crate::terrain::terrain_edit_application::apply_water_region_edit_changes(
            &mut commands,
            &mut owners,
            edit.water_regions
                .iter()
                .rev()
                .map(|delta| (delta.owner, delta.before.clone(), delta.before_active)),
        );
        let mut pending_overlays = bevy::ecs::entity::EntityHashMap::default();
        for delta in edit.deltas.iter().rev() {
            let Some(&chunk_entity) = chunk_ids.0.get(&delta.chunk) else {
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
                &mut render_changed,
                &mut pending_overlays,
            );
        }
        for (chunk, overlay) in pending_overlays {
            commands.entity(chunk).insert(overlay);
        }
    }
}
