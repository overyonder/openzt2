use bevy::state::app::StatesPlugin;
use bevy::{app::AppExit, prelude::*};
use openzt2_game_data::world_scenario::{
    WorldMapRecord, WorldMapSupportedGameModeFlags, WorldScenarioDocument,
};
use openzt2_game_data::AssetId;

use openzt2_game_assets::application_lifecycle::GamePhase;
use openzt2_game_assets::world_scenario::world_scenario_asset_set_state_and_borrowing_queries::WorldScenarios;
use openzt2_game_assets::world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset;
use openzt2_interface_state::persistence::profile_types::ProfileIndex;
use openzt2_interface_state::ui::ui_document_lifecycle_contracts::ShowUiRole;
use openzt2_interface_state::ui::ui_document_lifecycle_contracts::UiRoleRequests;
use openzt2_world_state::game_session_types::WorldSessionMode;
use openzt2_world_state::world_spawn::world_load_request_acceptance::BeginWorldLoad;

use openzt2_interface_state::shell::shell_screen_presentation_types::MainMenuBackdrop;
use openzt2_interface_state::shell::shell_selection_types::ShellScreen;
use openzt2_interface_state::shell::shell_selection_types::ShellSelection;

use super::{
    map_selection_screen_lifecycle::{
        create_or_reuse_selected_mode_map_selection_screen,
        despawn_retained_map_selection_screen_after_world_activation,
    },
    shell_navigation_and_world_launch::{
        begin_loading_selected_world, select_requested_play_mode_and_enter_map_selection,
    },
    shell_navigation_request_types::{SelectWorldSessionMode, StartSelectedWorld},
};

fn id(byte: u8) -> AssetId {
    AssetId([byte; 16])
}

fn routing_app(selection: ShellSelection, profile: Option<AssetId>) -> App {
    let mut app = App::new();
    app.add_plugins(StatesPlugin)
        .insert_state(GamePhase::MapSelection)
        .insert_resource(selection)
        .insert_resource(ProfileIndex {
            profile_records: Vec::new(),
            selected_profile_identifier: profile,
        })
        .init_resource::<Assets<WorldScenarioDocumentAsset>>()
        .init_resource::<WorldScenarios>()
        .init_resource::<openzt2_zoo_state::progression::profile_scenario_variables::ProfileScenarioVariables>()
        .add_message::<StartSelectedWorld>()
        .add_message::<BeginWorldLoad>()
        .add_message::<ShowUiRole>()
        .add_message::<AppExit>()
        .add_systems(Update, begin_loading_selected_world);
    app
}

#[test]
fn selecting_mode_clears_stale_world_and_enters_map_selection() {
    let mut app = routing_app(
        ShellSelection {
            mode: Some(WorldSessionMode::Campaign),
            scenario: Some(id(9)),
            starting_cash_cents: Some(123),
        },
        None,
    );
    app.add_message::<SelectWorldSessionMode>()
        .add_systems(Update, select_requested_play_mode_and_enter_map_selection);
    app.world_mut()
        .resource_mut::<Messages<SelectWorldSessionMode>>()
        .write(SelectWorldSessionMode(WorldSessionMode::Freeform));

    app.update();

    assert_eq!(
        *app.world().resource::<ShellSelection>(),
        ShellSelection {
            mode: Some(WorldSessionMode::Freeform),
            scenario: None,
            starting_cash_cents: None,
        }
    );
    assert!(matches!(
        app.world().resource::<NextState<GamePhase>>(),
        &NextState::Pending(GamePhase::MapSelection)
    ));
}

