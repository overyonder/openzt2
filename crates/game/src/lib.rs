pub(crate) mod application_lifecycle;
pub(crate) mod application_schedule;
pub mod assets;
#[cfg(feature = "dev-tools")]
mod diagnostics;
pub mod game_session_types;
pub mod plugins;

mod asset_source;
mod launch;
mod missing_game_content_screen;
mod verification;

use verification::verification_launch_arguments::VerificationLaunchRequest;

use application_lifecycle::GamePhase;
use application_schedule::{FixedGameSet, GameSet};
use assets::openzt2_asset_loading_plugin_group::OpenZt2AssetLoadingPluginGroup;
use bevy::{
    app::ScheduleRunnerPlugin,
    asset::{io::AssetSourceId, AssetApp, AssetPlugin},
    picking::mesh_picking::MeshPickingSettings,
    prelude::*,
    render::{
        settings::{RenderCreation, WgpuFeatures, WgpuSettings},
        RenderPlugin,
    },
    time::TimeUpdateStrategy,
    window::{ExitCondition, PresentMode, WindowResolution},
    winit::WinitPlugin,
};
use launch::GameLaunchConfiguration;
use plugins::openzt2_game_plugin_group::OpenZt2GamePluginGroup;

/// Reads the executable arguments, assembles the Bevy application, and runs
/// OpenZT2 until its normal application exit.
///
/// # Errors
///
/// Returns an error when launch arguments, game archives or the user data
/// directory cannot be read.
pub fn run_game_application_from_process_arguments() -> std::io::Result<()> {
    let launch_configuration = GameLaunchConfiguration::read_from_process_arguments()?;
    run_game_application(launch_configuration)
}

fn run_game_application(launch_configuration: GameLaunchConfiguration) -> std::io::Result<()> {
    let GameLaunchConfiguration {
        enabled_z2f_archive_paths,
        verification_request,
    } = launch_configuration;
    let mut game_application = App::new();

    let content_result = enabled_z2f_archive_paths.and_then(|paths| {
        add_enabled_z2f_archives_as_default_asset_source(&mut game_application, paths)
    });
    if let Err(error) = content_result {
        if verification_request.is_none() {
            missing_game_content_screen::show_missing_game_content_screen(&error);
        }
        return Err(error);
    }
    add_game_persistence_directories(&mut game_application, verification_request.as_ref())?;
    add_default_bevy_platform_plugins(&mut game_application, verification_request.as_ref());
    add_openzt2_game_states_and_update_schedules(&mut game_application);
    game_application
        .add_plugins(bevy_hanabi::HanabiPlugin)
        .add_plugins(avian3d::prelude::PhysicsPlugins::default())
        .add_plugins(OpenZt2AssetLoadingPluginGroup)
        .add_plugins(OpenZt2GamePluginGroup);
    add_developer_diagnostics_when_enabled(&mut game_application, verification_request.as_ref());
    add_verification_when_requested(&mut game_application, verification_request);
    execute_short_main_world_systems_without_per_system_task_dispatch(&mut game_application);
    execute_short_render_world_systems_without_per_system_task_dispatch(&mut game_application);
    if game_application.run().is_error() {
        Err(std::io::Error::other(
            "game application exited with an error",
        ))
    } else {
        Ok(())
    }
}

fn execute_short_render_world_systems_without_per_system_task_dispatch(app: &mut App) {
    use bevy::{
        core_pipeline::schedule::{Core2d, Core3d},
        ecs::schedule::{ScheduleLabel, SingleThreadedExecutor},
        render::{renderer::RenderGraph, ExtractSchedule, RenderApp},
    };

    // Preserve the render thread, query-level parallelism and topological
    // command order without dispatching each short pass to another worker.
    let render_app = app.sub_app_mut(RenderApp);
    for label in [
        ExtractSchedule.intern(),
        // macOS needs Bevy's multi-threaded executor to move NonSend
        // `create_surfaces` onto the main thread; NSView access panics elsewhere.
        #[cfg(not(target_vendor = "apple"))]
        bevy::render::Render.intern(),
        RenderGraph.intern(),
        Core3d.intern(),
        Core2d.intern(),
    ] {
        render_app.edit_schedule(label, |schedule| {
            schedule.set_executor(SingleThreadedExecutor::default());
            // Extraction's commands belong to the render world and are applied
            // later by apply_extract_commands, unlike the render schedules.
            schedule.set_apply_final_deferred(label != ExtractSchedule.intern());
        });
    }
}

