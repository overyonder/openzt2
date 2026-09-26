use bevy::prelude::*;

use crate::{
    assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset,
    plugins::construction::{
        construction_interaction_types::{
            ConstructionPreview, PlacementFailure, PlacementValidity,
        },
        construction_transaction_types::PrepareConstruction,
    },
};

use super::{
    terrain_brush_interaction_state::{TerrainBrushDab, TerrainBrushPath},
    terrain_brush_sample_mutation::{
        paint_layers_are_available_for_terrain_brush,
        painted_water_depth_is_available_for_terrain_brush,
        prepare_terrain_chunk_sample_delta_from_brush_dabs,
    },
    terrain_brush_types::{TerrainBrushKind, TerrainBrushPreview},
    terrain_chunk_identity_type::TerrainChunkId,
    terrain_chunk_types::{EditedTerrainSamples, TerrainChunk, TerrainIndex},
    terrain_edit_types::{TerrainEdit, TerrainEditPreparationRejected, TerrainEditPrepared},
};
pub(super) fn prepare_terrain_sample_deltas_for_construction_transactions(
    mut commands: Commands,
    mut requests: MessageReader<PrepareConstruction>,
    index: Res<TerrainIndex>,
    assets: Res<Assets<TerrainAsset>>,
    chunks: Query<(
        Entity,
        &TerrainChunk,
        Option<&EditedTerrainSamples>,
        &TerrainChunkId,
    )>,
    previews: Query<(
        Entity,
        &TerrainBrushPreview,
        Option<&TerrainBrushPath>,
        &ConstructionPreview,
        Option<&TerrainEdit>,
    )>,
    mut prepared: MessageWriter<TerrainEditPrepared>,
    mut rejected: MessageWriter<TerrainEditPreparationRejected>,
) {
    for request in requests.read() {
        let Ok((preview_entity, preview, path, construction, live_edit)) =
            previews.get(request.preview)
        else {
            continue;
        };
        let cost = match construction.validity {
            PlacementValidity::Valid { cost } => cost,
            PlacementValidity::Invalid(reason) => {
                rejected.write(TerrainEditPreparationRejected {
                    transaction: request.transaction,
                    reason,
                });
                continue;
            }
            PlacementValidity::Pending => {
                rejected.write(TerrainEditPreparationRejected {
                    transaction: request.transaction,
                    reason: PlacementFailure::OutsideMap,
                });
                continue;
            }
        };
        if live_edit.is_some_and(|edit| !edit.deltas.is_empty()) {
            let transaction = request.transaction;
            commands.queue(move |world: &mut World| {
                let Some(edit) = world.entity_mut(preview_entity).take::<TerrainEdit>() else {
                    return;
                };
                world.entity_mut(transaction).insert(edit);
            });
            prepared.write(TerrainEditPrepared {
                transaction: request.transaction,
                cost,
            });
            continue;
        }
        if !terrain_brush_preview_and_path_are_valid(preview, path) {
            rejected.write(TerrainEditPreparationRejected {
                transaction: request.transaction,
                reason: PlacementFailure::OutsideMap,
            });
            continue;
        }

        let fallback_dab = TerrainBrushDab {
            center: preview.center,
            seconds: preview.seconds,
        };
        let dabs = path.map_or(core::slice::from_ref(&fallback_dab), |path| {
            path.0.as_slice()
        });
        let mut candidates = Vec::new();
        if index.chunk_span_m > 0.0 {
            let (minimum, maximum) = dabs.iter().fold(
                (Vec2::splat(f32::INFINITY), Vec2::splat(f32::NEG_INFINITY)),
                |(minimum, maximum), dab| {
                    (
                        minimum.min(dab.center - Vec2::splat(preview.radius_m)),
                        maximum.max(dab.center + Vec2::splat(preview.radius_m)),
                    )
                },
            );
            let min_coord = ((minimum - index.origin) / index.chunk_span_m)
                .floor()
                .as_ivec2();
            let max_coord = ((maximum - index.origin) / index.chunk_span_m)
                .floor()
                .as_ivec2();
            for z in min_coord.y..=max_coord.y {
                for x in min_coord.x..=max_coord.x {
                    let Some(entity) = index.chunks.get(&IVec2::new(x, z)).copied() else {
                        continue;
                    };
                    let Ok((_, chunk, edited, persistent)) = chunks.get(entity) else {
                        continue;
                    };
                    let span = chunk.spacing_m * f32::from(chunk.side.saturating_sub(1));
                    if dabs.iter().any(|dab| {
                        terrain_brush_circle_intersects_chunk_rectangle(
                            dab.center,
                            preview.radius_m,
                            chunk.origin,
                            chunk.origin + Vec2::splat(span),
                        )
                    }) {
                        candidates.push((persistent.0, chunk, edited));
                    }
                }
            }
        }
        candidates.sort_unstable_by_key(|candidate| candidate.0);

        let water_surface_heights_cm: Vec<_> = dabs.iter().map(|dab| {
            super::terrain_brush_sample_mutation::resolve_painted_water_surface_height_centimetres(
                preview, dab, candidates.iter().filter_map(|(_, chunk, edited)| {
                    Some((*chunk, assets.get(&chunk.asset)?, *edited))
                }),
            )
        }).collect();

        let mut deltas = Vec::with_capacity(candidates.len());
        let mut failure = None;
        for (persistent, chunk, edited) in candidates {
            let Some(asset) = assets.get(&chunk.asset) else {
                failure = Some(PlacementFailure::OutsideMap);
                break;
            };
            if let TerrainBrushKind::Paint {
                biome,
                ground_cover,
            } = preview.kind
            {
                if !paint_layers_are_available_for_terrain_brush(asset, edited, biome, ground_cover)
                {
                    failure = Some(PlacementFailure::AuthoredRule(biome));
                    break;
                }
            }
            if let TerrainBrushKind::PaintWater { biome, depth } = preview.kind {
                if !painted_water_depth_is_available_for_terrain_brush(asset, biome, depth) {
                    failure = Some(PlacementFailure::AuthoredRule(biome));
                    break;
                }
            }
            match prepare_terrain_chunk_sample_delta_from_brush_dabs(
                TerrainChunkId(persistent),
                chunk,
                asset,
                edited,
                preview,
                dabs,
                &water_surface_heights_cm,
            ) {
                Some(delta) => deltas.push(delta),
                None => {}
            }
        }

        if failure.is_some()
            || (deltas.is_empty()
                && !(preview.paint_trees || preview.paint_foliage || preview.paint_rocks))
        {
            rejected.write(TerrainEditPreparationRejected {
                transaction: request.transaction,
                reason: failure.unwrap_or(PlacementFailure::OutsideMap),
            });
            continue;
        }
        if deltas.is_empty() {
            continue;
        }

        commands
            .entity(request.transaction)
            .insert(TerrainEdit { deltas });
        prepared.write(TerrainEditPrepared {
            transaction: request.transaction,
            cost,
        });
    }
}
fn terrain_brush_preview_and_path_are_valid(
    preview: &TerrainBrushPreview,
    path: Option<&TerrainBrushPath>,
) -> bool {
    preview.center.is_finite()
        && preview.radius_m.is_finite()
        && preview.radius_m > 0.0
        && preview.strength_per_s.is_finite()
        && preview.seconds.is_finite()
        && preview.seconds > 0.0
        && path.is_none_or(|path| {
            !path.0.is_empty()
                && path.0.iter().all(|dab| {
                    dab.center.is_finite() && dab.seconds.is_finite() && dab.seconds > 0.0
                })
        })
}

fn terrain_brush_circle_intersects_chunk_rectangle(
    center: Vec2,
    radius: f32,
    min: Vec2,
    max: Vec2,
) -> bool {
    let closest = center.clamp(min, max);
    closest.distance_squared(center) <= radius * radius
}