#[test]
fn valid_start_emits_exactly_one_world_load() {
    let scenario = id(3);
    let mut app = routing_app(
        ShellSelection {
            mode: Some(WorldSessionMode::Challenge),
            scenario: Some(scenario),
            starting_cash_cents: Some(3_000_000),
        },
        Some(id(1)),
    );
    let terrain = id(4);
    let document = WorldScenarioDocument {
        maps: vec![WorldMapRecord {
            id: scenario,
            catalogue_order: 0,
            name_key: id(5),
            location: id(6),
            biome: id(7),
            biome_key: id(8),
            size_key: id(9),
            description_key: None,
            thumbnail: id(10),
            environment: id(11),
            camera: id(12),
            terrain,
            starting_zoo: id(14),
            expansion_pack_filter_identifier: 0,
            supported_game_modes: WorldMapSupportedGameModeFlags::CHALLENGE,
        }],
        ..default()
    };
    let handle = app
        .world_mut()
        .resource_mut::<Assets<WorldScenarioDocumentAsset>>()
        .add(WorldScenarioDocumentAsset::from_test_document(
            document.clone(),
        ));
    app.world_mut()
        .resource_mut::<WorldScenarios>()
        .index_test_document(handle, &document);
    app.world_mut()
        .resource_mut::<Messages<StartSelectedWorld>>()
        .write(StartSelectedWorld);
    let mut cursor = app
        .world()
        .resource::<Messages<BeginWorldLoad>>()
        .get_cursor();

    app.update();

    let loads = app.world().resource::<Messages<BeginWorldLoad>>();
    let load = cursor.read(loads).collect::<Vec<_>>();
    assert_eq!(load.len(), 1);
    assert_eq!(load[0].scenario, scenario);
    assert_eq!(load[0].mode, WorldSessionMode::Challenge);
    assert_eq!(load[0].profile, id(1));
    assert_eq!(load[0].starting_cash_cents, Some(3_000_000));
}

#[test]
fn failed_load_reuses_retained_map_selection() {
    let mut app = App::new();
    app.insert_resource(ShellSelection {
        mode: Some(WorldSessionMode::Freeform),
        scenario: Some(id(3)),
        starting_cash_cents: None,
    })
    .init_resource::<UiRoleRequests>()
    .init_resource::<NextState<GamePhase>>()
    .add_systems(Update, create_or_reuse_selected_mode_map_selection_screen);
    let screen = app
        .world_mut()
        .spawn((
            ShellScreen::Globe {
                mode: WorldSessionMode::Freeform,
            },
            Visibility::Inherited,
        ))
        .id();
    app.world_mut()
        .spawn((MainMenuBackdrop, Visibility::Inherited, ChildOf(screen)));

    app.update();

    assert_eq!(
        app.world()
            .iter_entities()
            .filter(|entity| entity.contains::<ShellScreen>())
            .count(),
        1
    );
    assert_eq!(
        app.world()
            .iter_entities()
            .filter(|entity| entity.contains::<MainMenuBackdrop>())
            .count(),
        1
    );
}

#[test]
fn completed_load_removes_retained_map_selection() {
    let mut app = App::new();
    app.add_systems(
        Update,
        despawn_retained_map_selection_screen_after_world_activation,
    );
    let screen = app
        .world_mut()
        .spawn((
            ShellScreen::Globe {
                mode: WorldSessionMode::Freeform,
            },
            Visibility::Inherited,
        ))
        .id();
    app.world_mut()
        .spawn((MainMenuBackdrop, Visibility::Inherited, ChildOf(screen)));

    app.update();

    assert_eq!(
        app.world()
            .iter_entities()
            .filter(|entity| entity.contains::<ShellScreen>())
            .count(),
        0
    );
    assert_eq!(
        app.world()
            .iter_entities()
            .filter(|entity| entity.contains::<MainMenuBackdrop>())
            .count(),
        0
    );
}

