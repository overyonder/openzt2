use bevy::prelude::*;

use crate::{
    assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset,
    plugins::construction::construction_interaction_types::{
        CancelConstruction, ConstructionPreview,
    },
};

use super::{
    terrain_brush_interaction_state::{ActiveTerrainBrush, TerrainBrushDab},
    terrain_brush_sample_mutation::prepare_terrain_chunk_sample_delta_from_brush_dabs,
    terrain_brush_types::{TerrainBrushPreview, TerrainStroke},
    terrain_change_tracking_types::TerrainDirty,
    terrain_chunk_identity_type::TerrainChunkId,
    terrain_chunk_types::{EditedTerrainSamples, TerrainChunk, TerrainIndex},
    terrain_edit_application::apply_validated_terrain_edit_delta_and_publish_changed_region,
    terrain_edit_types::{TerrainChanged, TerrainEdit, TerrainWaterChanged},
    terrain_navigation_change_types::TerrainNavigationChanged,
};

/// Applies each held pointer dab to the terrain immediately while
/// retaining the ordered before/after deltas as one draft undo transaction.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn apply_held_terrain_brush_dabs_and_accumulate_one_draft_edit(
    mut commands: Commands,
    mut strokes: MessageReader<TerrainStroke>,
    index: Res<TerrainIndex>,
    assets: Res<Assets<TerrainAsset>>,
    mut chunks: Query<(
        Entity,
        &TerrainChunk,
        Option<&mut EditedTerrainSamples>,
        Option<&mut TerrainDirty>,
        &TerrainChunkId,
    )>,
    mut previews: Query<Option<&mut TerrainEdit>, With<ActiveTerrainBrush>>,
    mut changed: MessageWriter<TerrainChanged>,
    mut navigation_changed: MessageWriter<TerrainNavigationChanged>,
    mut water_changed: MessageWriter<TerrainWaterChanged>,
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

        let water_surface_height_cm =
            super::terrain_brush_sample_mutation::resolve_painted_water_surface_height_centimetres(
                &preview,
                &dab,
                candidate_entities.iter().filter_map(|entity| {
                    let (_, chunk, edited, _, _) = chunks.get(*entity).ok()?;
                    Some((chunk, assets.get(&chunk.asset)?, edited))
                }),
            );

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
                )
            };
            let Some(delta) = delta else {
                continue;
            };
            apply_validated_terrain_edit_delta_and_publish_changed_region(
                &mut commands,
                &mut chunks,
                &assets,
                stroke.preview,
                entity,
                &delta,
                true,
                &mut changed,
                &mut navigation_changed,
                &mut water_changed,
            );
            applied_deltas.push(delta);
        }
        if applied_deltas.is_empty() {
            continue;
        }
        if let Some(mut draft_edit) = draft_edit {
            draft_edit.deltas.extend(applied_deltas);
        } else {
            commands.entity(stroke.preview).insert(TerrainEdit {
                deltas: applied_deltas,
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
