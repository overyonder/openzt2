use bevy::prelude::*;
use openzt2_world_state::world_spawn::selected_world_identity::SelectedWorldIdentity;
use openzt2_world_state::world_spawn::{
    persistent_id_index::PersistentIdIndex, persistent_id_types::PersistentId,
    persistent_id_types::PersistentIdAllocator, world_membership_types::WorldRoot,
};
use openzt2_zoo_state::shows::{
    show_platform_upgrade_types::RestoreShowPlatformUpgrade,
    show_stage_types::{ShowName, ShowStage, ShowStageOpenState},
};

use super::super::{
    animal_adoption_offer_inventory_snapshot_section::{
        spawn_saved_adoption_offers, AnimalAdoptionOffersSnapshot,
    },
    animal_snapshot_application::restore_animal_relationships,
    animal_snapshot_types::AnimalSnapshotRecord,
    native_crate_snapshot_links::restore_crate_relationships,
    pending_snapshot_application_types::{
        RestoreRuntimeContinuation, SnapshotRestoreFailed, SnapshotSectionRestore,
    },
    photo_snapshot_application::spawn_snapshot_photos,
    photo_snapshot_types::PhotoSnapshotRecord,
    scenario_snapshot_section::{self, ScenarioSnapshot, ScenarioSnapshotApplication},
    show_snapshot_application::apply_show_snapshot_records_to_live_show_stages_and_schedule,
    show_snapshot_types::ShowSnapshotRecords,
    supported_snapshot_section_validation::DecodedSnapshot,
};
use crate::persistence::{
    persistence_failure_types::WorldSnapshotPersistenceFailure as Failure,
    progression_snapshot_application::{
        apply_progression_snapshot_records_to_live_world, ProgressionSnapshotApplicationParameters,
    },
    progression_snapshot_types::ProgressionSnapshotRecords,
    simulation_time_snapshot_section::{
        SimulationTimeSnapshotLoadResources, SimulationTimeSnapshotRecord,
    },
};

pub(super) type PendingSection<'w, 's, T> = Query<
    'w,
    's,
    (
        Entity,
        &'static RestoreRuntimeContinuation,
        &'static mut SnapshotSectionRestore<T>,
    ),
    Without<SnapshotRestoreFailed>,
>;

fn fail_section(commands: &mut Commands, operation: Entity, root: Entity, reason: Failure) {
    commands
        .entity(operation)
        .insert(SnapshotRestoreFailed::DuringReplacement {
            reason,
            root_to_unload: Some(root),
        });
}

pub(super) fn restore_snapshot_clock(
    mut commands: Commands,
    mut operations: PendingSection<SimulationTimeSnapshotRecord>,
    mut time: SimulationTimeSnapshotLoadResources,
) {
    let Ok((operation, stage, mut saved)) = operations.single_mut() else {
        return;
    };
    let root = stage.actors.root;
    if let Some(record) = saved.0.take() {
        if let Err(reason) =
            record.apply_simulation_time_snapshot_record_to_live_resources(&mut time)
        {
            fail_section(&mut commands, operation, root, reason);
        }
    }
}

pub(super) fn restore_world_metadata(
    mut commands: Commands,
    operations: Query<
        (Entity, &DecodedSnapshot, &RestoreRuntimeContinuation),
        Without<SnapshotRestoreFailed>,
    >,
    mut roots: Query<(Option<&mut Name>, Option<&mut SelectedWorldIdentity>), With<WorldRoot>>,
) {
    use super::super::restored_component_values::restore_optional_component;
    let Ok((operation, decoded, stage)) = operations.single() else {
        return;
    };
    let root = stage.actors.root;
    let Ok((name, identity)) = roots.get_mut(root) else {
        fail_section(
            &mut commands,
            operation,
            root,
            Failure::BrokenPersistentEntityReference,
        );
        return;
    };
    let saved = &decoded.world;
    let mut root = commands.entity(root);
    restore_optional_component(
        &mut root,
        name,
        Some(Name::new(saved.display_name.clone())),
        false,
    );
    restore_optional_component(
        &mut root,
        identity,
        Some(SelectedWorldIdentity {
            requested: saved.requested_world_definition_identifier,
            map: saved.map_definition_identifier,
            start: saved.starting_point_definition_identifier,
            mode: saved.session_mode,
            profile: decoded.profile,
        }),
        false,
    );
}

pub(super) fn restore_snapshot_animal_links(
    mut commands: Commands,
    mut operations: PendingSection<Vec<AnimalSnapshotRecord>>,
    identities: Res<PersistentIdIndex>,
) {
    let Ok((operation, stage, mut saved)) = operations.single_mut() else {
        return;
    };
    let root = stage.actors.root;
    let Some(records) = saved.0.take() else {
        return;
    };
    let result = restore_animal_relationships(&mut commands, &records, &identities, root)
        .and_then(|()| restore_crate_relationships(&mut commands, &records, &identities, root));
    if let Err(reason) = result {
        fail_section(&mut commands, operation, root, reason);
    }
}

