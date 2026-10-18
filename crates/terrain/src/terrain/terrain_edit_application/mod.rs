use bevy::prelude::*;

use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_zoo_state::construction::construction_transaction_types::EditApplication;

use crate::terrain::terrain_change_tracking_types::TerrainDirty;
use crate::terrain::terrain_change_tracking_types::TerrainDirtyFlags;
use crate::terrain::terrain_sample_change_calculations::determine_terrain_dirty_flags_from_changed_samples;
use crate::terrain::terrain_sample_change_calculations::merge_terrain_dirty_sample_rectangle_and_advance_revision;
use openzt2_zoo_state::terrain::terrain_chunk_identity_type::TerrainChunkId;
use openzt2_zoo_state::terrain::terrain_chunk_types::EditedTerrainSamples;
use openzt2_zoo_state::terrain::terrain_chunk_types::TerrainChunk;
use openzt2_zoo_state::terrain::terrain_collision_rebuilding_types::TerrainCollisionPending;
use openzt2_zoo_state::terrain::terrain_edit_data_types::TerrainEditDelta;
use openzt2_zoo_state::terrain::terrain_edit_rectangle_operations::apply_terrain_sample_rectangle;
use openzt2_zoo_state::terrain::terrain_edit_rectangle_operations::materialize_complete_edited_terrain_sample_overlay;
use openzt2_zoo_state::terrain::terrain_edit_rectangle_operations::terrain_sample_overlay_has_expected_lengths;
use openzt2_zoo_state::terrain::terrain_edit_rectangle_operations::terrain_sample_rectangle_length;
use openzt2_zoo_state::terrain::terrain_edit_types::CommitTerrainEdit;
use openzt2_zoo_state::terrain::terrain_edit_types::TerrainChanged;
use openzt2_zoo_state::terrain::terrain_edit_types::TerrainEdit;
use openzt2_zoo_state::terrain::terrain_edit_types::TerrainEditAcknowledged;
use openzt2_zoo_state::terrain::terrain_edit_types::TerrainWaterChanged;
use openzt2_zoo_state::terrain::terrain_navigation_change_types::TerrainNavigationChanged;
use openzt2_zoo_state::terrain::terrain_sample_grid_queries::read_materialized_terrain_sample;
pub(super) fn apply_committed_terrain_sample_deltas_and_publish_changed_regions(
    mut commands: Commands,
    mut requests: MessageReader<CommitTerrainEdit>,
    edits: Query<(
        &TerrainEdit,
        Option<&openzt2_zoo_state::construction::construction_transaction_types::RollbackContext>,
    )>,
    entities: Query<Entity>,
    mut chunks: Query<(
        Entity,
        &TerrainChunk,
        Option<&mut EditedTerrainSamples>,
        Option<&mut TerrainDirty>,
        &TerrainChunkId,
    )>,
    assets: Res<Assets<TerrainAsset>>,
    mut changed: MessageWriter<TerrainChanged>,
    mut navigation_changed: MessageWriter<TerrainNavigationChanged>,
    mut water_changed: MessageWriter<TerrainWaterChanged>,
    mut acknowledged: MessageWriter<TerrainEditAcknowledged>,
) {
    for request in requests.read() {
        let Ok((edit, rollback)) = edits.get(request.transaction) else {
            acknowledged.write(TerrainEditAcknowledged {
                transaction: request.transaction,
                application: request.application,
                accepted: false,
            });
            continue;
        };

        let use_after = matches!(
            request.application,
            EditApplication::InitialCommit | EditApplication::Redo
        ) || (request.application == EditApplication::FailureRollback
            && rollback.is_some_and(|context| context.failed_application == EditApplication::Undo));
        let mut targets = Vec::with_capacity(edit.deltas.len());
        let mut valid = edit.water_regions.iter().all(|delta| {
            entities.contains(delta.owner)
                && [&delta.before, &delta.after]
                    .into_iter()
                    .flatten()
                    .all(|state| {
                        state.surface_height_metres.is_finite()
                            && state.depth_ratio.is_finite()
                            && (0.0..=1.0).contains(&state.depth_ratio)
                            && assets.get(&state.terrain).is_some_and(|asset| {
                                state.biome_index < asset.canonical_terrain_grid().biomes.len()
                            })
                    })
        });
        for delta in edit.deltas.iter() {
            let Some((entity, chunk, edited, _, _)) = chunks
                .iter_mut()
                .find(|(_, _, _, _, id)| **id == delta.chunk)
            else {
                valid = false;
                break;
            };
            let expected = chunk.side as usize * chunk.side as usize;
            let area = terrain_sample_rectangle_length(delta.min, delta.max);
            if area != Some(delta.before.len()) || delta.before.len() != delta.after.len() {
                valid = false;
                break;
            }
            let Some(asset) = assets.get(&chunk.asset) else {
                valid = false;
                break;
            };
            if edited.as_deref().is_some_and(|samples| {
                !terrain_sample_overlay_has_expected_lengths(samples, expected)
            }) || (edited.is_none()
                && !(0..chunk.side as usize).all(|z| {
                    (0..chunk.side as usize).all(|x| {
                        read_materialized_terrain_sample(chunk, asset, None, x, z).is_some()
                    })
                }))
            {
                valid = false;
                break;
            }
            targets.push(entity);
        }
        if !valid {
            acknowledged.write(TerrainEditAcknowledged {
                transaction: request.transaction,
                application: request.application,
                accepted: false,
            });
            continue;
        }

        let owner_changes: Box<[_]> = if use_after {
            edit.water_regions
                .iter()
                .map(|delta| (delta.owner, delta.after.clone(), delta.after_active))
                .collect()
        } else {
            edit.water_regions
                .iter()
                .rev()
                .map(|delta| (delta.owner, delta.before.clone(), delta.before_active))
                .collect()
        };
        apply_water_region_edit_changes(&mut commands, owner_changes);

        if use_after {
            for (entity, delta) in targets.into_iter().zip(edit.deltas.iter()) {
                apply_validated_terrain_edit_delta_and_publish_changed_region(
                    &mut commands,
                    &mut chunks,
                    &assets,
                    request.transaction,
                    entity,
                    delta,
                    true,
                    &mut changed,
                    &mut navigation_changed,
                    &mut water_changed,
                );
            }
        } else {
            for (entity, delta) in targets.into_iter().zip(edit.deltas.iter()).rev() {
                apply_validated_terrain_edit_delta_and_publish_changed_region(
                    &mut commands,
                    &mut chunks,
                    &assets,
                    request.transaction,
                    entity,
                    delta,
                    false,
                    &mut changed,
                    &mut navigation_changed,
                    &mut water_changed,
                );
            }
        }
        acknowledged.write(TerrainEditAcknowledged {
            transaction: request.transaction,
            application: request.application,
            accepted: true,
        });
    }
}