#[test]
fn campaign_next_and_retry_unload_before_start_and_preserve_locks() {
    use super::shell_navigation_and_world_launch::continue_campaign_after_world_exit;
    use super::shell_navigation_request_types::ContinueCampaign;
    use openzt2_game_data::world_scenario::{
        CampaignScenarioRecord, CampaignScenarioState, ScenarioCampaignRecord,
        ScenarioDefinitionRecord,
    };
    use openzt2_world::world_spawn::world_unloading::despawn_all_world_roots_and_members_when_leaving_ingame;
    use openzt2_world_state::world_spawn::world_membership_types::{WorldMember, WorldRoot};
    use openzt2_zoo_state::progression::profile_scenario_variables::ProfileScenarioVariables;

    // The same authored action starts the next mission or retries the current one.
    for target in [id(2), id(3)] {
        let mut app = routing_app(
            ShellSelection {
                mode: Some(WorldSessionMode::Campaign),
                scenario: Some(id(2)),
                starting_cash_cents: Some(99),
            },
            Some(id(1)),
        );
        app.insert_state(GamePhase::InGame)
            .add_message::<ContinueCampaign>()
            .add_systems(
                Update,
                continue_campaign_after_world_exit.before(begin_loading_selected_world),
            )
            .add_systems(
                OnExit(GamePhase::InGame),
                despawn_all_world_roots_and_members_when_leaving_ingame,
            );
        let document = WorldScenarioDocument {
            maps: vec![WorldMapRecord {
                id: id(4),
                catalogue_order: 0,
                name_key: id(5),
                location: id(6),
                biome: id(7),
                biome_key: id(8),
                size_key: id(9),
                description_key: None,
                thumbnail: id(10),
                environment: id(11),
                camera: id(12),
                terrain: id(13),
                starting_zoo: id(14),
                expansion_pack_filter_identifier: 0,
                supported_game_modes: WorldMapSupportedGameModeFlags::CAMPAIGN,
            }],
            campaigns: vec![ScenarioCampaignRecord {
                id: id(15),
                name_key: id(15),
                description_key: id(15),
                scenarios: [id(2), id(3)]
                    .into_iter()
                    .map(|scenario| CampaignScenarioRecord {
                        id: scenario,
                        initial_state: if scenario == id(2) {
                            CampaignScenarioState::Unlocked
                        } else {
                            CampaignScenarioState::Locked
                        },
                        map: id(4),
                        starting_zoo: id(14),
                        starting_cash_cents: 50_000,
                        difficulty: id(16),
                        name_key: scenario,
                        description_key: scenario,
                    })
                    .collect(),
            }],
            scenarios: [id(2), id(3)]
                .into_iter()
                .map(|scenario| ScenarioDefinitionRecord {
                    id: scenario,
                    name_key: scenario,
                    description_key: scenario,
                    objectives: Vec::new(),
                    time_limit_ticks: 0,
                    flags: default(),
                    ui_modifications: Vec::new(),
                })
                .collect(),
            ..default()
        };
        let handle = app
            .world_mut()
            .resource_mut::<Assets<WorldScenarioDocumentAsset>>()
            .add(WorldScenarioDocumentAsset::from_test_document(
                document.clone(),
            ));
        app.world_mut()
            .resource_mut::<WorldScenarios>()
            .index_test_document(handle, &document);
        let old_world = app.world_mut().spawn(WorldRoot { scenario: id(2) }).id();
        let old_member = app.world_mut().spawn(WorldMember { root: old_world }).id();
        let mut loads = app
            .world()
            .resource::<Messages<BeginWorldLoad>>()
            .get_cursor();
        app.update();
        if target == id(3) {
            app.world_mut().write_message(ContinueCampaign(target));
            app.update();
            app.update();
            assert_eq!(
                *app.world().resource::<State<GamePhase>>().get(),
                GamePhase::InGame
            );
            assert!(app.world().entities().contains(old_world));
            assert!(loads
                .read(app.world().resource::<Messages<BeginWorldLoad>>())
                .next()
                .is_none());
            app.world_mut()
                .resource_mut::<ProfileScenarioVariables>()
                .set(target, Some("unlocked"))
                .unwrap();
        }
        app.world_mut().write_message(ContinueCampaign(target));
        app.update();
        assert!(app.world().entities().contains(old_world));
        assert!(loads
            .read(app.world().resource::<Messages<BeginWorldLoad>>())
            .next()
            .is_none());
        app.update();
        assert!(!app.world().entities().contains(old_world));
        assert!(!app.world().entities().contains(old_member));
        let emitted: Vec<_> = loads
            .read(app.world().resource::<Messages<BeginWorldLoad>>())
            .collect();
        assert_eq!(emitted.len(), 1);
        assert_eq!(emitted[0].scenario, target);
        assert_eq!(emitted[0].mode, WorldSessionMode::Campaign);
        assert_eq!(emitted[0].profile, id(1));
        assert_eq!(emitted[0].starting_cash_cents, None);
        assert!(matches!(
            app.world().resource::<NextState<GamePhase>>(),
            NextState::Pending(GamePhase::Loading)
        ));
        app.update();
        assert_eq!(
            *app.world().resource::<State<GamePhase>>().get(),
            GamePhase::Loading
        );
        assert!(loads
            .read(app.world().resource::<Messages<BeginWorldLoad>>())
            .next()
            .is_none());
    }
}
