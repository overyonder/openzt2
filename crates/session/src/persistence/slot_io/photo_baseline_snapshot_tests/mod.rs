use bevy::prelude::*;
use openzt2_game_data::{
    localization::{LocalizationCatalog, LocalizationEntry},
    species::SpeciesDocument,
    world_definitions::{
        document::WorldDefinitionDocument,
        simulation_time::{
            SimulationCalendarLeapPolicy, SimulationCalendarPolicy, SimulationTimingDefinition,
        },
    },
    world_scenario::{
        PhotoChallengeRule, WorldMapRecord, WorldMapSupportedGameModeFlags, WorldScenarioDocument,
    },
    AssetId,
};
use openzt2_photos::PhotosPlugin;

use openzt2_game_assets::{
    application_lifecycle::GamePhase,
    {
        localization::{
            localization_asset_types::LocalizationAsset,
            localization_precedence_index::LocalizationPrecedenceIndex,
        },
        lua_script::lua_script_asset_loading::LuaScriptAsset,
        scene_prefab::ScenePrefabAsset,
        species::species_asset_types::{SpeciesAsset, SpeciesAssets},
        world_definitions::{
            world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
            world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
        },
        world_scenario::{
            world_scenario_asset_set_refresh_and_indexing::index_loaded_world_scenario_assets_by_identifier,
            world_scenario_asset_set_state_and_borrowing_queries::WorldScenarios,
            world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset,
        },
    },
};
use openzt2_interface_state::photos::photo_album_types::AlbumMember;
use openzt2_interface_state::photos::photo_album_types::PhotoAlbum;
use openzt2_interface_state::photos::photo_album_types::DEFAULT_PHOTO_ALBUM_NAME_KEY;
use openzt2_interface_state::photos::photo_capture_types::Photo;
use openzt2_interface_state::photos::photo_challenge_types::PhotoChallenge;
use openzt2_interface_state::photos::photo_challenge_types::PhotoChallengeProgress;
use openzt2_world_state::application_schedule::GameSet;
use openzt2_world_state::game_session_types::WorldSessionMode;
use openzt2_world_state::simulation_time::deterministic_random_stream::ZooSeed;
use openzt2_world_state::simulation_time::simulation_clock_types::ZooCalendar;
use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_world_state::simulation_time::simulation_control_types::SimulationControl;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentIdAllocator;
use openzt2_world_state::world_spawn::world_hydration_completion::WorldLoadFinished;
use openzt2_world_state::world_spawn::world_load_completion_marker::WorldLoadCompleted;
use openzt2_world_state::world_spawn::world_load_failure::WorldLoadFailed;
use openzt2_world_state::world_spawn::world_load_request_acceptance::BeginWorldLoad;
use openzt2_world_state::world_spawn::world_load_request_acceptance::WorldLoadOperationOwner;
use openzt2_world_state::world_spawn::world_membership_types::WorldRoot;
use openzt2_world_state::world_spawn::world_unloading::UnloadWorld;
use openzt2_zoo_state::economy::guest_admission_types::AdmissionPrice;
use openzt2_zoo_state::economy::guest_admission_types::ZooAdmissionsOpen;
use openzt2_zoo_state::economy::money_types::Money;
use openzt2_zoo_state::economy::monthly_finance_types::MonthlyFinanceHistory;
use openzt2_zoo_state::economy::zoo_cash_types::ZooCash;
use openzt2_zoo_state::progression::award_and_progression_fact_types::ScenarioAwardPointTotal;
use openzt2_zoo_state::progression::fame_history_types::FameHistory;
use openzt2_zoo_state::progression::fame_types::Fame;
use openzt2_zoo_state::progression::profile_challenge_types::TotalEndangeredAnimalBirthCount;
use openzt2_zoo_state::progression::rating_types::ZooRating;
use openzt2_zoo_state::progression::unlock_types::UnlockedCatalogueDefinitionSet;
use openzt2_zoo_state::shows::show_platform_upgrade_types::RestoreShowPlatformUpgrade;

use openzt2_interface_state::persistence::profile_types::ProfileOptions;
use openzt2_interface_state::persistence::save_slot_types::SaveSlotId;
use openzt2_interface_state::persistence::save_slot_types::WorldSnapshotLoadedFromSlot;