pub(super) fn restore_snapshot_adoption(
    mut commands: Commands,
    mut operations: PendingSection<Option<AnimalAdoptionOffersSnapshot>>,
) {
    let Ok((_, stage, mut saved)) = operations.single_mut() else {
        return;
    };
    if let Some(Some(inventory)) = saved.0.take() {
        spawn_saved_adoption_offers(&mut commands, stage.actors.root, inventory);
    }
}

pub(super) fn restore_snapshot_progression(
    mut commands: Commands,
    mut operations: PendingSection<ProgressionSnapshotRecords>,
    mut allocations: Query<&mut PersistentIdAllocator, With<WorldRoot>>,
    identities: Res<PersistentIdIndex>,
    mut progression: ProgressionSnapshotApplicationParameters,
) {
    let Ok((operation, stage, mut saved)) = operations.single_mut() else {
        return;
    };
    let Some(records) = saved.0.take() else {
        return;
    };
    let root = stage.actors.root;
    let result = allocations
        .get_mut(root)
        .map_err(|_| Failure::BrokenPersistentEntityReference)
        .and_then(|mut allocator| {
            apply_progression_snapshot_records_to_live_world(
                &mut commands,
                records,
                root,
                &mut allocator,
                &identities,
                &mut progression,
            )
        });
    match result {
        Ok(research) => {
            commands.entity(operation).insert(research);
        }
        Err(reason) => fail_section(&mut commands, operation, root, reason),
    }
}

pub(super) fn restore_snapshot_shows(
    mut commands: Commands,
    mut operations: PendingSection<ShowSnapshotRecords>,
    mut allocations: Query<&mut PersistentIdAllocator, With<WorldRoot>>,
    identities: Res<PersistentIdIndex>,
    mut shows: Query<
        (
            Entity,
            &PersistentId,
            Option<&mut ShowStageOpenState>,
            Option<&mut ShowName>,
        ),
        With<ShowStage>,
    >,
    mut upgrades: MessageWriter<RestoreShowPlatformUpgrade>,
) {
    let Ok((operation, stage, mut saved)) = operations.single_mut() else {
        return;
    };
    let Some(records) = saved.0.take() else {
        return;
    };
    let root = stage.actors.root;
    let result = allocations
        .get_mut(root)
        .map_err(|_| Failure::BrokenPersistentEntityReference)
        .and_then(|mut allocator| {
            apply_show_snapshot_records_to_live_show_stages_and_schedule(
                &mut commands,
                records,
                root,
                &identities,
                &mut shows,
                &mut allocator,
                &mut upgrades,
            )
        });
    if let Err(reason) = result {
        fail_section(&mut commands, operation, root, reason);
    }
}

pub(super) fn restore_snapshot_images(
    mut commands: Commands,
    mut operations: PendingSection<Vec<PhotoSnapshotRecord>>,
    mut allocations: Query<&mut PersistentIdAllocator, With<WorldRoot>>,
    identities: Res<PersistentIdIndex>,
    mut images: ResMut<Assets<Image>>,
) {
    let Ok((operation, stage, mut saved)) = operations.single_mut() else {
        return;
    };
    let Some(records) = saved.0.take() else {
        return;
    };
    let root = stage.actors.root;
    let result = allocations
        .get_mut(root)
        .map_err(|_| Failure::BrokenPersistentEntityReference)
        .and_then(|mut allocator| {
            spawn_snapshot_photos(
                &mut commands,
                records,
                root,
                &identities,
                &mut allocator,
                &mut images,
            )
        });
    if let Err(reason) = result {
        fail_section(&mut commands, operation, root, reason);
    }
}

pub(super) fn restore_snapshot_scenario(
    mut commands: Commands,
    mut operations: Query<
        (
            Entity,
            &DecodedSnapshot,
            &RestoreRuntimeContinuation,
            &mut SnapshotSectionRestore<Option<ScenarioSnapshot>>,
        ),
        Without<SnapshotRestoreFailed>,
    >,
    mut allocations: Query<&mut PersistentIdAllocator, With<WorldRoot>>,
    mut scenario: ScenarioSnapshotApplication,
) {
    let Ok((operation, decoded, stage, mut saved)) = operations.single_mut() else {
        return;
    };
    let Some(snapshot) = saved.0.take() else {
        return;
    };
    let root = stage.actors.root;
    let result = (|| {
        let prepared = scenario_snapshot_section::prepare(
            snapshot,
            decoded.world.scenario_definition_identifier,
            root,
            &scenario,
        )?;
        let mut allocator = allocations
            .get_mut(root)
            .map_err(|_| Failure::BrokenPersistentEntityReference)?;
        scenario_snapshot_section::apply(
            &mut commands,
            prepared,
            root,
            &mut allocator,
            &mut scenario,
        )
    })();
    if let Err(reason) = result {
        fail_section(&mut commands, operation, root, reason);
    }
}
