use super::*;

pub(super) fn snapshot_section_dependencies_changed(
    stages: Query<Ref<RestoreSnapshotSections>>,
    mut prefabs: MessageReader<AssetEvent<ScenePrefabAsset>>,
    mut models: MessageReader<AssetEvent<bevy::gltf::Gltf>>,
    mut behavior: MessageReader<AssetEvent<BehaviorDocumentAsset>>,
    mut scripts: MessageReader<AssetEvent<LuaScriptAsset>>,
    mut species_documents: MessageReader<AssetEvent<SpeciesAsset>>,
    mut failures: MessageReader<UntypedAssetLoadFailedEvent>,
    species: Res<SpeciesAssets>,
    scripts_index: Option<Res<LuaScriptArchivePathIndex>>,
    locomotion: Option<Res<LocomotionCapacity>>,
    profile: Res<ProfileOptions>,
) -> bool {
    let assets_changed = prefabs.read().last().is_some()
        | models.read().last().is_some()
        | behavior.read().last().is_some()
        | scripts.read().last().is_some()
        | species_documents.read().last().is_some()
        | failures.read().last().is_some();
    stages.single().is_ok_and(|stage| {
        stage.is_added()
            || assets_changed
            || species.is_changed()
            || scripts_index.is_some_and(|index| index.is_changed())
            || locomotion.is_some_and(|capacity| capacity.is_changed())
            || profile.is_changed()
    })
}

pub(super) fn apply_snapshot_sections(
    mut commands: Commands,
    mut operations: Query<(
        Entity,
        &PendingSnapshotApplication,
        &mut DecodedSnapshot,
        &mut RestoreSnapshotSections,
    )>,
    profile: Res<ProfileOptions>,
    mut allocators: Query<&mut PersistentIdAllocator, With<WorldRoot>>,
    application: WorldSnapshotApplicationState,
    progression: ProgressionSnapshotApplicationParameters,
) {
    let Ok((operation, pending, mut decoded, mut stage)) = operations.single_mut() else {
        return;
    };
    let stage = &mut *stage;
    let root = stage.root;
    let result = if decoded.profile != profile.profile_identifier {
        Err(Failure::SnapshotProfileMismatch)
    } else if let Ok(mut allocator) = allocators.get_mut(root) {
        restore_snapshot_sections(
            &mut commands,
            &mut decoded,
            pending.source,
            root,
            &mut allocator,
            application,
            &progression,
            &mut stage.prefabs,
            &mut stage.behavior_documents,
        )
    } else {
        Err(Failure::BrokenPersistentEntityReference)
    };
    let mut operation = commands.entity(operation);
    match result {
        Ok(None) => {}
        Ok(Some(spawns)) => {
            let Some(sections) = decoded.sections.as_mut() else {
                operation.remove::<RestoreSnapshotSections>().insert(
                    SnapshotRestoreFailed::DuringReplacement {
                        reason: Failure::CorruptSnapshotSection,
                        root_to_unload: Some(root),
                    },
                );
                return;
            };
            operation.remove::<RestoreSnapshotSections>().insert((
                RestoreTransportIdentities,
                RestoreSavedFenceEdges(spawns.retired_fence_bounds),
                AttachSavedWorldState {
                    root,
                    behavior_documents: std::mem::take(&mut stage.behavior_documents),
                },
                AttachSnapshotEconomy(sections.economy.take()),
                AttachSnapshotMaintenance(std::mem::take(&mut sections.maintenance.objects)),
                AttachSnapshotDonations(std::mem::take(&mut sections.donations)),
            ));
        }
        Err(reason) => {
            operation.remove::<RestoreSnapshotSections>().insert(
                SnapshotRestoreFailed::DuringReplacement {
                    reason,
                    root_to_unload: Some(root),
                },
            );
        }
    }
}

