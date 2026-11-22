use bevy::{ecs::schedule::ScheduleConfigs, ecs::system::ScheduleSystem, prelude::*};
use openzt2_game_assets::{
    species::species_asset_types::{SpeciesAsset, SpeciesAssets},
    world_definitions::{
        world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
        world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
    },
};
use openzt2_game_data::AssetId;
use openzt2_interface_state::persistence::{
    profile_types::ProfileOptions,
    save_slot_types::{SaveSlotRecord, SaveWorldSnapshotToSlot},
};
use openzt2_world_state::world_spawn::world_membership_types::WorldRoot;
use openzt2_zoo_state::{
    progression::fame_types::Fame, topology::topology_edit_types::TopologySnapshot,
};

use super::{
    super::{
        persistence_failure_types::{
            WorldSnapshotPersistenceFailed, WorldSnapshotPersistenceFailure as Failure,
            WorldSnapshotPersistenceOperation,
        },
        persistence_filesystem_paths::PersistenceFilesystemPaths,
        persistence_ui_types::{SaveDialogSubmission, SaveSnapshotFromDialog},
        progression_snapshot_capture::{
            capture_progression_snapshot_records_from_live_world, ProgressionSnapshotCaptureQueries,
        },
        progression_snapshot_types::ProgressionSnapshotRecords,
        simulation_time_snapshot_section::{
            SimulationTimeSnapshotRecord, SimulationTimeSnapshotSaveResources,
        },
        snapshot_container_encoding_and_validation::initialize_empty_world_snapshot_container,
        snapshot_container_types::ReusableWorldSnapshotByteBuffer,
    },
    animal_adoption_offer_inventory_snapshot_section::{
        validate_animal_adoption_offer_inventory_against_loaded_authored_content,
        validate_animal_adoption_offer_inventory_presence_for_session_mode,
        AdoptionOfferSnapshotCapture, AnimalAdoptionOffersSnapshot,
    },
    animal_snapshot_capture::{
        capture_animal_snapshot_records, AnimalSnapshotEntities, PendingTokenSnapshotEntities,
    },
    animal_snapshot_types::AnimalSnapshotRecord,
    behavior_snapshot_capture::BehaviorSnapshotCapture,
    behavior_snapshot_types::BehaviorActorSnapshot,
    donation_snapshot_section::{
        capture_donation_records, DonationSnapshotEntities, DonationSnapshotRecord,
    },
    economy_snapshot_capture::capture_economy_snapshot_records_from_live_world,
    economy_snapshot_types::EconomySnapshotRecords,
    fence_topology_snapshot_section::{
        capture_fence_topology_records, FenceTopologySnapshotEntities,
    },
    maintenance_snapshot_capture::{
        capture_loose_waste_records, capture_maintained_object_records, LooseWasteSnapshotEntities,
        MaintainedObjectSnapshotEntities,
    },
    maintenance_snapshot_types::MaintenanceSnapshot,
    native_crate_snapshot_links::{
        capture_native_crate_links, validate_native_crate_links, NativeCrateSnapshotEntities,
    },
    original_saved_game_load_preparation::OriginalSavedGameLoadPreparation,
    pending_snapshot_application_types::PendingSnapshotApplication,
    persistence_failure_message_publication::publish_persistence_failure_for_slot_operation,
    persistence_slot_filesystem_task_types::PersistenceSlotFilesystemTask,
    photo_snapshot_capture::{
        capture_sorted_photo_and_challenge_progress_snapshot_records_from_live_world,
        PhotoSnapshotCaptureQueries,
    },
    photo_snapshot_types::{PhotoChallengeProgressSnapshotRecord, PhotoSnapshotRecord},
    placement_snapshot_section::{capture_world_placement_records, PlacementSnapshotRecord},
    runtime_continuation_snapshot_capture::RuntimeContinuationSnapshotCapture,
    runtime_continuation_snapshot_types::RuntimeContinuationSnapshot,
    save_snapshot_file_write_task_execution::start_persistence_slot_snapshot_file_write_task,
    scenario_snapshot_section::{self, ScenarioSnapshot, ScenarioSnapshotCapture},
    show_snapshot_capture::{
        capture_sorted_show_snapshot_records_from_live_world, ShowScheduleSnapshotQueries,
    },
    show_snapshot_types::{
        ScheduledShowBreakSnapshotRecord, ScheduledShowSnapshotRecord, ShowStageSnapshotRecord,
    },
    snapshot_container_section_assembly::append_captured_world_snapshot_sections_and_finalize_container,
    staff_snapshot_capture::{capture_staff_snapshot_records, StaffSnapshotEntities},
    staff_snapshot_types::StaffSnapshotRecord,
    tank_snapshot_capture::capture_sorted_tank_snapshot_records_from_live_world,
    tank_snapshot_types::TankSnapshotRecord,
    terrain_snapshot_capture_parameters::TerrainSnapshotCapture,
    terrain_snapshot_types::TerrainSnapshotRecord,
    transport_snapshot_capture::capture_sorted_transport_snapshot_records_from_live_world,
    transport_snapshot_types::TransportSnapshotRecords,
    world_snapshot_capture::capture_world_snapshot_record_from_live_world,
    world_snapshot_capture_system_parameters::{
        EconomyCaptureQueries, PlacementCaptureQueries, ShowStageCaptureQueries,
        TankCaptureQueries, TransportCaptureQueries, WorldMetadataCaptureQueries,
    },
    world_snapshot_types::WorldSnapshotRecord,
};

