use bevy::{prelude::*, window::PrimaryWindow};
use openzt2_game_assets::graphics_settings_types::GraphicsSettings;
use openzt2_game_assets::graphics_settings_types::ReplaceGraphicsSettingsRequest;

use openzt2_game_assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use openzt2_interface_state::camera::camera_runtime_state_types::ZooCamera;
use openzt2_world_state::application_schedule::GameSet;
use openzt2_zoo_state::environment::environment_presentation_types::EnvironmentLight;

use super::{
    display_settings_types::{
        DisplayMode, DisplaySettings, FramePacing, ReplaceDisplaySettingsRequest,
    },
    SettingsPlugin,
};

fn representative_display_settings() -> DisplaySettings {
    DisplaySettings {
        width: 1280,
        height: 720,
        mode: DisplayMode::Windowed,
        pacing: FramePacing::VSync,
        ui_scale_permille: 1000,
    }
}

fn representative_graphics_settings() -> GraphicsSettings {
    GraphicsSettings::from_source_highest_detail_preset()
}

fn settings_test_application() -> App {
    let mut app = App::new();
    app.init_resource::<Assets<UiDocumentAsset>>()
        .add_message::<
            openzt2_interface_state::ui::authored_ui_activation_contracts::UiNodeActivated,
        >()
        .add_message::<
            openzt2_interface_state::ui::authored_reusable_list_and_table_runtime_types::SetUiListRowCount,
        >()
        .insert_resource(representative_display_settings())
        .insert_resource(representative_graphics_settings())
        .configure_sets(
            Update,
            (
                GameSet::Intent,
                GameSet::Ui,
                GameSet::Presentation,
                GameSet::Diagnostics,
            )
                .chain(),
        )
        .add_plugins(SettingsPlugin);
    app.world_mut().spawn((Window::default(), PrimaryWindow));
    app
}

#[test]
fn accepted_values_map_to_actual_bevy_components() {
    let mut app = settings_test_application();
    let light = app
        .world_mut()
        .spawn((
            EnvironmentLight {
                keyframe: 0,
                target:
                    openzt2_game_data::world_definitions::environment::EnvironmentLightTarget::Object,
                kind_rank: 1,
            },
            DirectionalLight::default(),
        ))
        .id();
    let camera = app.world_mut().spawn(ZooCamera).id();
    let proposed_display = DisplaySettings {
        width: 1920,
        height: 1080,
        mode: DisplayMode::BorderlessFullscreen,
        pacing: FramePacing::Mailbox,
        ui_scale_permille: 1250,
    };
    let proposed_graphics = GraphicsSettings {
        multisample_count: 4,
        ..GraphicsSettings::from_source_medium_detail_preset()
    };
    app.world_mut()
        .write_message(ReplaceDisplaySettingsRequest(proposed_display));
    app.world_mut()
        .write_message(ReplaceGraphicsSettingsRequest(proposed_graphics));
    app.update();
    app.update();

    let window = app
        .world_mut()
        .query_filtered::<&Window, With<PrimaryWindow>>()
        .single(app.world())
        .unwrap();
    // Fullscreen keeps the last windowed size; the monitor dictates the surface.
    assert_eq!(window.resolution.physical_width(), 1280);
    assert_eq!(window.resolution.physical_height(), 720);
    assert!(matches!(
        window.mode,
        bevy::window::WindowMode::BorderlessFullscreen(_)
    ));
    assert_eq!(window.present_mode, bevy::window::PresentMode::Mailbox);
    assert!(
        !app.world()
            .get::<DirectionalLight>(light)
            .unwrap()
            .shadow_maps_enabled
    );
    assert_eq!(app.world().get::<Msaa>(camera), Some(&Msaa::Sample4));
}