pub(super) fn apply_saved_fence_edges(
    mut commands: Commands,
    operations: Query<(
        Entity,
        &DecodedSnapshot,
        &AttachSavedWorldState,
        &RestoreSavedFenceEdges,
    )>,
    index: Res<PersistentIdIndex>,
    nodes: Query<&TopologyNode>,
    grid: Res<TopologyGrid>,
) {
    let Ok((operation, decoded, stage, saved)) = operations.single() else {
        return;
    };
    let root = stage.root;
    let mut operation_commands = commands.entity(operation);
    operation_commands.remove::<RestoreSavedFenceEdges>();
    if let Some(records) = decoded
        .sections
        .as_ref()
        .and_then(|records| records.topology.as_deref())
    {
        if let Err(reason) = restore_snapshot_fence_edges(
            &mut commands,
            records,
            root,
            &index,
            &nodes,
            *grid,
            saved.0,
        ) {
            commands
                .entity(operation)
                .insert(SnapshotRestoreFailed::DuringReplacement {
                    reason,
                    root_to_unload: Some(root),
                });
        }
    }
}

pub(super) fn prepare_transport_identities(
    mut commands: Commands,
    operations: Query<
        (Entity, &DecodedSnapshot, &AttachSavedWorldState),
        With<RestoreTransportIdentities>,
    >,
    mut allocations: Query<&mut PersistentIdAllocator, With<WorldRoot>>,
    baseline: SavedTransportEntities,
    index: Res<PersistentIdIndex>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    definitions: Res<WorldDefinitions>,
) {
    let Ok((operation, decoded, stage)) = operations.single() else {
        return;
    };
    let root = stage.root;
    let result = (|| {
        let records = decoded
            .sections
            .as_ref()
            .ok_or(Failure::CorruptSnapshotSection)?;
        let definitions = definitions
            .get(&assets)
            .ok_or(Failure::UnknownAssetIdentifier)?;
        let mut allocator = allocations
            .get_mut(root)
            .map_err(|_| Failure::BrokenPersistentEntityReference)?;
        spawn_missing_transport_identities(
            &mut commands,
            &records.transport,
            definitions,
            root,
            &index,
            &baseline,
            &mut allocator,
        )
    })();
    let mut operation = commands.entity(operation);
    operation.remove::<RestoreTransportIdentities>();
    if let Err(reason) = result {
        operation.remove::<AttachSavedWorldState>().insert(
            SnapshotRestoreFailed::DuringReplacement {
                reason,
                root_to_unload: Some(root),
            },
        );
    }
}

pub(super) fn restore_transport_state(
    mut commands: Commands,
    mut operations: Query<(
        Entity,
        &RestoreRuntimeContinuation,
        &mut RestoreTransportState,
    )>,
    index: Res<PersistentIdIndex>,
    mut transport: Query<RestoredTransportComponents>,
) {
    let Ok((operation, stage, mut saved)) = operations.single_mut() else {
        return;
    };
    let root = stage.actors.root;
    commands.entity(operation).remove::<RestoreTransportState>();
    let result = restore_transport_sections(
        &mut commands,
        std::mem::take(&mut saved.records),
        root,
        &index,
        &mut transport,
    );
    if let Err(reason) = result {
        commands
            .entity(operation)
            .remove::<RestoreRuntimeContinuation>()
            .insert(SnapshotRestoreFailed::DuringReplacement {
                reason,
                root_to_unload: Some(root),
            });
    }
}