#[derive(Component)]
struct SnapshotCaptureRequest {
    profile: AssetId,
    requests: Vec<(SaveWorldSnapshotToSlot, Option<SaveDialogSubmission>)>,
}

#[derive(Component)]
struct Captured<T: Send + Sync + 'static>(Option<Result<T, Failure>>);

type CapturingSnapshot<'w, 's> = Query<'w, 's, Entity, With<SnapshotCaptureRequest>>;
type ShowSnapshot = (
    Vec<ShowStageSnapshotRecord>,
    Vec<ScheduledShowSnapshotRecord>,
    Vec<ScheduledShowBreakSnapshotRecord>,
);
type PhotoSnapshot = (
    Vec<PhotoSnapshotRecord>,
    Vec<PhotoChallengeProgressSnapshotRecord>,
);

fn admit_snapshot_capture(
    mut commands: Commands,
    mut save_requests: MessageReader<SaveWorldSnapshotToSlot>,
    mut dialog_requests: MessageReader<SaveSnapshotFromDialog>,
    directories: Option<Res<PersistenceFilesystemPaths>>,
    profile: Res<ProfileOptions>,
    active_operations: Query<
        (),
        Or<(
            With<PersistenceSlotFilesystemTask>,
            With<PendingSnapshotApplication>,
            With<OriginalSavedGameLoadPreparation>,
        )>,
    >,
    mut failures: MessageWriter<WorldSnapshotPersistenceFailed>,
) {
    let requests = save_requests
        .read()
        .copied()
        .map(|request| (request, None))
        .chain(dialog_requests.read().map(|request| {
            (
                SaveWorldSnapshotToSlot {
                    save_slot_identifier: request.slot,
                },
                Some(request.dialog),
            )
        }));
    if directories.is_none() || !active_operations.is_empty() {
        for (request, _) in requests {
            report_save_failure(&mut failures, request, Failure::FilesystemIo);
        }
        return;
    }
    let requests = requests.collect::<Vec<_>>();
    if !requests.is_empty() {
        commands.spawn(SnapshotCaptureRequest {
            profile: profile.profile_identifier,
            requests,
        });
    }
}

fn report_save_failure(
    failures: &mut MessageWriter<WorldSnapshotPersistenceFailed>,
    request: SaveWorldSnapshotToSlot,
    reason: Failure,
) {
    publish_persistence_failure_for_slot_operation(
        failures,
        WorldSnapshotPersistenceOperation::SaveToSlot,
        request.save_slot_identifier,
        reason,
    );
}