use super::{
    super::{
        persistence_failure_types::WorldSnapshotPersistenceFailed,
        progression_snapshot_capture::{
            capture_progression_snapshot_records_from_live_world, ProgressionSnapshotCaptureQueries,
        },
        progression_snapshot_section_encoding::append_encoded_progression_snapshot_section,
        simulation_time_snapshot_section::SimulationTimeSnapshotSaveResources,
        snapshot_container_encoding_and_validation::{
            append_encoded_world_snapshot_section_to_container,
            finalize_world_snapshot_container_header_and_checksum,
            initialize_empty_world_snapshot_container,
        },
        snapshot_container_types::{ReusableWorldSnapshotByteBuffer, WorldSnapshotSectionKind},
    },
    economy_snapshot_section_encoding::append_encoded_economy_snapshot_section,
    economy_snapshot_types::EconomySnapshotRecords,
    loaded_snapshot_application_execution::coordinate_pending_snapshot_validation_baseline_world_loading_and_application,
    pending_snapshot_application_types::PendingSnapshotApplication,
    photo_snapshot_section_encoding::append_encoded_photo_snapshot_section,
    photo_snapshot_types::{PhotoChallengeProgressSnapshotRecord, PhotoSnapshotRecord},
    world_snapshot_section_encoding::append_encoded_world_snapshot_section,
    world_snapshot_types::WorldSnapshotRecord,
};

pub(crate) const PROFILE: AssetId = AssetId([9; 16]);
pub(crate) const SCENARIO: AssetId = AssetId([3; 16]);
pub(crate) const FIRST: AssetId = AssetId([1; 16]);
pub(crate) const SECOND: AssetId = AssetId([2; 16]);

pub(crate) fn loading_app() -> (App, Entity) {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        AssetPlugin::default(),
        bevy::state::app::StatesPlugin,
    ))
    .insert_state(GamePhase::Loading)
    .init_resource::<ReusableWorldSnapshotByteBuffer>()
    .init_resource::<Assets<LocalizationAsset>>()
    .init_resource::<LocalizationPrecedenceIndex>()
    .init_resource::<Assets<WorldScenarioDocumentAsset>>()
    .init_resource::<WorldScenarios>()
    .init_resource::<Assets<SpeciesAsset>>()
    .init_resource::<SpeciesAssets>()
    .init_resource::<Assets<WorldDefinitionAsset>>()
    .init_resource::<WorldDefinitions>()
    .init_resource::<Assets<ScenePrefabAsset>>()
    .init_resource::<Assets<Image>>()
    .init_resource::<Fame>()
    .init_resource::<FameHistory>()
    .init_resource::<ZooRating>()
    .init_resource::<UnlockedCatalogueDefinitionSet>()
    .init_resource::<ScenarioAwardPointTotal>()
    .init_resource::<TotalEndangeredAnimalBirthCount>()
    .init_resource::<ZooCash>()
    .init_resource::<AdmissionPrice>()
    .init_resource::<ZooAdmissionsOpen>()
    .init_resource::<MonthlyFinanceHistory>()
    .insert_resource(ProfileOptions {
        profile_identifier: PROFILE,
        ..default()
    })
    .insert_resource(ZooClock {
        tick: 1,
        absolute_day: 0,
        tick_in_day: 1,
    })
    .insert_resource(ZooCalendar {
        year: 2000,
        month: 1,
        day: 1,
    })
    .insert_resource(SimulationControl::new(0, false))
    .insert_resource(ZooSeed(17))
    .add_message::<AssetEvent<WorldScenarioDocumentAsset>>()
    .add_message::<AssetEvent<LuaScriptAsset>>()
    .add_message::<BeginWorldLoad>()
    .add_message::<UnloadWorld>()
    .add_message::<WorldLoadFinished>()
    .add_message::<WorldLoadFailed>()
    .add_message::<WorldSnapshotLoadedFromSlot>()
    .add_message::<WorldSnapshotPersistenceFailed>()
    .add_message::<RestoreShowPlatformUpgrade>()
    .add_plugins(PhotosPlugin)
    .add_systems(PreUpdate, index_loaded_world_scenario_assets_by_identifier)
    .add_systems(
        Update,
        coordinate_pending_snapshot_validation_baseline_world_loading_and_application
            .after(GameSet::Intent),
    );
    let root = app
        .world_mut()
        .spawn((
            WorldRoot { scenario: SCENARIO },
            WorldLoadCompleted,
            Name::new("baseline"),
        ))
        .id();
    let mut allocator = PersistentIdAllocator::new(root);
    // Imported baseline objects precede default storage in the ordinary loader.
    allocator.reserve_imported(root, PersistentId(6)).unwrap();
    app.insert_resource(allocator);

    let species = SpeciesDocument::default();
    let handle = app
        .world_mut()
        .resource_mut::<Assets<SpeciesAsset>>()
        .add(SpeciesAsset::from_test_document(species.clone()));
    app.world_mut()
        .resource_mut::<SpeciesAssets>()
        .index_test_document(handle, &species);
    let definitions = WorldDefinitionDocument {
        simulation_timing: Some(SimulationTimingDefinition {
            fixed_hz: 30,
            ticks_per_day: 300,
            speed_multipliers: vec![],
            calendar: SimulationCalendarPolicy {
                epoch_year: 2000,
                epoch_month: 1,
                epoch_day: 1,
                month_lengths: [30; 12],
                leap: SimulationCalendarLeapPolicy::None,
            },
        }),
        ..default()
    };
    let handle = app
        .world_mut()
        .resource_mut::<Assets<WorldDefinitionAsset>>()
        .add(WorldDefinitionAsset::from_test_document(
            definitions.clone(),
        ));
    app.world_mut()
        .resource_mut::<WorldDefinitions>()
        .index_test_document(handle, &definitions);
    (app, root)
}