pub(super) fn apply_water_region_edit_changes(
    commands: &mut Commands,
    changes: Box<[(Entity, Option<openzt2_zoo_state::terrain::terrain_water_presentation_types::TerrainWaterRegionEdit>, bool)]>,
) {
    commands.queue(move |world: &mut World| {
        for (owner, state, active) in changes {
            let Ok(mut entity) = world.get_entity_mut(owner) else { continue; };
            if let Some(state) = state {
                entity.insert(state);
            } else {
                entity.remove::<openzt2_zoo_state::terrain::terrain_water_presentation_types::TerrainWaterRegionEdit>();
            }
            if active {
                entity.remove::<openzt2_zoo_state::terrain::terrain_water_presentation_types::TerrainWaterRegionRetired>();
            } else {
                entity.insert(openzt2_zoo_state::terrain::terrain_water_presentation_types::TerrainWaterRegionRetired);
            }
        }
    });
}

#[allow(clippy::too_many_arguments)]
pub(super) fn apply_validated_terrain_edit_delta_and_publish_changed_region(
    commands: &mut Commands,
    chunks: &mut Query<(
        Entity,
        &TerrainChunk,
        Option<&mut EditedTerrainSamples>,
        Option<&mut TerrainDirty>,
        &TerrainChunkId,
    )>,
    assets: &Assets<TerrainAsset>,
    transaction: Entity,
    entity: Entity,
    delta: &TerrainEditDelta,
    use_after: bool,
    changed: &mut MessageWriter<TerrainChanged>,
    navigation_changed: &mut MessageWriter<TerrainNavigationChanged>,
    water_changed: &mut MessageWriter<TerrainWaterChanged>,
) {
    let Ok((_, chunk, existing, dirty, _)) = chunks.get_mut(entity) else {
        return;
    };
    let Some(asset) = assets.get(&chunk.asset) else {
        return;
    };
    let samples = if use_after {
        &delta.after
    } else {
        &delta.before
    };
    let flags = determine_terrain_dirty_flags_from_changed_samples(&delta.before, &delta.after);
    if let Some(mut overlay) = existing {
        apply_terrain_sample_rectangle(&mut overlay, chunk.side, delta.min, delta.max, samples);
    } else {
        let mut overlay = materialize_complete_edited_terrain_sample_overlay(chunk, asset);
        apply_terrain_sample_rectangle(&mut overlay, chunk.side, delta.min, delta.max, samples);
        commands.entity(entity).insert(overlay);
    }
    if let Some(mut dirty) = dirty {
        *dirty = merge_terrain_dirty_sample_rectangle_and_advance_revision(
            Some(*dirty),
            delta.min,
            delta.max,
            flags,
        );
    } else {
        commands
            .entity(entity)
            .insert(merge_terrain_dirty_sample_rectangle_and_advance_revision(
                None, delta.min, delta.max, flags,
            ));
    }
    if flags.contains(TerrainDirtyFlags::COLLISION) {
        commands.entity(entity).insert(TerrainCollisionPending);
    }
    changed.write(TerrainChanged {
        transaction,
        chunk: entity,
        min: delta.min,
        max: delta.max,
    });
    if flags.contains(TerrainDirtyFlags::COLLISION) {
        navigation_changed.write(TerrainNavigationChanged {
            chunk: entity,
            min: delta.min,
            max: delta.max,
        });
    }
    if flags.contains(TerrainDirtyFlags::WATER) {
        water_changed.write(TerrainWaterChanged {
            chunk: entity,
            min: delta.min,
            max: delta.max,
        });
    }
}
