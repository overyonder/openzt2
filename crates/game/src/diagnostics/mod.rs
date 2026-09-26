//! Native developer views ported from Gigaya's Bevy diagnostics setup.

mod developer_diagnostics_egui_context_ownership;
mod developer_diagnostics_view_operations;
mod developer_diagnostics_view_types;
mod terrain;

use avian3d::debug_render::{PhysicsDebugPlugin, PhysicsGizmos};
use bevy::{
    dev_tools::{
        fps_overlay::{FpsOverlayConfig, FpsOverlayPlugin, FrameTimeGraphConfig},
        infinite_grid::InfiniteGridPlugin,
        picking_debug::{DebugPickingMode, DebugPickingPlugin},
        states::log_transitions,
    },
    diagnostic::{EntityCountDiagnosticsPlugin, FrameTimeDiagnosticsPlugin},
    prelude::*,
};
use bevy_inspector_egui::{
    bevy_egui::{EguiGlobalSettings, EguiPlugin, EguiPreUpdateSet},
    quick::{AssetInspectorPlugin, StateInspectorPlugin, WorldInspectorPlugin},
};

use crate::application_lifecycle::GamePhase;

use developer_diagnostics_view_types::DeveloperDiagnosticsViewState;

pub(crate) struct OpenZt2DeveloperDiagnosticsPlugin;

impl Plugin for OpenZt2DeveloperDiagnosticsPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<GamePhase>()
            .register_type::<DeveloperDiagnosticsViewState>()
            .init_resource::<DeveloperDiagnosticsViewState>()
            .add_plugins((
                EguiPlugin::default(),
                WorldInspectorPlugin::new().run_if(
                    developer_diagnostics_view_operations::developer_world_inspectors_are_visible,
                ),
                StateInspectorPlugin::<GamePhase>::default().run_if(
                    developer_diagnostics_view_operations::developer_world_inspectors_are_visible,
                ),
                AssetInspectorPlugin::<StandardMaterial>::default().run_if(
                    developer_diagnostics_view_operations::developer_world_inspectors_are_visible,
                ),
                FrameTimeDiagnosticsPlugin::default(),
                EntityCountDiagnosticsPlugin::default(),
                InfiniteGridPlugin,
                FpsOverlayPlugin {
                    config: FpsOverlayConfig {
                        frame_time_graph_config: FrameTimeGraphConfig::target_fps(144.0),
                        ..default()
                    },
                },
                DebugPickingPlugin,
                PhysicsDebugPlugin,
            ))
            .insert_resource(EguiGlobalSettings {
                auto_create_primary_context: false,
                ..default()
            })
            .insert_gizmo_config(
                PhysicsGizmos {
                    aabb_color: Some(Color::WHITE),
                    ..default()
                },
                GizmoConfig::default(),
            )
            .insert_resource(DebugPickingMode::Disabled)
            .add_systems(
                Startup,
                (
                    log_transitions::<GamePhase>,
                    developer_diagnostics_view_operations::spawn_developer_diagnostics_visuals,
                    developer_diagnostics_view_operations::spawn_developer_diagnostics_hotkey_legend,
                ),
            )
            .add_systems(
                Update,
                (
                    developer_diagnostics_view_operations::toggle_developer_diagnostics_views_from_hotkeys,
                    developer_diagnostics_view_operations::synchronize_developer_diagnostics_view_visibility,
                    developer_diagnostics_view_operations::update_developer_diagnostics_hotkey_legend,
                    terrain::synchronize_retained_terrain_wireframe_gizmo_assets,
                    terrain::synchronize_retained_terrain_wireframe_gizmo_visibility,
                )
                    .chain(),
            );
        app.add_systems(
            PreUpdate,
            developer_diagnostics_egui_context_ownership::own_primary_egui_context_on_shell_camera
                .before(EguiPreUpdateSet::BeginPass),
        );
    }
}