pub(crate) fn saved_photo_container(world: &mut World) -> Vec<u8> {
    saved_photo_container_for_scenario(world, SCENARIO)
}

pub(crate) fn saved_photo_container_for_scenario(world: &mut World, scenario: AssetId) -> Vec<u8> {
    let mut state = bevy::ecs::system::SystemState::<(
        ProgressionSnapshotCaptureQueries,
        SimulationTimeSnapshotSaveResources,
    )>::new(world);
    let (progression, time) = state.get(world).unwrap();
    let progression = capture_progression_snapshot_records_from_live_world(&progression).unwrap();
    let time = time
        .capture_validated_simulation_time_snapshot_record()
        .unwrap();
    let photos = [41, 42]
        .into_iter()
        .map(|id| PhotoSnapshotRecord {
            persistent_identifier: PersistentId(id),
            album_persistent_identifier: PersistentId(7),
            album_order: id,
            captured_simulation_tick: 123,
            camera_world_position: Vec3::new(1.0, 2.0, 3.0),
            camera_world_rotation: Quat::IDENTITY,
            score_millipoints: 2000,
            image_width_pixels: 1,
            image_height_pixels: 1,
            rgba8_pixel_bytes: vec![id as u8, 10, 20, 255],
            challenge_photo_file_identifiers: vec![],
        })
        .collect::<Vec<_>>();
    let progress = [
        PhotoChallengeProgressSnapshotRecord {
            challenge_definition_identifier: FIRST,
            completed_by_photo_persistent_identifier: Some(PersistentId(41)),
            rating_stars: 2,
        },
        PhotoChallengeProgressSnapshotRecord {
            challenge_definition_identifier: SECOND,
            completed_by_photo_persistent_identifier: Some(PersistentId(42)),
            rating_stars: 5,
        },
    ];
    let economy = EconomySnapshotRecords {
        zoo_cash: Money(5000),
        admission_price: Money(250),
        admissions_open: true,
        current_calendar_month: 0,
        lifetime_income: Money::ZERO,
        lifetime_expenses: Money::ZERO,
        lifetime_donation_income: Money::ZERO,
        lifetime_users: 0,
        monthly_finance_history_initialized: false,
        monthly_finance_records: vec![],
        facility_economy_records: vec![],
    };
    let mut bytes = Vec::new();
    initialize_empty_world_snapshot_container(&mut bytes, PROFILE);
    for kind in WorldSnapshotSectionKind::ENCODING_ORDER {
        let count = match kind {
            WorldSnapshotSectionKind::World
            | WorldSnapshotSectionKind::Economy
            | WorldSnapshotSectionKind::Progression
            | WorldSnapshotSectionKind::SimulationTime => 1,
            WorldSnapshotSectionKind::Photos => 4,
            _ => 0,
        };
        append_encoded_world_snapshot_section_to_container(&mut bytes, kind, count, |payload| {
            match kind {
                WorldSnapshotSectionKind::World => append_encoded_world_snapshot_section(
                    payload,
                    &WorldSnapshotRecord {
                        scenario_definition_identifier: scenario,
                        requested_world_definition_identifier: SCENARIO,
                        map_definition_identifier: SCENARIO,
                        starting_point_definition_identifier: AssetId([4; 16]),
                        session_mode: WorldSessionMode::Freeform,
                        last_saved_unix_timestamp_milliseconds: 0,
                        display_name: "restored".into(),
                    },
                ),
                WorldSnapshotSectionKind::Animals => payload.extend_from_slice(b"[]"),
                WorldSnapshotSectionKind::Economy => {
                    append_encoded_economy_snapshot_section(payload, &economy)
                }
                WorldSnapshotSectionKind::Progression => {
                    append_encoded_progression_snapshot_section(payload, &progression)
                }
                WorldSnapshotSectionKind::SimulationTime => {
                    time.append_encoded_simulation_time_snapshot_record(payload)
                }
                WorldSnapshotSectionKind::Photos => {
                    append_encoded_photo_snapshot_section(payload, &photos, &progress)
                }
                WorldSnapshotSectionKind::Shows => payload.extend_from_slice(&[0; 12]),
                _ => {}
            }
        })
        .unwrap();
    }
    finalize_world_snapshot_container_header_and_checksum(&mut bytes, 3).unwrap();
    bytes
}

