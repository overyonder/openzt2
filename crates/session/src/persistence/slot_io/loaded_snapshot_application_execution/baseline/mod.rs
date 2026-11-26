use super::*;

pub(super) fn decode_snapshot_operation(
    mut commands: Commands,
    mut operations: Query<(Entity, &mut SnapshotBytes), With<PendingSnapshotApplication>>,
    profile: Res<ProfileOptions>,
    mut scratch: ResMut<ReusableWorldSnapshotByteBuffer>,
) {
    let Ok((operation, mut bytes)) = operations.single_mut() else {
        return;
    };
    let decoded = decode_snapshot_for_world_restore(&bytes.0, profile.profile_identifier);
    scratch.snapshot_container_bytes = std::mem::take(&mut bytes.0);
    scratch.clear_logical_contents();
    let mut operation = commands.entity(operation);
    operation.remove::<SnapshotBytes>();
    match decoded {
        Ok(decoded) => {
            operation.insert((decoded, PrepareSnapshotBaseline));
        }
        Err(reason) => {
            operation.insert(SnapshotRestoreFailed::BeforeReplacement(reason));
        }
    }
}

pub(super) fn prepare_snapshot_baseline(
    mut commands: Commands,
    mut operations: Query<
        (Entity, &DecodedSnapshot, &mut PendingSnapshotApplication),
        With<PrepareSnapshotBaseline>,
    >,
    profile: Res<ProfileOptions>,
    preparation: SavedWorldSnapshotPreparation,
) {
    let Ok((operation, decoded, mut pending)) = operations.single_mut() else {
        return;
    };
    let result = if decoded.profile == profile.profile_identifier {
        preparation.prepare(
            &decoded.world,
            decoded.terrain.as_ref(),
            &mut pending.saved_world_document,
        )
    } else {
        Err(Failure::SnapshotProfileMismatch)
    };
    let mut operation = commands.entity(operation);
    operation.remove::<PrepareSnapshotBaseline>();
    match result {
        Ok(terrain) => {
            if let Some(terrain) = terrain {
                operation.insert(terrain);
            }
            operation.insert(AwaitingSavedWorldDocument);
        }
        Err(reason) => {
            operation.insert(SnapshotRestoreFailed::BeforeReplacement(reason));
        }
    }
}

pub(super) fn retire_world_once_saved_assets_are_ready(
    mut commands: Commands,
    operations: Query<(
        Entity,
        Ref<AwaitingSavedWorldDocument>,
        &DecodedSnapshot,
        &PendingSnapshotApplication,
        Option<&SnapshotTerrainAsset>,
    )>,
    server: Res<AssetServer>,
    mut documents: MessageReader<AssetEvent<WorldScenarioDocumentAsset>>,
    mut terrains: MessageReader<AssetEvent<TerrainAsset>>,
    mut failed_assets: MessageReader<UntypedAssetLoadFailedEvent>,
    profile: Res<ProfileOptions>,
    roots: Query<Entity, With<WorldRoot>>,
    mut unload: MessageWriter<UnloadWorld>,
    mut phase: ResMut<NextState<GamePhase>>,
) {
    let asset_failed = failed_assets.read().last().is_some();
    let Ok((operation, stage, decoded, pending, terrain)) = operations.single() else {
        documents.clear();
        terrains.clear();
        return;
    };
    let document = pending.saved_world_document.as_ref().map(Handle::id);
    let terrain = terrain.map(|terrain| terrain.0.id());
    let document_ready = documents.read().fold(false, |ready, event| {
        ready || document.is_some_and(|id| event.is_loaded_with_dependencies(id))
    });
    let terrain_ready = terrains.read().fold(false, |ready, event| {
        ready || terrain.is_some_and(|id| event.is_loaded_with_dependencies(id))
    });
    let failure = if decoded.profile != profile.profile_identifier {
        Some(Failure::SnapshotProfileMismatch)
    } else {
        if !stage.is_added() && !asset_failed && !document_ready && !terrain_ready {
            return;
        }
        let ids = document
            .map(bevy::asset::AssetId::untyped)
            .into_iter()
            .chain(terrain.map(bevy::asset::AssetId::untyped));
        if ids.clone().any(|id| {
            matches!(server.load_state(id), LoadState::Failed(_))
                || matches!(
                    server.recursive_dependency_load_state(id),
                    RecursiveDependencyLoadState::Failed(_)
                )
        }) {
            Some(Failure::UnknownAssetIdentifier)
        } else if ids
            .into_iter()
            .all(|id| server.is_loaded_with_dependencies(id))
        {
            None
        } else {
            return;
        }
    };

    let mut operation = commands.entity(operation);
    operation.remove::<AwaitingSavedWorldDocument>();
    if let Some(reason) = failure {
        operation.insert(SnapshotRestoreFailed::BeforeReplacement(reason));
        return;
    }
    for root in &roots {
        unload.write(UnloadWorld(root));
    }
    phase.set(GamePhase::Loading);
    operation.insert(AwaitingWorldRetirement);
}