pub(super) fn attach_saved_economy(
    mut commands: Commands,
    mut operations: Query<(Entity, &AttachSavedWorldState, &mut AttachSnapshotEconomy)>,
    index: Res<PersistentIdIndex>,
    mut facilities: Query<RestoredFacilityEconomy>,
    mut cash: ResMut<ZooCash>,
    mut price: ResMut<AdmissionPrice>,
    mut admissions: ResMut<ZooAdmissionsOpen>,
    mut history: ResMut<MonthlyFinanceHistory>,
) {
    let Ok((operation, stage, mut saved)) = operations.single_mut() else {
        return;
    };
    let root = stage.root;
    let Some(saved) = saved.0.take() else {
        return;
    };
    let result = apply_economy_snapshot_records_to_live_economy_resources_and_facilities(
        &mut commands,
        saved,
        &index,
        root,
        &mut facilities,
        &mut cash,
        &mut price,
        &mut admissions,
        &mut history,
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

pub(super) fn attach_saved_maintenance(
    mut commands: Commands,
    operations: Query<(Entity, &AttachSavedWorldState, &AttachSnapshotMaintenance)>,
    index: Res<PersistentIdIndex>,
    mut objects: Query<RestoredMaintenanceComponents>,
) {
    let Ok((operation, stage, saved)) = operations.single() else {
        return;
    };
    let root = stage.root;
    if let Err(reason) =
        apply_maintained_object_records(&mut commands, &saved.0, &index, root, &mut objects)
    {
        commands
            .entity(operation)
            .insert(SnapshotRestoreFailed::DuringReplacement {
                reason,
                root_to_unload: Some(root),
            });
    }
}

pub(super) fn attach_saved_donations(
    mut commands: Commands,
    mut operations: Query<(Entity, &AttachSavedWorldState, &mut AttachSnapshotDonations)>,
    index: Res<PersistentIdIndex>,
    mut acceptors: Query<RestoredDonationComponents>,
) {
    let Ok((operation, stage, mut saved)) = operations.single_mut() else {
        return;
    };
    let root = stage.root;
    if let Err(reason) = restore_donation_totals(
        &mut commands,
        std::mem::take(&mut saved.0),
        &index,
        root,
        &mut acceptors,
    ) {
        commands
            .entity(operation)
            .insert(SnapshotRestoreFailed::DuringReplacement {
                reason,
                root_to_unload: Some(root),
            });
    }
}

pub(super) fn distribute_saved_snapshot_sections(
    mut commands: Commands,
    mut operations: Query<
        (Entity, &mut DecodedSnapshot, &mut AttachSavedWorldState),
        Without<SnapshotRestoreFailed>,
    >,
) {
    use super::super::supported_snapshot_section_validation::SnapshotRestoreSections;

    let Ok((operation, mut decoded, mut stage)) = operations.single_mut() else {
        return;
    };
    let root = stage.root;
    let mut operation = commands.entity(operation);
    operation.remove::<(
        AttachSavedWorldState,
        AttachSnapshotDonations,
        AttachSnapshotEconomy,
        AttachSnapshotMaintenance,
    )>();
    let Some(SnapshotRestoreSections {
        runtime,
        adoption,
        animals,
        placements,
        time,
        scenario,
        progression,
        photos,
        photo_challenges,
        shows,
        transport,
        ..
    }) = decoded.sections.take()
    else {
        operation.insert(SnapshotRestoreFailed::DuringReplacement {
            reason: Failure::CorruptSnapshotSection,
            root_to_unload: Some(root),
        });
        return;
    };
    let placement_identifiers = placements
        .unwrap_or_default()
        .into_iter()
        .map(|record| record.persistent_identifier)
        .collect::<Vec<_>>();
    operation.insert((
        RestoreTransportState { records: transport },
        SnapshotSectionRestore(Some(time)),
        SnapshotSectionRestore(Some(adoption)),
        SnapshotSectionRestore(Some(animals)),
        SnapshotSectionRestore(Some(photos)),
        SnapshotSectionRestore(Some(shows)),
        SnapshotSectionRestore(Some(progression)),
        SnapshotSectionRestore(Some(scenario)),
        RestoreRuntimeContinuation {
            record: runtime,
            photo_challenges,
            actors: ResumeSnapshotActors {
                root,
                placement_identifiers,
                behavior_documents: std::mem::take(&mut stage.behavior_documents),
            },
        },
    ));
}

pub(super) fn restore_research_projects(
    mut commands: Commands,
    mut operations: Query<(
        Entity,
        &RestoreRuntimeContinuation,
        &mut SavedResearchProjects,
    )>,
    identities: Res<PersistentIdIndex>,
    mut allocations: Query<&mut PersistentIdAllocator, With<WorldRoot>>,
    research_items: Query<&WorldResearchItems>,
    mut active_research: Query<(&mut ResearchProjectPersistentId, &mut ResearchProject)>,
) {
    let Ok((operation, stage, mut saved)) = operations.single_mut() else {
        return;
    };
    let root = stage.actors.root;
    let result = allocations
        .get_mut(root)
        .map_err(|_| Failure::BrokenPersistentEntityReference)
        .and_then(|mut allocator| {
            restore_saved_research_projects(
                &mut commands,
                &mut saved,
                root,
                &identities,
                &mut allocator,
                &research_items,
                &mut active_research,
            )
        });
    let mut operation = commands.entity(operation);
    operation.remove::<SavedResearchProjects>();
    if let Err(reason) = result {
        operation.insert(SnapshotRestoreFailed::DuringReplacement {
            reason,
            root_to_unload: Some(root),
        });
    }
}
