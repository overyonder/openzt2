use bevy::{ecs::system::SystemParam, prelude::*};

use openzt2_game_assets::species::species_asset_types::SpeciesAsset;
use openzt2_game_assets::species::species_asset_types::SpeciesAssets;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_animals::animal_lifecycle::adoption_offer_inventory_types::AnimalAdoptionOfferInventory;
use openzt2_zoo_state::aquatic::aquatic_simulation_types::Tank;
use openzt2_zoo_state::aquatic::aquatic_simulation_types::TankGeometry;
use openzt2_zoo_state::aquatic::aquatic_simulation_types::WaterQuality;
use openzt2_zoo_state::economy::guest_admission_types::AdmissionPrice;
use openzt2_zoo_state::economy::guest_admission_types::ZooAdmissionsOpen;
use openzt2_zoo_state::economy::monthly_finance_types::MonthlyFinanceHistory;
use openzt2_zoo_state::economy::zoo_cash_types::ZooCash;
use openzt2_zoo_state::progression::fame_types::Fame;
use openzt2_zoo_state::shows::show_platform_upgrade_types::InstalledShowPlatformUpgrade;
use openzt2_zoo_state::shows::show_platform_upgrade_types::InstalledShowPlatformUpgradeStage;
use openzt2_zoo_state::shows::show_stage_types::ShowName;
use openzt2_zoo_state::shows::show_stage_types::ShowStage;
use openzt2_zoo_state::shows::show_stage_types::ShowStageOpenState;
use openzt2_zoo_state::transport_tours::transport_circuit_types::CircuitDirection;
use openzt2_zoo_state::transport_tours::transport_circuit_types::CircuitMember;
use openzt2_zoo_state::transport_tours::transport_circuit_types::CircuitRunning;
use openzt2_zoo_state::transport_tours::transport_circuit_types::TransportCircuit;
use openzt2_zoo_state::transport_tours::transport_vehicle_types::TransportVehicle;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_world_state::world_spawn::selected_world_identity::SelectedWorldIdentity;
use openzt2_world_state::world_spawn::world_membership_types::DefinitionId;
use openzt2_world_state::world_spawn::world_membership_types::WorldRoot;

use super::super::{
    progression_snapshot_capture::ProgressionSnapshotCaptureQueries,
    simulation_time_snapshot_section::SimulationTimeSnapshotSaveResources,
};