impl<T: Send + Sync + 'static> Captured<T> {
    fn new(result: Result<T, Failure>) -> Self {
        Self(Some(result))
    }

    fn take(&mut self) -> Result<T, Failure> {
        self.0.take().ok_or(Failure::CorruptSnapshotSection)?
    }
}

fn capture_world_and_terrain(
    mut commands: Commands,
    request: CapturingSnapshot,
    world: WorldMetadataCaptureQueries,
    terrain: TerrainSnapshotCapture,
) {
    let Ok(operation) = request.single() else {
        return;
    };
    let world = capture_world_snapshot_record_from_live_world(&world);
    let terrain = world
        .as_ref()
        .ok()
        .filter(|world| world.source_document.is_some())
        .map(|_| terrain.capture())
        .transpose();
    commands
        .entity(operation)
        .insert((Captured::new(world), Captured::new(terrain)));
}

fn check_snapshot_identities(
    mut commands: Commands,
    request: CapturingSnapshot,
    transport: TransportCaptureQueries,
    shows: ShowStageCaptureQueries,
) {
    let Ok(operation) = request.single() else {
        return;
    };
    let valid = transport
        .transport_entities_without_persistent_identifiers
        .is_empty()
        && transport
            .transport_circuit_members_without_definition_identifiers
            .is_empty()
        && shows
            .show_platform_upgrades_without_persistent_identifiers
            .is_empty();
    commands.entity(operation).insert(Captured::new(
        valid
            .then_some(())
            .ok_or(Failure::BrokenPersistentEntityReference),
    ));
}

fn capture_adoption_offers(
    mut commands: Commands,
    request: CapturingSnapshot,
    adoption_offers: AdoptionOfferSnapshotCapture,
    roots: Query<
        (
            &openzt2_world_state::world_spawn::selected_world_identity::SelectedWorldIdentity,
            &Fame,
        ),
        With<WorldRoot>,
    >,
    species_assets: Res<Assets<SpeciesAsset>>,
    species: Res<SpeciesAssets>,
    definition_assets: Res<Assets<WorldDefinitionAsset>>,
    definitions: Res<WorldDefinitions>,
) {
    let Ok(operation) = request.single() else {
        return;
    };
    let capture = || {
        let inventory = adoption_offers.capture()?;
        let (selection, fame) = roots
            .single()
            .map_err(|_| Failure::CorruptSnapshotSection)?;
        validate_animal_adoption_offer_inventory_presence_for_session_mode(
            inventory.as_ref(),
            selection.mode,
        )?;
        if let Some(inventory) = &inventory {
            let species = species
                .get(&species_assets)
                .ok_or(Failure::UnknownAssetIdentifier)?;
            let configuration = definitions
                .get(&definition_assets)
                .and_then(|definitions| definitions.animal_adoption_offer_configuration())
                .ok_or(Failure::UnknownAssetIdentifier)?;
            validate_animal_adoption_offer_inventory_against_loaded_authored_content(
                inventory,
                species,
                configuration,
                *fame,
            )?;
        }
        Ok(inventory)
    };
    commands.entity(operation).insert(Captured::new(capture()));
}

fn capture_animals(
    mut commands: Commands,
    request: CapturingSnapshot,
    animals: AnimalSnapshotEntities,
    tokens: PendingTokenSnapshotEntities,
    crates: NativeCrateSnapshotEntities,
) {
    let Ok(operation) = request.single() else {
        return;
    };
    let capture = capture_animal_snapshot_records(&animals, &tokens).and_then(|mut records| {
        capture_native_crate_links(&animals, &crates, &mut records)?;
        Ok(records)
    });
    commands.entity(operation).insert(Captured::new(capture));
}

fn capture_simulation_time(
    mut commands: Commands,
    request: CapturingSnapshot,
    time: SimulationTimeSnapshotSaveResources,
    assets: Res<Assets<WorldDefinitionAsset>>,
    definitions: Res<WorldDefinitions>,
) {
    let Ok(operation) = request.single() else {
        return;
    };
    let capture = time
        .capture_validated_simulation_time_snapshot_record()
        .and_then(|record| {
            let definitions = definitions
                .get(&assets)
                .ok_or(Failure::UnknownAssetIdentifier)?;
            record.validate_against_timing(definitions.timing())
        });
    commands.entity(operation).insert(Captured::new(capture));
}

