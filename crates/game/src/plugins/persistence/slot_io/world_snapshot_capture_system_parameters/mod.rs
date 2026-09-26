use bevy::{ecs::system::SystemParam, prelude::*};

use crate::assets::species::species_asset_types::SpeciesAsset;
use crate::assets::species::species_asset_types::SpeciesAssets;
use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::{
    animal_lifecycle::adoption_offer_inventory_types::AnimalAdoptionOfferInventory,
    aquatic::aquatic_simulation_types::{Tank, TankGeometry, WaterQuality},
    economy::{
        facility_economy_types::{FacilityProfit, OperatingSinceDay},
        guest_admission_types::{AdmissionPrice, ZooAdmissionsOpen},
        monthly_finance_types::MonthlyFinanceHistory,
        zoo_cash_types::ZooCash,
    },
    photos::{
        photo_album_types::{AlbumMember, PhotoAlbum, PhotoAlbumOrder},
        photo_capture_types::Photo,
        photo_challenge_types::{PhotoChallenge, PhotoChallengeProgress},
    },
    progression::fame_types::Fame,
    shows::{
        show_platform_upgrade_types::{
            InstalledShowPlatformUpgrade, InstalledShowPlatformUpgradeStage,
        },
        show_schedule_types::{
            ScheduledShowBreakRow, ScheduledShowPerformancePlan, ScheduledShowRow,
            ScheduledShowRowActivationState, ShowScheduleRowOrder,
        },
        show_stage_types::{ShowName, ShowStage, ShowStageOpenState},
    },
    transport_tours::{
        transport_circuit_types::{
            CircuitDirection, CircuitMember, CircuitRunning, TransportCircuit,
        },
        transport_vehicle_types::TransportVehicle,
    },
    world_spawn::{
        persistent_id_types::PersistentId, selected_world_identity::SelectedWorldIdentity,
        world_membership_types::DefinitionId, world_membership_types::WorldRoot,
    },
};

use super::super::{
    generated_photo_persistence_types::DurableChallengePhotoFileIdentifiers,
    progression_snapshot_capture::ProgressionSnapshotCaptureQueries,
    simulation_time_snapshot_section::SimulationTimeSnapshotSaveResources,
};

#[derive(SystemParam)]
pub(in crate::plugins::persistence) struct WorldSnapshotCaptureQueries<'w, 's> {
    pub(super) species_assets: Res<'w, Assets<SpeciesAsset>>,
    pub(super) active_species: Res<'w, SpeciesAssets>,
    pub(super) world_definition_assets: Res<'w, Assets<WorldDefinitionAsset>>,
    pub(super) active_world_definitions: Res<'w, WorldDefinitions>,
    pub(super) fame: Res<'w, Fame>,
    pub(super) world_roots: Query<
        'w,
        's,
        (
            &'static WorldRoot,
            &'static SelectedWorldIdentity,
            Option<&'static Name>,
        ),
    >,
    pub(super) animal_adoption_offer_inventories:
        Query<'w, 's, &'static AnimalAdoptionOfferInventory, With<WorldRoot>>,
    pub(super) tanks_with_snapshot_state: Query<
        'w,
        's,
        (
            &'static PersistentId,
            &'static Tank,
            &'static TankGeometry,
            Option<&'static WaterQuality>,
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
    pub(super) scheduled_show_rows_with_snapshot_state: Query<
        'w,
        's,
        (
            &'static PersistentId,
            &'static ScheduledShowRow,
            &'static ShowScheduleRowOrder,
            &'static ScheduledShowPerformancePlan,
            Option<&'static ScheduledShowRowActivationState>,
        ),
    >,
    pub(super) scheduled_show_break_rows: Query<
        'w,
        's,
        (
            &'static PersistentId,
            &'static ScheduledShowBreakRow,
            &'static ShowScheduleRowOrder,
        ),
    >,
    pub(super) entities_with_persistent_identifiers: Query<'w, 's, &'static PersistentId>,
    pub(super) photos_with_album_membership: Query<
        'w,
        's,
        (
            &'static PersistentId,
            &'static Photo,
            &'static AlbumMember,
            Option<&'static PhotoAlbumOrder>,
            Option<&'static DurableChallengePhotoFileIdentifiers>,
        ),
    >,
    pub(super) photo_album_persistent_identifiers:
        Query<'w, 's, &'static PersistentId, With<PhotoAlbum>>,
    pub(super) photo_challenges_with_progress:
        Query<'w, 's, (&'static PhotoChallenge, &'static PhotoChallengeProgress)>,
    pub(super) loaded_photo_images: Res<'w, Assets<Image>>,
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
    pub(super) facilities_with_economy_state: Query<
        'w,
        's,
        (
            &'static PersistentId,
            &'static OperatingSinceDay,
            &'static FacilityProfit,
        ),
    >,
    pub(super) zoo_cash: Res<'w, ZooCash>,
    pub(super) zoo_admission_price: Res<'w, AdmissionPrice>,
    pub(super) zoo_admissions_open_state: Res<'w, ZooAdmissionsOpen>,
    pub(super) monthly_finance_history: Res<'w, MonthlyFinanceHistory>,
    pub(super) progression_snapshot_capture_queries: ProgressionSnapshotCaptureQueries<'w, 's>,
    pub(super) simulation_time_snapshot_capture_resources: SimulationTimeSnapshotSaveResources<'w>,
}