pub(crate) fn prepare_photo_snapshot_application(world: &mut World) {
    let root = world
        .query_filtered::<Entity, With<WorldRoot>>()
        .single(world)
        .unwrap();
    let bytes = saved_photo_container(world);
    world.insert_resource(ReusableWorldSnapshotByteBuffer {
        snapshot_container_bytes: bytes,
        contains_validated_snapshot_container: true,
    });
    let pending = world
        .spawn(PendingSnapshotApplication {
            save_slot_identifier: SaveSlotId(0),
            scenario_definition_identifier: Some(SCENARIO),
            session_mode: Some(WorldSessionMode::Freeform),
            baseline_world_load_requested: true,
            baseline_world_load_completed: false,
            baseline_world_root: Some(root),
            maintenance_prefabs: vec![],
        })
        .id();
    world
        .entity_mut(root)
        .insert(WorldLoadOperationOwner(pending));
}

pub(crate) fn initialize_fixture_album_localization(world: &mut World) {
    world.resource_scope(|world, mut index: Mut<LocalizationPrecedenceIndex>| {
        index.index_test_catalogue(
            &mut world.resource_mut::<Assets<LocalizationAsset>>(),
            LocalizationCatalog {
                localization_entries: vec![LocalizationEntry {
                    localization_entry_identifier: AssetId::from_key(DEFAULT_PHOTO_ALBUM_NAME_KEY),
                    authored_localization_key: DEFAULT_PHOTO_ALBUM_NAME_KEY.into(),
                    plain_localized_text: "Fixture Album".into(),
                    localized_text_format_tokens: vec![],
                    localized_rich_content_nodes: vec![],
                    localized_text_presentation: default(),
                }],
                ..default()
            },
        );
    });
}

pub(crate) fn prepare_unvalidated_snapshot_application(
    world: &mut World,
    bytes: Vec<u8>,
) -> Entity {
    world.insert_resource(ReusableWorldSnapshotByteBuffer {
        snapshot_container_bytes: bytes,
        contains_validated_snapshot_container: false,
    });
    world
        .spawn(PendingSnapshotApplication {
            save_slot_identifier: SaveSlotId(0),
            scenario_definition_identifier: None,
            session_mode: None,
            baseline_world_load_requested: false,
            baseline_world_load_completed: false,
            baseline_world_root: None,
            maintenance_prefabs: vec![],
        })
        .id()
}

