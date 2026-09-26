use bevy::{ecs::system::SystemParam, prelude::*};

use crate::assets::species::species_asset_types::SpeciesAsset;
use crate::assets::species::species_asset_types::SpeciesAssets;
use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::{
    aquatic::aquatic_simulation_types::Tank,
    economy::{
        facility_economy_types::FacilityProfit,
        guest_admission_types::{AdmissionPrice, ZooAdmissionsOpen},
        monthly_finance_types::MonthlyFinanceHistory,
        zoo_cash_types::ZooCash,
    },
    photos::{
        photo_album_types::PhotoAlbum,
        photo_challenge_types::{PhotoChallenge, PhotoChallengeProgress},
    },
    shows::{
        show_platform_upgrade_types::RestoreShowPlatformUpgrade,
        show_stage_types::{ShowName, ShowStage, ShowStageOpenState},
    },
    transport_tours::{
        transport_circuit_types::{CircuitMember, TransportCircuit},
        transport_vehicle_types::TransportVehicle,
    },
    world_spawn::{
        persistent_id_types::PersistentId, world_hydration_completion::WorldLoadFinished,
        world_load_failure::WorldLoadFailed, world_load_request_acceptance::BeginWorldLoad,
        world_unloading::UnloadWorld,
    },
};

use super::super::{
    persistence_failure_types::WorldSnapshotPersistenceFailed,
    save_slot_types::WorldSnapshotLoadedFromSlot,
};

#[derive(SystemParam)]
pub(in crate::plugins::persistence) struct WorldSnapshotApplicationMessages<'w, 's> {
    pub(super) begin_world_load_requests: MessageWriter<'w, BeginWorldLoad>,
    pub(super) world_unload_requests: MessageWriter<'w, UnloadWorld>,
    pub(super) completed_world_loads: MessageReader<'w, 's, WorldLoadFinished>,
    pub(super) failed_world_loads: MessageReader<'w, 's, WorldLoadFailed>,
    pub(super) completed_snapshot_loads: MessageWriter<'w, WorldSnapshotLoadedFromSlot>,
    pub(super) persistence_failures: MessageWriter<'w, WorldSnapshotPersistenceFailed>,
    pub(super) show_platform_upgrade_restore_requests:
        MessageWriter<'w, RestoreShowPlatformUpgrade>,
}

#[derive(SystemParam)]
pub(in crate::plugins::persistence) struct WorldSnapshotApplicationState<'w, 's> {
    pub(super) species_assets: Res<'w, Assets<SpeciesAsset>>,
    pub(super) active_species: Res<'w, SpeciesAssets>,
    pub(super) world_definition_assets: Res<'w, Assets<WorldDefinitionAsset>>,
    pub(super) active_world_definitions: Res<'w, WorldDefinitions>,
    pub(super) entities_with_persistent_identifiers: Query<'w, 's, (Entity, &'static PersistentId)>,
    pub(super) transport_entities_with_persistent_identifiers: Query<
        'w,
        's,
        (Entity, &'static PersistentId),
        Or<(
            With<TransportCircuit>,
            With<CircuitMember>,
            With<TransportVehicle>,
        )>,
    >,
    pub(super) show_stages_with_mutable_snapshot_state: Query<
        'w,
        's,
        (
            Entity,
            &'static PersistentId,
            Option<&'static mut ShowStageOpenState>,
            Option<&'static mut ShowName>,
        ),
        With<ShowStage>,
    >,
    pub(super) loaded_photo_images: ResMut<'w, Assets<Image>>,
    pub(super) photo_challenges_with_mutable_progress:
        Query<'w, 's, (&'static PhotoChallenge, &'static mut PhotoChallengeProgress)>,
    pub(super) tank_persistent_identifiers: Query<'w, 's, &'static PersistentId, With<Tank>>,
    pub(super) facility_persistent_identifiers:
        Query<'w, 's, &'static PersistentId, With<FacilityProfit>>,
    pub(super) zoo_cash: ResMut<'w, ZooCash>,
    pub(super) zoo_admission_price: ResMut<'w, AdmissionPrice>,
    pub(super) zoo_admissions_open_state: ResMut<'w, ZooAdmissionsOpen>,
    pub(super) monthly_finance_history: ResMut<'w, MonthlyFinanceHistory>,
    pub(super) photo_album_persistent_identifiers:
        Query<'w, 's, &'static PersistentId, With<PhotoAlbum>>,
}