#[derive(SystemParam)]
pub(in crate::persistence) struct WorldSnapshotCaptureQueries<'w, 's> {
    pub(super) runtime_continuation: super::runtime_continuation_snapshot_capture::RuntimeContinuationSnapshotCapture<'w, 's>,
    pub(super) behavior: super::behavior_snapshot_capture::BehaviorSnapshotCapture<'w, 's>,
    pub(super) terrain: super::terrain_snapshot_capture_parameters::TerrainSnapshotCapture<'w,'s>,
    pub(super) asset_server: Option<Res<'w, AssetServer>>,
    pub(super) world_scenario_assets: Option<Res<'w, Assets<openzt2_game_assets::world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset>>>,
    pub(super) world_scenarios: Option<Res<'w, openzt2_game_assets::world_scenario::world_scenario_asset_set_state_and_borrowing_queries::WorldScenarios>>,
    pub(super) world_documents: Query<'w,'s,(
        &'static openzt2_world_state::world_spawn::selected_world_document_handles::SelectedWorldDocumentHandles,
        &'static SelectedWorldIdentity,
    ),With<WorldRoot>>,
    pub(super) topology:
        super::fence_topology_snapshot_section::FenceTopologySnapshotEntities<'w, 's>,
    pub(super) scenario: super::scenario_snapshot_section::ScenarioSnapshotCapture<'w, 's>,
    pub(super) placements: super::placement_snapshot_section::PlacementSnapshotEntities<'w, 's>,
    pub(super) donations: super::donation_snapshot_section::DonationSnapshotEntities<'w, 's>,
    pub(super) habitat_waste:
        super::maintenance_snapshot_capture::LooseWasteSnapshotEntities<'w, 's>,
    pub(super) maintained_objects:
        super::maintenance_snapshot_capture::MaintainedObjectSnapshotEntities<'w, 's>,
    pub(super) animals: super::animal_snapshot_capture::AnimalSnapshotEntities<'w, 's>,
    pub(super) pending_tokens: super::animal_snapshot_capture::PendingTokenSnapshotEntities<'w, 's>,
    pub(super) native_crates:
        super::native_crate_snapshot_links::NativeCrateSnapshotEntities<'w, 's>,
    pub(super) staff: super::staff_snapshot_capture::StaffSnapshotEntities<'w, 's>,
    pub(super) species_assets: Res<'w, Assets<SpeciesAsset>>,
    pub(super) active_species: Res<'w, SpeciesAssets>,
    pub(super) world_definition_assets: Res<'w, Assets<WorldDefinitionAsset>>,
    pub(super) active_world_definitions: Res<'w, WorldDefinitions>,
    pub(super) fame: Res<'w, Fame>,
    pub(super) world_roots: Query<
        'w,
        's,
        (
            Entity,
            &'static WorldRoot,
            &'static SelectedWorldIdentity,
            Option<&'static Name>,
        ),
    >,
    pub(super) animal_adoption_offer_inventories:
        Query<'w, 's, Option<&'static AnimalAdoptionOfferInventory>, With<WorldRoot>>,
    pub(super) tanks_with_snapshot_state: Query<
        'w,
        's,
        (
            &'static Tank,
            &'static TankGeometry,
            Option<&'static WaterQuality>,
            &'static openzt2_zoo_state::habitat::habitat_types::HabitatRegion,
        ),
    >,
    pub(super) show_stages_with_snapshot_state: Query<
        'w,
        's,
        (
            Entity,
            &'static PersistentId,
            Option<&'static ShowStageOpenState>,
            Option<&'static ShowName>,
        ),
        With<ShowStage>,
    >,
    pub(super) installed_show_platform_upgrades: Query<
        'w,
        's,
        (
            &'static PersistentId,
            &'static InstalledShowPlatformUpgrade,
            &'static InstalledShowPlatformUpgradeStage,
        ),
    >,
    pub(super) show_platform_upgrades_without_persistent_identifiers:
        Query<'w, 's, (), (With<InstalledShowPlatformUpgrade>, Without<PersistentId>)>,
    pub(super) entities_with_persistent_identifiers: Query<'w, 's, &'static PersistentId>,
    pub(super) transport_entity_poses: Query<'w, 's, &'static Transform>,
    pub(super) transport_fares: Query<'w, 's, &'static openzt2_zoo_state::economy::facility_economy_types::Price>,
    pub(super) transport_track_routes: Query<'w, 's, (
        &'static PersistentId,
        openzt2_zoo_state::transport_tours::transport_topology_types::TrackRoute,
        &'static openzt2_zoo_state::transport_tours::transport_topology_types::TrackProfile,
        &'static openzt2_zoo_state::transport_tours::transport_track_construction_types::TrackPieces,
    )>,
    pub(super) transport_track_pieces: Query<'w, 's, (&'static PersistentId, &'static openzt2_zoo_state::transport_tours::transport_track_construction_types::TransportTrackConstructionPiece)>,
    pub(super) transport_junctions: Query<'w, 's, (&'static PersistentId, Option<&'static openzt2_zoo_state::transport_tours::transport_topology_types::SkyTowerHeight>),
        With<openzt2_zoo_state::transport_tours::transport_topology_types::TransportTrackJunction>>,
    pub(super) transport_circuits_with_snapshot_state: Query<
        'w,
        's,
        (
            Entity,
            &'static PersistentId,
            &'static TransportCircuit,
            Option<&'static CircuitRunning>,
            Option<&'static CircuitDirection>,
        ),
    >,
    pub(super) transport_circuit_members_with_definition_identifiers: Query<
        'w,
        's,
        (
            Entity,
            &'static PersistentId,
            &'static CircuitMember,
            &'static DefinitionId,
        ),
        Without<TransportVehicle>,
    >,
    pub(super) transport_vehicles_with_definition_identifiers: Query<
        'w,
        's,
        (
            Entity,
            &'static PersistentId,
            &'static CircuitMember,
            &'static DefinitionId,
        ),
        With<TransportVehicle>,
    >,
    pub(super) transport_entities_without_persistent_identifiers: Query<
        'w,
        's,
        (),
        (
            Or<(
                With<TransportCircuit>,
                With<CircuitMember>,
                With<TransportVehicle>,
            )>,
            Without<PersistentId>,
        ),
    >,
    pub(super) transport_circuit_members_without_definition_identifiers:
        Query<'w, 's, (), (With<CircuitMember>, Without<DefinitionId>)>,
    pub(super) entities_with_economy_state: Query<
        'w,
        's,
        EntityRef<'static>,
        (
            With<PersistentId>,
            Without<bevy::ecs::resource::IsResource>,
            Without<openzt2_zoo_state::guests::guest_simulation_types::Guest>,
        ),
    >,
    pub(super) unlimited_cash:
        Option<Res<'w, openzt2_zoo_state::economy::zoo_cash_types::UnlimitedZooCash>>,
    pub(super) zoo_cash: Res<'w, ZooCash>,
    pub(super) zoo_admission_price: Res<'w, AdmissionPrice>,
    pub(super) zoo_admissions_open_state: Res<'w, ZooAdmissionsOpen>,
    pub(super) monthly_finance_history: Res<'w, MonthlyFinanceHistory>,
    pub(super) progression_snapshot_capture_queries: ProgressionSnapshotCaptureQueries<'w, 's>,
    pub(super) simulation_time_snapshot_capture_resources: SimulationTimeSnapshotSaveResources<'w>,
}