#[test]
fn prevalidation_rejection_preserves_the_active_world_and_simulation() {
    use bevy::ecs::system::RunSystemOnce;
    use openzt2_simulation_time::simulation_time_world_lifecycle::remove_simulation_time_after_leaving_world;
    use openzt2_world::world_spawn::world_unloading::despawn_all_world_roots_and_members_when_leaving_ingame;

    let (mut app, root) = loading_app();
    app.insert_state(GamePhase::InGame).add_systems(
        OnExit(GamePhase::InGame),
        (
            despawn_all_world_roots_and_members_when_leaving_ingame,
            remove_simulation_time_after_leaving_world,
        ),
    );
    let operation = prepare_unvalidated_snapshot_application(app.world_mut(), vec![0; 16]);
    let next_identifier = app.world().resource::<PersistentIdAllocator>().next();
    let clock = *app.world().resource::<ZooClock>();
    let calendar = *app.world().resource::<ZooCalendar>();
    let control = *app.world().resource::<SimulationControl>();
    let seed = *app.world().resource::<ZooSeed>();
    // This boundary exercises the coordinator and its deferred commands; the
    // fixture does not install the input/rendering dependencies of Photos UI.
    app.world_mut()
        .run_system_once(
            coordinate_pending_snapshot_validation_baseline_world_loading_and_application,
        )
        .unwrap();
    assert!(matches!(
        app.world().resource::<NextState<GamePhase>>(),
        NextState::Unchanged
    ));
    app.world_mut().run_schedule(StateTransition);
    assert_eq!(
        *app.world().resource::<State<GamePhase>>().get(),
        GamePhase::InGame
    );
    assert!(app.world().get_entity(root).is_ok());
    assert!(app
        .world()
        .resource::<PersistentIdAllocator>()
        .owns_world(root));
    assert_eq!(
        app.world().resource::<PersistentIdAllocator>().next(),
        next_identifier
    );
    assert_eq!(*app.world().resource::<ZooClock>(), clock);
    assert_eq!(*app.world().resource::<ZooCalendar>(), calendar);
    assert_eq!(*app.world().resource::<SimulationControl>(), control);
    assert_eq!(*app.world().resource::<ZooSeed>(), seed);
    assert!(app.world().get_entity(operation).is_err());
    assert_eq!(
        app.world()
            .resource::<Messages<WorldSnapshotPersistenceFailed>>()
            .len(),
        1
    );
    assert!(app.world().resource::<Messages<UnloadWorld>>().is_empty());
    assert!(app
        .world()
        .resource::<Messages<BeginWorldLoad>>()
        .is_empty());
}