fn execute_short_main_world_systems_without_per_system_task_dispatch(app: &mut App) {
    use bevy::ecs::schedule::{ScheduleLabel, SingleThreadedExecutor};

    // These schedules contain many short, ordered systems. Run their outer
    // dispatch inline; parallel queries, asset tasks and the render thread keep
    // using Bevy's task pools. Schedule dependency ordering is unchanged.
    for label in [
        First.intern(),
        PreUpdate.intern(),
        Update.intern(),
        PostUpdate.intern(),
        Last.intern(),
        FixedFirst.intern(),
        FixedPreUpdate.intern(),
        FixedUpdate.intern(),
        FixedPostUpdate.intern(),
        FixedLast.intern(),
    ] {
        app.edit_schedule(label, |schedule| {
            schedule.set_executor(SingleThreadedExecutor::default());
            schedule.set_apply_final_deferred(true);
        });
    }
}

fn add_enabled_z2f_archives_as_default_asset_source(
    game_application: &mut App,
    enabled_archive_paths: Vec<std::path::PathBuf>,
) -> std::io::Result<()> {
    let (asset_source, asset_archives) =
        asset_source::create_bevy_asset_source_for_enabled_z2f_archives(enabled_archive_paths)?;
    game_application.register_asset_source(AssetSourceId::Default, asset_source);
    game_application.insert_resource(asset_archives);
    Ok(())
}

fn add_game_persistence_directories(
    game_application: &mut App,
    verification_request: Option<&VerificationLaunchRequest>,
) -> std::io::Result<()> {
    let data_directory = if verification_request.is_some() {
        std::env::temp_dir().join("openzt2-capture")
    } else {
        dirs::data_dir()
            .ok_or_else(|| std::io::Error::other("could not locate the user data directory"))?
            .join("openzt2")
    };
    game_application.insert_resource(
        plugins::persistence::persistence_filesystem_paths::PersistenceFilesystemPaths::new(
            data_directory,
        ),
    );
    Ok(())
}

