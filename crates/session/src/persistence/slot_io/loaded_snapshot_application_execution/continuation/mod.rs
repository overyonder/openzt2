use super::*;
use crate::persistence::slot_io::fossil_continuation_snapshot::RestoreFossilContinuation;

pub(super) fn restore_fossil_continuation(
    mut commands: Commands,
    operations: Query<(Entity, &RestoreRuntimeContinuation), Without<SnapshotRestoreFailed>>,
    mut fossils: RestoreFossilContinuation,
) {
    let Ok((operation, stage)) = operations.single() else {
        return;
    };
    let Some(snapshot) = stage
        .record
        .as_ref()
        .and_then(|record| record.domain_continuation.as_ref())
        .and_then(|domain| domain.fossils.as_ref())
    else {
        return;
    };
    let root = stage.actors.root;
    if let Err(reason) = fossils.restore(&mut commands, root, snapshot) {
        commands
            .entity(operation)
            .insert(SnapshotRestoreFailed::DuringReplacement {
                reason,
                root_to_unload: Some(root),
            });
    }
}

pub(super) fn restore_runtime_continuation(
    mut commands: Commands,
    operations: Query<(Entity, &RestoreRuntimeContinuation), Without<SnapshotRestoreFailed>>,
    mut runtime: RuntimeContinuationApplication,
) {
    let Ok((operation, stage)) = operations.single() else {
        return;
    };
    let root = stage.actors.root;
    if let Err(reason) = runtime.apply(&mut commands, root, stage.record.as_ref()) {
        commands
            .entity(operation)
            .insert(SnapshotRestoreFailed::DuringReplacement {
                reason,
                root_to_unload: Some(root),
            });
    }
}

pub(super) fn restore_snapshot_names(
    mut commands: Commands,
    operations: Query<&RestoreRuntimeContinuation, Without<SnapshotRestoreFailed>>,
    mut names: SavedEntityNames,
) {
    let Ok(stage) = operations.single() else {
        return;
    };
    if let Some(names_to_restore) = stage
        .record
        .as_ref()
        .and_then(|record| record.entity_names.as_deref())
    {
        restore_saved_entity_names(
            &mut commands,
            &mut names,
            stage.actors.root,
            names_to_restore,
        );
    }
}

pub(super) fn restore_snapshot_name_registration(
    mut commands: Commands,
    operations: Query<&RestoreRuntimeContinuation, Without<SnapshotRestoreFailed>>,
    members: SavedNameRegistration,
) {
    let Ok(stage) = operations.single() else {
        return;
    };
    let registered = stage
        .record
        .as_ref()
        .and_then(|record| record.registered_entity_name_identifiers.as_deref());
    restore_entity_name_registration(&mut commands, &members, stage.actors.root, registered);
}

pub(super) fn restore_snapshot_audio(
    mut commands: Commands,
    operations: Query<(Entity, &RestoreRuntimeContinuation), Without<SnapshotRestoreFailed>>,
    mut current: Query<
        Option<&mut openzt2_zoo_state::audio::audio_environment_types::WorldAudioPreset>,
    >,
    mut assets: Option<ResMut<Assets<openzt2_game_assets::audio::audio_asset_types::AudioAsset>>>,
) {
    let Ok((operation, stage)) = operations.single() else {
        return;
    };
    let root = stage.actors.root;
    let Some(record) = &stage.record else {
        return;
    };
    let result = restore_world_audio_preset(
        &mut commands,
        root,
        record.world_audio_preset.clone(),
        &mut current,
        assets.as_deref_mut(),
    );
    if let Err(reason) = result {
        commands
            .entity(operation)
            .insert(SnapshotRestoreFailed::DuringReplacement {
                reason,
                root_to_unload: Some(root),
            });
    }
}