#[test]
fn snapshot_replacement_is_requested_after_ingame_teardown_removes_world_resources() {
    use bevy::ecs::system::RunSystemOnce;
    use openzt2_simulation_time::simulation_time_world_lifecycle::remove_simulation_time_after_leaving_world;
    use openzt2_world::world_spawn::world_unloading::despawn_all_world_roots_and_members_when_leaving_ingame;

    let (mut app, old_root) = loading_app();
    app.insert_state(GamePhase::InGame).add_systems(
        OnExit(GamePhase::InGame),
        (
            despawn_all_world_roots_and_members_when_leaving_ingame,
            remove_simulation_time_after_leaving_world,
        ),
    );
    let bytes = saved_photo_container(app.world_mut());
    app.insert_resource(ReusableWorldSnapshotByteBuffer {
        snapshot_container_bytes: bytes,
        contains_validated_snapshot_container: false,
    });
    let pending = app
        .world_mut()
        .spawn(PendingSnapshotApplication {
            save_slot_identifier: SaveSlotId(0),
            scenario_definition_identifier: None,
            session_mode: None,
            baseline_world_load_requested: false,
            baseline_world_load_completed: false,
            baseline_world_root: None,
            maintenance_prefabs: vec![],
        })
        .id();
    app.world_mut()
        .run_system_once(
            coordinate_pending_snapshot_validation_baseline_world_loading_and_application,
        )
        .unwrap();
    assert!(matches!(
        app.world().resource::<NextState<GamePhase>>(),
        NextState::Pending(GamePhase::Loading)
    ));
    app.update();
    assert_eq!(
        *app.world().resource::<State<GamePhase>>().get(),
        GamePhase::Loading
    );
    assert!(app.world().get_entity(old_root).is_err());
    assert!(!app.world().contains_resource::<PersistentIdAllocator>());
    assert!(!app.world().contains_resource::<ZooClock>());
    assert!(!app.world().contains_resource::<ZooCalendar>());
    assert!(!app.world().contains_resource::<SimulationControl>());
    assert!(!app.world().contains_resource::<ZooSeed>());
    assert!(
        app.world()
            .get::<PendingSnapshotApplication>(pending)
            .unwrap()
            .baseline_world_load_requested
    );
    let requests = app.world().resource::<Messages<BeginWorldLoad>>();
    let mut cursor = requests.get_cursor();
    let request = cursor.read(requests).next().unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(request.scenario, SCENARIO);
    assert_eq!(request.profile, PROFILE);
    assert_eq!(request.mode, WorldSessionMode::Freeform);
    assert!(app
        .world()
        .resource::<Messages<WorldSnapshotLoadedFromSlot>>()
        .is_empty());
    assert!(app
        .world()
        .resource::<Messages<WorldSnapshotPersistenceFailed>>()
        .is_empty());
}

pub(crate) fn photo_fixture_catalogue() -> WorldScenarioDocument {
    WorldScenarioDocument {
        maps: vec![WorldMapRecord {
            id: SCENARIO,
            catalogue_order: 0,
            name_key: SCENARIO,
            location: SCENARIO,
            biome: SCENARIO,
            biome_key: SCENARIO,
            size_key: SCENARIO,
            description_key: None,
            thumbnail: SCENARIO,
            environment: SCENARIO,
            camera: SCENARIO,
            terrain: SCENARIO,
            starting_zoo: SCENARIO,
            expansion_pack_filter_identifier: 0,
            supported_game_modes: WorldMapSupportedGameModeFlags::FREEFORM,
        }],
        photo_challenges: vec![PhotoChallengeRule {
            id: FIRST,
            instruction_key: FIRST,
            completion_key: FIRST,
            difficulty: 1,
        }],
        ..default()
    }
}