fn capture_runtime_continuation(
    mut commands: Commands,
    request: CapturingSnapshot,
    roots: Query<Entity, With<WorldRoot>>,
    runtime: RuntimeContinuationSnapshotCapture,
) {
    let Ok(operation) = request.single() else {
        return;
    };
    let capture = roots
        .single()
        .map_err(|_| Failure::BrokenPersistentEntityReference)
        .and_then(|root| runtime.capture(root));
    commands.entity(operation).insert(Captured::new(capture));
}

fn capture_topology(
    mut commands: Commands,
    request: CapturingSnapshot,
    topology: FenceTopologySnapshotEntities,
) {
    let Ok(operation) = request.single() else {
        return;
    };
    commands
        .entity(operation)
        .insert(Captured::new(capture_fence_topology_records(&topology)));
}

fn capture_maintenance(
    mut commands: Commands,
    request: CapturingSnapshot,
    waste: LooseWasteSnapshotEntities,
    objects: MaintainedObjectSnapshotEntities,
) {
    let Ok(operation) = request.single() else {
        return;
    };
    commands
        .entity(operation)
        .insert(Captured::new(capture_loose_waste_records(&waste).map(
            |waste| MaintenanceSnapshot {
                waste,
                objects: capture_maintained_object_records(&objects),
            },
        )));
}

fn capture_staff(mut commands: Commands, request: CapturingSnapshot, staff: StaffSnapshotEntities) {
    let Ok(operation) = request.single() else {
        return;
    };
    commands
        .entity(operation)
        .insert(Captured::new(capture_staff_snapshot_records(&staff)));
}

fn capture_tanks(mut commands: Commands, request: CapturingSnapshot, tanks: TankCaptureQueries) {
    let Ok(operation) = request.single() else {
        return;
    };
    commands.entity(operation).insert(Captured::new(Ok(
        capture_sorted_tank_snapshot_records_from_live_world(&tanks),
    )));
}

fn capture_shows(
    mut commands: Commands,
    request: CapturingSnapshot,
    stages: ShowStageCaptureQueries,
    shows: ShowScheduleSnapshotQueries,
) {
    let Ok(operation) = request.single() else {
        return;
    };
    commands.entity(operation).insert(Captured::new(
        capture_sorted_show_snapshot_records_from_live_world(&stages, &shows),
    ));
}

fn capture_transport(
    mut commands: Commands,
    request: CapturingSnapshot,
    transport: TransportCaptureQueries,
) {
    let Ok(operation) = request.single() else {
        return;
    };
    commands.entity(operation).insert(Captured::new(
        capture_sorted_transport_snapshot_records_from_live_world(&transport),
    ));
}

fn capture_donations(
    mut commands: Commands,
    request: CapturingSnapshot,
    donations: DonationSnapshotEntities,
) {
    let Ok(operation) = request.single() else {
        return;
    };
    commands
        .entity(operation)
        .insert(Captured::new(capture_donation_records(&donations)));
}

fn capture_placements(
    mut commands: Commands,
    request: CapturingSnapshot,
    placements: PlacementCaptureQueries,
) {
    let Ok(operation) = request.single() else {
        return;
    };
    commands
        .entity(operation)
        .insert(Captured::new(capture_world_placement_records(&placements)));
}

fn capture_economy(
    mut commands: Commands,
    request: CapturingSnapshot,
    economy: EconomyCaptureQueries,
) {
    let Ok(operation) = request.single() else {
        return;
    };
    commands.entity(operation).insert(Captured::new(Ok(
        capture_economy_snapshot_records_from_live_world(&economy),
    )));
}

fn capture_photos(
    mut commands: Commands,
    request: CapturingSnapshot,
    photos: PhotoSnapshotCaptureQueries,
) {
    let Ok(operation) = request.single() else {
        return;
    };
    commands.entity(operation).insert(Captured::new(
        capture_sorted_photo_and_challenge_progress_snapshot_records_from_live_world(&photos),
    ));
}