pub(super) fn begin_snapshot_baseline_load(
    mut commands: Commands,
    operations: Query<
        (Entity, &DecodedSnapshot, &PendingSnapshotApplication),
        With<AwaitingWorldRetirement>,
    >,
    roots: Query<(), With<WorldRoot>>,
    mut loads: MessageWriter<BeginWorldLoad>,
) {
    let Ok((operation, decoded, pending)) = operations.single() else {
        return;
    };
    if !roots.is_empty() {
        return;
    }
    loads.write(BeginWorldLoad {
        saved_world_document: pending.saved_world_document.as_ref().map(Handle::id),
        mode_document: None,
        operation: Some(operation),
        scenario: decoded.world.scenario_definition_identifier,
        mode: decoded.world.session_mode,
        profile: decoded.profile,
        starting_cash_cents: None,
    });
    commands
        .entity(operation)
        .remove::<AwaitingWorldRetirement>()
        .insert(AwaitingSnapshotWorldLoad);
}

pub(super) fn receive_snapshot_baseline_result(
    mut commands: Commands,
    operations: Query<
        (
            Entity,
            &DecodedSnapshot,
            Option<
                &openzt2_zoo_state::economy::world_economy_configuration::WorldEconomyConfiguration,
            >,
        ),
        With<AwaitingSnapshotWorldLoad>,
    >,
    roots: Query<(Entity, &WorldLoadOperationOwner), With<WorldRoot>>,
    profile: Res<ProfileOptions>,
    mut arrived: MessageReader<WorldLoadFinished>,
    mut failed: MessageReader<WorldLoadFailed>,
) {
    let Ok((operation, decoded, economy)) = operations.single() else {
        arrived.clear();
        failed.clear();
        return;
    };
    if decoded.profile != profile.profile_identifier {
        let root = roots
            .iter()
            .find_map(|(root, owner)| (owner.0 == operation).then_some(root));
        commands
            .entity(operation)
            .remove::<AwaitingSnapshotWorldLoad>()
            .insert(SnapshotRestoreFailed::DuringReplacement {
                reason: Failure::SnapshotProfileMismatch,
                root_to_unload: root,
            });
        return;
    }
    let scenario = decoded.world.scenario_definition_identifier;
    let failed = failed
        .read()
        .any(|result| result.operation == Some(operation) && result.scenario == scenario);
    let root = arrived.read().find_map(|result| {
        (result.operation == Some(operation)
            && result.scenario == scenario
            && roots
                .get(result.root)
                .is_ok_and(|(_, owner)| owner.0 == operation))
        .then_some(result.root)
    });
    if failed {
        commands
            .entity(operation)
            .remove::<AwaitingSnapshotWorldLoad>()
            .insert(SnapshotRestoreFailed::DuringReplacement {
                reason: Failure::UnknownAssetIdentifier,
                root_to_unload: None,
            });
    } else if let Some(root) = root {
        if let Some(economy) = economy {
            commands.entity(root).insert(economy.clone());
        }
        let mut operation = commands.entity(operation);
        operation.remove::<AwaitingSnapshotWorldLoad>();
        if decoded.terrain.is_some() {
            operation.insert(RestoreSnapshotTerrain(root));
        } else {
            operation.insert(AwaitingSnapshotDomainBaseline(root));
        }
    }
}