#[test]
fn loading_initializes_photo_receivers_before_snapshot_reference_validation_and_mutation() {
    let (mut app, root) = loading_app();
    let catalogue = photo_fixture_catalogue();
    // A retained catalogue handle whose asset has not arrived yet.
    let pending_catalogue = app
        .world()
        .resource::<Assets<WorldScenarioDocumentAsset>>()
        .reserve_handle();
    app.world_mut()
        .resource_mut::<WorldScenarios>()
        .queue_test_document(pending_catalogue.clone());
    let bytes = saved_photo_container(app.world_mut());
    app.insert_resource(ReusableWorldSnapshotByteBuffer {
        snapshot_container_bytes: bytes,
        contains_validated_snapshot_container: true,
    });
    let pending = app
        .world_mut()
        .spawn(PendingSnapshotApplication {
            save_slot_identifier: SaveSlotId(0),
            scenario_definition_identifier: Some(SCENARIO),
            session_mode: Some(WorldSessionMode::Freeform),
            baseline_world_load_requested: true,
            baseline_world_load_completed: false,
            baseline_world_root: Some(root),
            maintenance_prefabs: vec![],
        })
        .id();
    app.world_mut()
        .entity_mut(root)
        .insert(WorldLoadOperationOwner(pending));
    app.world_mut().write_message(WorldLoadFinished {
        operation: Some(pending),
        root,
        scenario: SCENARIO,
    });
    app.update();
    app.update();
    assert!(matches!(
        app.world().resource::<NextState<GamePhase>>(),
        NextState::Unchanged
    ));
    assert_eq!(
        *app.world().resource::<State<GamePhase>>().get(),
        GamePhase::Loading
    );
    assert_eq!(app.world().get::<Name>(root).unwrap().as_str(), "baseline");
    assert_eq!(
        app.world_mut()
            .query::<&PhotoChallenge>()
            .iter(app.world())
            .count(),
        0
    );
    assert!(app
        .world()
        .resource::<Messages<WorldSnapshotPersistenceFailed>>()
        .is_empty());
    assert!(app
        .world()
        .resource::<Messages<WorldSnapshotLoadedFromSlot>>()
        .is_empty());

    let mut arriving_asset = Some(WorldScenarioDocumentAsset::from_test_document(catalogue));
    app.add_systems(
        Update,
        (move |mut assets: ResMut<Assets<WorldScenarioDocumentAsset>>| {
            if let Some(asset) = arriving_asset.take() {
                assets.insert(pending_catalogue.id(), asset).unwrap();
            }
        })
        .before(GameSet::Intent),
    );
    let second = WorldScenarioDocument {
        photo_challenges: vec![PhotoChallengeRule {
            id: SECOND,
            instruction_key: SECOND,
            completion_key: SECOND,
            difficulty: 1,
        }],
        ..default()
    };
    let handle = app
        .world_mut()
        .resource_mut::<Assets<WorldScenarioDocumentAsset>>()
        .add(WorldScenarioDocumentAsset::from_test_document(
            second.clone(),
        ));
    app.world_mut()
        .resource_mut::<WorldScenarios>()
        .queue_test_document(handle);
    initialize_fixture_album_localization(app.world_mut());
    app.update();
    // The asset arrived after this frame's indexing pass. Hydration must not
    // stamp the partial index's revision before a complete pass consumes it.
    assert_eq!(app.world().get::<Name>(root).unwrap().as_str(), "baseline");
    assert_eq!(
        app.world_mut()
            .query::<&PhotoChallenge>()
            .iter(app.world())
            .count(),
        0
    );
    assert!(matches!(
        app.world().resource::<NextState<GamePhase>>(),
        NextState::Unchanged
    ));
    app.update();
    assert!(matches!(
        app.world().resource::<NextState<GamePhase>>(),
        NextState::Pending(GamePhase::InGame)
    ));
    assert_eq!(
        *app.world().resource::<State<GamePhase>>().get(),
        GamePhase::Loading
    );
    assert_eq!(app.world().get::<Name>(root).unwrap().as_str(), "restored");
    assert_eq!(app.world().resource::<AdmissionPrice>().0, Money(250));
    assert_eq!(
        app.world()
            .resource::<Messages<WorldSnapshotLoadedFromSlot>>()
            .len(),
        1
    );
    assert!(app
        .world()
        .resource::<Messages<WorldSnapshotPersistenceFailed>>()
        .is_empty());
    let challenges = app
        .world_mut()
        .query::<(&PhotoChallenge, &PhotoChallengeProgress)>()
        .iter(app.world())
        .map(|(challenge, progress)| (challenge.id, challenge.row, *progress))
        .collect::<Vec<_>>();
    assert_eq!(challenges.len(), 2);
    for (id, expected_photo, stars) in [(FIRST, 41, 2), (SECOND, 42, 5)] {
        let (_, row, progress) = challenges
            .iter()
            .find(|(challenge, ..)| *challenge == id)
            .unwrap();
        assert_eq!(*row, 0);
        assert_eq!(progress.rating_stars, stars);
        let photo = progress.completed_by.unwrap();
        assert!(progress.completed);
        assert_eq!(
            app.world().get::<PersistentId>(photo),
            Some(&PersistentId(expected_photo))
        );
        let album = app.world().get::<AlbumMember>(photo).unwrap().0;
        assert_eq!(
            app.world().get::<PersistentId>(album),
            Some(&PersistentId(7))
        );
        assert_eq!(
            app.world().get::<PhotoAlbum>(album).unwrap().profile,
            PROFILE
        );
        let image = &app.world().get::<Photo>(photo).unwrap().image;
        assert_eq!(
            app.world()
                .resource::<Assets<Image>>()
                .get(image)
                .unwrap()
                .data
                .as_deref(),
            Some([expected_photo as u8, 10, 20, 255].as_slice())
        );
    }
}
