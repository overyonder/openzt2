use bevy::state::app::StatesPlugin;
use bevy::{app::AppExit, prelude::*};
use openzt2_game_data::world_scenario::{
    WorldMapRecord, WorldMapSupportedGameModeFlags, WorldScenarioDocument,
};
use openzt2_game_data::AssetId;

use crate::application_lifecycle::GamePhase;
use crate::assets::world_scenario::world_scenario_asset_set_state_and_borrowing_queries::WorldScenarios;
use crate::assets::world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset;
use crate::game_session_types::WorldSessionMode;
use crate::plugins::persistence::profile_types::ProfileIndex;
use crate::plugins::ui::ui_document_lifecycle_contracts::ShowUiRole;
use crate::plugins::ui::ui_document_lifecycle_contracts::UiRoleRequests;
use crate::plugins::world_spawn::world_load_request_acceptance::BeginWorldLoad;

use super::{
    map_selection_screen_lifecycle::{
        create_or_reuse_selected_mode_map_selection_screen,
        despawn_retained_map_selection_screen_after_world_activation,
    },
    shell_navigation_and_world_launch::{
        begin_loading_selected_world, select_requested_play_mode_and_enter_map_selection,
    },
    shell_navigation_request_types::{SelectWorldSessionMode, StartSelectedWorld},
    shell_screen_presentation_types::MainMenuBackdrop,
    shell_selection_types::{ShellScreen, ShellSelection},
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