fn capture_progression(
    mut commands: Commands,
    request: CapturingSnapshot,
    progression: ProgressionSnapshotCaptureQueries,
) {
    let Ok(operation) = request.single() else {
        return;
    };
    commands.entity(operation).insert(Captured::new(
        capture_progression_snapshot_records_from_live_world(&progression),
    ));
}

fn capture_scenario(
    mut commands: Commands,
    request: CapturingSnapshot,
    scenario: ScenarioSnapshotCapture,
) {
    let Ok(operation) = request.single() else {
        return;
    };
    commands
        .entity(operation)
        .insert(Captured::new(scenario_snapshot_section::capture(&scenario)));
}

fn capture_behavior(
    mut commands: Commands,
    request: CapturingSnapshot,
    behavior: BehaviorSnapshotCapture,
) {
    let Ok(operation) = request.single() else {
        return;
    };
    commands
        .entity(operation)
        .insert(Captured::new(behavior.capture()));
}

#[derive(bevy::ecs::query::QueryData)]
#[query_data(mutable)]
struct CapturedSnapshot {
    identities: &'static mut Captured<()>,
    world: &'static mut Captured<WorldSnapshotRecord>,
    adoption: &'static mut Captured<Option<AnimalAdoptionOffersSnapshot>>,
    maintenance: &'static mut Captured<MaintenanceSnapshot>,
    topology: &'static mut Captured<Vec<TopologySnapshot>>,
    animals: &'static mut Captured<Vec<AnimalSnapshotRecord>>,
    staff: &'static mut Captured<Vec<StaffSnapshotRecord>>,
    tanks: &'static mut Captured<Vec<TankSnapshotRecord>>,
    shows: &'static mut Captured<ShowSnapshot>,
    transport: &'static mut Captured<TransportSnapshotRecords>,
    donations: &'static mut Captured<Vec<DonationSnapshotRecord>>,
    placements: &'static mut Captured<Vec<PlacementSnapshotRecord>>,
    economy: &'static mut Captured<EconomySnapshotRecords>,
    photos: &'static mut Captured<PhotoSnapshot>,
    progression: &'static mut Captured<ProgressionSnapshotRecords>,
    time: &'static mut Captured<SimulationTimeSnapshotRecord>,
    scenario: &'static mut Captured<Option<ScenarioSnapshot>>,
    behavior: &'static mut Captured<Vec<BehaviorActorSnapshot>>,
    terrain: &'static mut Captured<Option<TerrainSnapshotRecord>>,
    runtime: &'static mut Captured<RuntimeContinuationSnapshot>,
}