fn add_default_bevy_platform_plugins(
    game_application: &mut App,
    verification_request: Option<&VerificationLaunchRequest>,
) {
    let default_plugins = DefaultPlugins
        .set(
            bevy::gltf::GltfPlugin::default()
                .add_custom_vertex_attribute(
                    "UV_2",
                    assets::model::custom_vertex_attributes::ATTRIBUTE_UV_2,
                )
                .add_custom_vertex_attribute(
                    "UV_EFFECTS",
                    assets::model::custom_vertex_attributes::ATTRIBUTE_UV_EFFECTS,
                )
                .add_custom_vertex_attribute(
                    "AUXILIARY_VECTOR",
                    assets::model::custom_vertex_attributes::ATTRIBUTE_AUXILIARY_VECTOR,
                )
                .add_custom_vertex_attribute(
                    "BIOME_INDEX",
                    assets::model::custom_vertex_attributes::ATTRIBUTE_BIOME_INDEX,
                )
                .add_custom_vertex_attribute(
                    "GROUND_COVER",
                    assets::model::custom_vertex_attributes::ATTRIBUTE_GROUND_COVER,
                )
                .add_custom_vertex_attribute(
                    "WATER_TYPE",
                    assets::model::custom_vertex_attributes::ATTRIBUTE_WATER_TYPE,
                ),
        )
        .set(RenderPlugin {
            render_creation: RenderCreation::Automatic(Box::new(WgpuSettings {
                features: WgpuFeatures::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES
                    | WgpuFeatures::POLYGON_MODE_LINE,
                ..default()
            })),
            ..default()
        })
        .set(AssetPlugin {
            watch_for_changes_override: Some(true),
            ..default()
        })
        .set(WindowPlugin {
            // Diagnostic captures are genuine offscreen renders: creating
            // a hidden native window still registers compositor surface
            // state and can disrupt the user's desktop.
            // Keep a logical primary window for UI layout and camera systems
            // during offscreen capture. Winit is disabled below, so this ECS
            // window can never become a compositor/native window.
            primary_window: Some(Window {
                title: "Open Zoo".into(),
                resolution: WindowResolution::new(1600, 900),
                present_mode: PresentMode::AutoNoVsync,
                ..default()
            }),
            exit_condition: if verification_request.is_some() {
                ExitCondition::DontExit
            } else {
                ExitCondition::OnAllClosed
            },
            ..default()
        });
    if verification_request.is_some_and(|request| !request.renders_to_window()) {
        // Disabling Winit guarantees headless runs never create a compositor window.
        // Simulation advances 1/60 s per frame, and frames are paced at 60 Hz because
        // UI animation and pointer activation run on real time.
        let frame_duration = std::time::Duration::from_secs_f64(1.0 / 60.0);
        game_application
            .add_plugins(default_plugins.disable::<WinitPlugin>())
            .insert_resource(TimeUpdateStrategy::ManualDuration(frame_duration))
            .add_plugins(ScheduleRunnerPlugin::run_loop(frame_duration));
    } else {
        game_application.add_plugins(default_plugins);
    }
    game_application.insert_resource(MeshPickingSettings {
        require_markers: true,
        ..default()
    });
}

fn add_openzt2_game_states_and_update_schedules(game_application: &mut App) {
    game_application
        .init_state::<GamePhase>()
        .insert_resource(Time::<Fixed>::from_hz(30.0))
        .insert_resource(Time::<Virtual>::from_max_delta(
            std::time::Duration::from_secs_f64(2.0 / 30.0),
        ))
        .configure_sets(
            Update,
            (
                GameSet::Input,
                GameSet::Intent,
                GameSet::Ui,
                GameSet::Presentation,
                GameSet::Diagnostics,
            )
                .chain(),
        )
        .configure_sets(
            FixedUpdate,
            (
                FixedGameSet::Clock,
                FixedGameSet::Think,
                FixedGameSet::Navigate,
                FixedGameSet::Act,
                FixedGameSet::Economy,
                FixedGameSet::Cleanup,
            )
                .chain()
                .run_if(in_state(GamePhase::InGame))
                .run_if(
                    plugins::simulation_time::simulation_control_application::simulation_is_running,
                ),
        );
}

fn add_developer_diagnostics_when_enabled(
    game_application: &mut App,
    verification_request: Option<&VerificationLaunchRequest>,
) {
    #[cfg(feature = "dev-tools")]
    if verification_request.is_none() {
        game_application.add_plugins(diagnostics::OpenZt2DeveloperDiagnosticsPlugin);
    }
    #[cfg(not(feature = "dev-tools"))]
    let _ = (game_application, verification_request);
}

fn add_verification_when_requested(
    game_application: &mut App,
    verification_request: Option<VerificationLaunchRequest>,
) {
    match verification_request {
        Some(VerificationLaunchRequest::Journey {
            journey,
            output_directory,
            windowed,
        }) => {
            game_application.add_plugins(verification::VerificationJourneyPlugin {
                journey,
                output_directory,
                windowed,
            });
        }
        Some(VerificationLaunchRequest::MapLoad { map_index }) => {
            game_application.add_plugins(
                verification::verification_map_load::VerificationMapLoadPlugin { map_index },
            );
        }
        None => {}
    }
}
