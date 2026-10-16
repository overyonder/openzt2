use bevy::{ecs::system::SystemParam, prelude::*};

use openzt2_game_assets::species::species_asset_types::SpeciesAsset;
use openzt2_game_assets::species::species_asset_types::SpeciesAssets;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_zoo_state::aquatic::aquatic_simulation_types::Tank;
use openzt2_zoo_state::economy::facility_economy_types::FacilityProfit;
use openzt2_zoo_state::economy::guest_admission_types::AdmissionPrice;
use openzt2_zoo_state::economy::guest_admission_types::ZooAdmissionsOpen;
use openzt2_zoo_state::economy::monthly_finance_types::MonthlyFinanceHistory;
use openzt2_zoo_state::economy::zoo_cash_types::ZooCash;
use openzt2_interface_state::photos::photo_album_types::PhotoAlbum;
use openzt2_interface_state::photos::photo_challenge_types::PhotoChallenge;
use openzt2_interface_state::photos::photo_challenge_types::PhotoChallengeProgress;
use openzt2_zoo_state::shows::show_platform_upgrade_types::RestoreShowPlatformUpgrade;
use openzt2_zoo_state::shows::show_stage_types::ShowName;
use openzt2_zoo_state::shows::show_stage_types::ShowStage;
use openzt2_zoo_state::shows::show_stage_types::ShowStageOpenState;
use openzt2_zoo_state::transport_tours::transport_circuit_types::CircuitMember;
use openzt2_zoo_state::transport_tours::transport_circuit_types::TransportCircuit;
use openzt2_zoo_state::transport_tours::transport_vehicle_types::TransportVehicle;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_world_state::world_spawn::world_hydration_completion::WorldLoadFinished;
use openzt2_world_state::world_spawn::world_membership_types::DefinitionId;
use openzt2_world_state::world_spawn::world_load_failure::WorldLoadFailed;
use openzt2_world_state::world_spawn::world_load_request_acceptance::BeginWorldLoad;
use openzt2_world_state::world_spawn::world_unloading::UnloadWorld;

use openzt2_interface_state::persistence::save_slot_types::WorldSnapshotLoadedFromSlot;

use super::super::persistence_failure_types::WorldSnapshotPersistenceFailed;

#[derive(SystemParam)]
pub(in crate::persistence) struct WorldSnapshotApplicationMessages<'w, 's> {
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
pub(in crate::persistence) struct WorldSnapshotApplicationState<'w, 's> {
    pub(super) scenario: super::scenario_snapshot_section::ScenarioSnapshotApplication<'w, 's>,
    pub(super) placements: super::placement_snapshot_section::PlacementSnapshotEntities<'w, 's>,
    pub(super) placement_models: Res<'w, Assets<bevy::gltf::Gltf>>,
    pub(super) placement_readiness: Query<
        'w,
        's,
        (
            &'static PersistentId,
            &'static DefinitionId,
            &'static Transform,
            Has<openzt2_zoo_state::placement::placed_object_types::PlacedObjectDefinitionReference>,
            Has<openzt2_zoo_state::donations::donation_opportunity_types::DonationUsePoint>,
        ),
    >,
    pub(super) photo_baseline:
        super::photo_snapshot_baseline_readiness::PhotoSnapshotBaselineReadiness<'w, 's>,
    pub(super) baseline_waste: Query<
        'w,
        's,
        Entity,
        With<openzt2_zoo_state::maintenance::maintenance_types::AnimalHabitatWaste>,
    >,
    pub(super) prefabs: Res<'w, Assets<openzt2_game_assets::scene_prefab::ScenePrefabAsset>>,
    pub(super) asset_server: Res<'w, AssetServer>,
    pub(super) baseline_animals:
        Query<'w, 's, Entity, With<openzt2_zoo_state::animal_lifecycle::types::Animal>>,
    pub(super) species_assets: Res<'w, Assets<SpeciesAsset>>,
    pub(super) active_species: Res<'w, SpeciesAssets>,
    pub(super) world_definition_assets: Res<'w, Assets<WorldDefinitionAsset>>,
    pub(super) active_world_definitions: Res<'w, WorldDefinitions>,
    pub(super) entities_with_persistent_identifiers: Query<'w, 's, (Entity, &'static PersistentId)>,
    pub(super) entities_with_definition_identifiers: Query<'w, 's, (Entity, &'static DefinitionId)>,
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