fn encode_captured_snapshot(
    mut commands: Commands,
    mut captured: Query<(Entity, &SnapshotCaptureRequest, CapturedSnapshot)>,
    directories: Res<PersistenceFilesystemPaths>,
    mut scratch: ResMut<ReusableWorldSnapshotByteBuffer>,
    mut failures: MessageWriter<WorldSnapshotPersistenceFailed>,
) {
    let Ok((operation, request, mut captured)) = captured.single_mut() else {
        return;
    };
    let profile = request.profile;
    let (first, dialog) = request.requests[0];
    let mut start = || {
        captured.identities.take()?;
        let world = captured.world.take()?;
        let adoption = captured.adoption.take()?;
        let maintenance = captured.maintenance.take()?;
        let topology = captured.topology.take()?;
        let animals = captured.animals.take()?;
        let staff = captured.staff.take()?;
        let tanks = captured.tanks.take()?;
        let (stages, shows, breaks) = captured.shows.take()?;
        let transport = captured.transport.take()?;
        let donations = captured.donations.take()?;
        let placements = captured.placements.take()?;
        validate_native_crate_links(&animals, &placements)?;
        let economy = captured.economy.take()?;
        let (photos, challenges) = captured.photos.take()?;
        let progression = captured.progression.take()?;
        let time = captured.time.take()?;
        let scenario = captured.scenario.take()?;
        let behavior = captured.behavior.take()?;
        let terrain = captured.terrain.take()?;
        let runtime = captured.runtime.take()?;
        let slot = first.save_slot_identifier;
        let path = directories.save_slot_file_path(&profile.0, slot);
        let record = SaveSlotRecord {
            save_slot_identifier: slot,
            world_display_name: world.display_name.clone(),
            last_saved_unix_timestamp_milliseconds: world.last_saved_unix_timestamp_milliseconds,
            scenario_definition_identifier: world.scenario_definition_identifier,
            map_definition_identifier: world.map_definition_identifier,
            profile_identifier: profile,
        };
        scratch.clear_logical_contents();
        let bytes = std::mem::take(&mut scratch.snapshot_container_bytes);
        commands
            .entity(operation)
            .remove::<SnapshotCaptureRequest>()
            .remove::<(
                Captured<()>,
                Captured<WorldSnapshotRecord>,
                Captured<Option<AnimalAdoptionOffersSnapshot>>,
                Captured<MaintenanceSnapshot>,
                Captured<Vec<TopologySnapshot>>,
                Captured<Vec<AnimalSnapshotRecord>>,
                Captured<Vec<StaffSnapshotRecord>>,
                Captured<Vec<TankSnapshotRecord>>,
                Captured<ShowSnapshot>,
                Captured<TransportSnapshotRecords>,
                Captured<Vec<DonationSnapshotRecord>>,
                Captured<Vec<PlacementSnapshotRecord>>,
            )>()
            .remove::<(
                Captured<EconomySnapshotRecords>,
                Captured<PhotoSnapshot>,
                Captured<ProgressionSnapshotRecords>,
                Captured<SimulationTimeSnapshotRecord>,
                Captured<Option<ScenarioSnapshot>>,
                Captured<Vec<BehaviorActorSnapshot>>,
                Captured<Option<TerrainSnapshotRecord>>,
                Captured<RuntimeContinuationSnapshot>,
            )>();
        start_persistence_slot_snapshot_file_write_task(
            commands.entity(operation),
            path,
            record,
            bytes,
            dialog,
            move |bytes| {
                bytes.clear();
                initialize_empty_world_snapshot_container(bytes, profile);
                append_captured_world_snapshot_sections_and_finalize_container(
                    bytes,
                    &world,
                    terrain.as_ref(),
                    scenario.as_ref(),
                    adoption.as_ref(),
                    &animals,
                    &maintenance,
                    &staff,
                    &tanks,
                    Some(&topology),
                    &economy,
                    &donations,
                    Some(&placements),
                    &progression,
                    &time,
                    &stages,
                    &shows,
                    &breaks,
                    &transport,
                    &photos,
                    &challenges,
                    &behavior,
                    Some(&runtime),
                )
            },
        );
        Ok(())
    };
    match start() {
        Ok(()) => {
            for (duplicate, _) in &request.requests[1..] {
                report_save_failure(&mut failures, *duplicate, Failure::FilesystemIo);
            }
        }
        Err(reason) => {
            for (request, _) in &request.requests {
                report_save_failure(&mut failures, *request, reason);
            }
            commands.entity(operation).despawn();
        }
    }
}

pub(in crate::persistence) fn capture_requested_world_snapshots() -> ScheduleConfigs<ScheduleSystem>
{
    (
        admit_snapshot_capture,
        (
            (
                capture_world_and_terrain,
                check_snapshot_identities,
                capture_adoption_offers,
                capture_animals,
                capture_simulation_time,
                capture_runtime_continuation,
            ),
            (
                capture_topology,
                capture_maintenance,
                capture_staff,
                capture_tanks,
                capture_shows,
                capture_transport,
                capture_donations,
            ),
            (
                capture_placements,
                capture_economy,
                capture_photos,
                capture_progression,
                capture_scenario,
                capture_behavior,
            ),
        )
            .run_if(any_with_component::<SnapshotCaptureRequest>),
        encode_captured_snapshot.run_if(any_with_component::<SnapshotCaptureRequest>),
    )
        .chain()
}
