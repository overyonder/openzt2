use bevy::prelude::*;

use crate::{
    assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset,
    plugins::construction::construction_transaction_types::EditApplication,
};

use super::{
    terrain_change_tracking_types::{TerrainDirty, TerrainDirtyFlags},
    terrain_chunk_identity_type::TerrainChunkId,
    terrain_chunk_types::{EditedTerrainSamples, TerrainChunk},
    terrain_collision_rebuilding_types::TerrainCollisionPending,
    terrain_edit_data_types::TerrainEditDelta,
    terrain_edit_rectangle_operations::{
        apply_terrain_sample_rectangle, materialize_complete_edited_terrain_sample_overlay,
        terrain_sample_overlay_has_expected_lengths, terrain_sample_rectangle_length,
    },
    terrain_edit_types::{
        CommitTerrainEdit, TerrainChanged, TerrainEdit, TerrainEditAcknowledged,
        TerrainWaterChanged,
    },
    terrain_navigation_change_types::TerrainNavigationChanged,
    terrain_sample_change_calculations::{
        determine_terrain_dirty_flags_from_changed_samples,
        merge_terrain_dirty_sample_rectangle_and_advance_revision,
    },
    terrain_sample_grid_queries::read_materialized_terrain_sample,
};
pub(super) fn apply_committed_terrain_sample_deltas_and_publish_changed_regions(
    mut commands: Commands,
    mut requests: MessageReader<CommitTerrainEdit>,
    edits: Query<&TerrainEdit>,
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
        let Ok(edit) = edits.get(request.transaction) else {
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
        );
        let mut targets = Vec::with_capacity(edit.deltas.len());
        let mut valid = true;
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