pub(super) fn restore_photo_challenge_completion(
    mut commands: Commands,
    operations: Query<(Entity, &RestoreRuntimeContinuation), Without<SnapshotRestoreFailed>>,
    identities: Res<PersistentIdIndex>,
    catalogues: Query<&PhotoChallengeCatalogueIndex, With<WorldRoot>>,
    mut challenges: Query<&mut PhotoChallengeProgress>,
) {
    let Ok((operation, stage)) = operations.single() else {
        return;
    };
    let root = stage.actors.root;
    if let Err(reason) = restore_snapshot_photo_challenge_completion(
        &stage.photo_challenges,
        root,
        &identities,
        catalogues.get(root).ok(),
        &mut challenges,
    ) {
        commands
            .entity(operation)
            .insert(SnapshotRestoreFailed::DuringReplacement {
                reason,
                root_to_unload: Some(root),
            });
    }
}

pub(super) fn runtime_continuation_is_restored(
    mut commands: Commands,
    mut operations: Query<
        (Entity, &mut RestoreRuntimeContinuation),
        Without<SnapshotRestoreFailed>,
    >,
) {
    let Ok((operation, mut stage)) = operations.single_mut() else {
        return;
    };
    commands
        .entity(operation)
        .remove::<RestoreRuntimeContinuation>()
        .insert(ResumeSnapshotActors {
            root: stage.actors.root,
            placement_identifiers: std::mem::take(&mut stage.actors.placement_identifiers),
            behavior_documents: std::mem::take(&mut stage.actors.behavior_documents),
        });
}

#[derive(SystemParam)]
pub(super) struct SnapshotPlacementReadiness<'w, 's> {
    persistent_ids: Res<'w, PersistentIdIndex>,
    active_world_definitions: Res<'w, WorldDefinitions>,
    world_definition_assets: Res<'w, Assets<WorldDefinitionAsset>>,
    pub(super) placement_footprints: Option<Res<'w, openzt2_zoo_state::placement::object_placement_geometry::ScenePlacementFootprints>>,
    pub(super) placement_readiness: Query<
        'w,
        's,
        (
            &'static PersistentId,
            &'static DefinitionId,
            &'static Transform,
            Option<&'static openzt2_world_state::world_spawn::prefab_source_asset_handle::PrefabSourceAssetHandle>,
            Has<openzt2_zoo_state::placement::placed_object_types::PlacedObjectDefinitionReference>,
            Has<openzt2_zoo_state::donations::donation_opportunity_types::DonationUsePoint>,
        ),
    >,
}

pub(super) fn resume_snapshot_actors(
    mut commands: Commands,
    mut operations: Query<(Entity, &DecodedSnapshot, &mut ResumeSnapshotActors)>,
    profile: Res<ProfileOptions>,
    placement: SnapshotPlacementReadiness,
    mut behavior: BehaviorSnapshotApplication,
) {
    let Ok((operation, decoded, mut stage)) = operations.single_mut() else {
        return;
    };
    let root = stage.root;
    let result = (|| {
        if decoded.profile != profile.profile_identifier {
            return Err(Failure::SnapshotProfileMismatch);
        }
        let Some(definitions) = placement
            .active_world_definitions
            .get(&placement.world_definition_assets)
        else {
            return Ok(false);
        };
        stage.placement_identifiers.retain(|id| {
            !placement
                .persistent_ids
                .get(root, PersistentId(*id))
                .and_then(|entity| placement.placement_readiness.get(entity).ok())
                .is_some_and(|(candidate, definition, transform, prefab, placed, dock)| {
                    candidate.0 == *id
                        && super::super::placement_snapshot_section::placement_hydration_complete(
                            definitions,
                            definition.0,
                            transform,
                            prefab.map(|prefab| &prefab.0),
                            placement.placement_footprints.as_deref(),
                            placed,
                            dock,
                        )
                })
        });
        if !stage.placement_identifiers.is_empty() {
            return Ok(false);
        }
        restore_behavior_actor_snapshots(
            &mut commands,
            &decoded.behavior,
            &mut behavior,
            &mut stage.behavior_documents,
        )
    })();
    let mut operation = commands.entity(operation);
    match result {
        Ok(false) => {}
        Ok(true) => {
            operation
                .remove::<ResumeSnapshotActors>()
                .insert(SnapshotRestored(root));
        }
        Err(reason) => {
            operation.remove::<ResumeSnapshotActors>().insert(
                SnapshotRestoreFailed::DuringReplacement {
                    reason,
                    root_to_unload: Some(root),
                },
            );
        }
    }
}