impl RestoreSnapshotSections {
    fn new(root: Entity) -> Self {
        Self {
            root,
            prefabs: Vec::new(),
            behavior_documents: Vec::new(),
        }
    }
}

pub(super) fn restore_snapshot_terrain(
    mut commands: Commands,
    mut operations: Query<(Entity, &RestoreSnapshotTerrain, &mut DecodedSnapshot)>,
    mut terrain_application: TerrainSnapshotApplicationParameters,
) {
    let Ok((operation, stage, mut decoded)) = operations.single_mut() else {
        return;
    };
    let root = stage.0;
    let Some(terrain) = decoded.terrain.as_ref() else {
        commands
            .entity(operation)
            .remove::<RestoreSnapshotTerrain>()
            .insert(SnapshotRestoreFailed::DuringReplacement {
                reason: Failure::CorruptSnapshotSection,
                root_to_unload: Some(root),
            });
        return;
    };
    match terrain_application.apply(terrain, root, operation, &mut commands) {
        Ok(()) => {
            decoded.terrain = None;
            commands
                .entity(operation)
                .remove::<RestoreSnapshotTerrain>()
                .insert(AwaitingSnapshotTerrainNavigation { root });
        }
        Err(Failure::SnapshotTransactionInFlight) => {}
        Err(reason) => {
            commands
                .entity(operation)
                .remove::<RestoreSnapshotTerrain>()
                .insert(SnapshotRestoreFailed::DuringReplacement {
                    reason,
                    root_to_unload: Some(root),
                });
        }
    }
}

pub(super) fn accept_restored_terrain_navigation(
    mut commands: Commands,
    operations: Query<&AwaitingSnapshotTerrainNavigation>,
    mut ready: MessageReader<
        openzt2_zoo_state::terrain::terrain_navigation_change_types::SnapshotTerrainNavigationReady,
    >,
) {
    for completion in ready.read() {
        if operations
            .get(completion.operation)
            .is_ok_and(|stage| stage.root == completion.root)
        {
            commands
                .entity(completion.operation)
                .remove::<AwaitingSnapshotTerrainNavigation>()
                .insert(AwaitingSnapshotDomainBaseline(completion.root));
        }
    }
}

pub(super) fn accept_snapshot_domain_baseline(
    mut commands: Commands,
    operations: Query<(Entity, &AwaitingSnapshotDomainBaseline, &DecodedSnapshot)>,
    profile: Res<ProfileOptions>,
    allocators: Query<&PersistentIdAllocator, With<WorldRoot>>,
    photos: PhotoSnapshotBaselineReadiness,
    time: SimulationTimeSnapshotRestore,
) {
    let Ok((operation, stage, decoded)) = operations.single() else {
        return;
    };
    let root = stage.0;
    let readiness = if profile.profile_identifier != decoded.profile {
        Err(Failure::SnapshotProfileMismatch)
    } else if !time.is_initialized(root) {
        return;
    } else {
        allocators
            .get(root)
            .map_err(|_| Failure::BrokenPersistentEntityReference)
            .and_then(|allocator| photos.is_ready_for_snapshot_application(root, allocator))
    };
    let mut operation = commands.entity(operation);
    match readiness {
        Ok(false) => {}
        Ok(true) => {
            operation
                .remove::<AwaitingSnapshotDomainBaseline>()
                .insert(RestoreSnapshotSections::new(root));
        }
        Err(reason) => {
            operation.remove::<AwaitingSnapshotDomainBaseline>().insert(
                SnapshotRestoreFailed::DuringReplacement {
                    reason,
                    root_to_unload: Some(root),
                },
            );
        }
    }
}
