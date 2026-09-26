use avian3d::debug_render::PhysicsGizmos;
use bevy::{
    dev_tools::{
        fps_overlay::FpsOverlayConfig, infinite_grid::InfiniteGrid, picking_debug::DebugPickingMode,
    },
    prelude::*,
};

use super::developer_diagnostics_view_types::{
    DeveloperDiagnosticsHotkeyLegend, DeveloperDiagnosticsViewState, DeveloperDiagnosticsVisual,
};

pub(super) fn developer_world_inspectors_are_visible(
    views: Res<DeveloperDiagnosticsViewState>,
) -> bool {
    views.world_inspectors_are_visible()
}

pub(super) fn toggle_developer_diagnostics_views_from_hotkeys(
    keys: Res<ButtonInput<KeyCode>>,
    mut views: ResMut<DeveloperDiagnosticsViewState>,
) {
    if keys.just_pressed(KeyCode::F3) {
        views.toggle_all_views_visibility();
    }
    if keys.just_pressed(KeyCode::F4) {
        views.toggle_physics_wireframe_visibility();
    }
    if keys.just_pressed(KeyCode::F5) {
        views.toggle_world_inspector_visibility();
    }
    if keys.just_pressed(KeyCode::F6) {
        views.toggle_frame_diagnostics_visibility();
    }
    if keys.just_pressed(KeyCode::F7) {
        views.toggle_infinite_grid_visibility();
    }
    if keys.just_pressed(KeyCode::F9) {
        views.toggle_picking_overlay_visibility();
    }
}

pub(super) fn synchronize_developer_diagnostics_view_visibility(
    views: Res<DeveloperDiagnosticsViewState>,
    mut picking_mode: ResMut<DebugPickingMode>,
    mut fps_overlay: ResMut<FpsOverlayConfig>,
    mut gizmos: ResMut<GizmoConfigStore>,
    mut overlays: Query<(&mut Visibility, Option<&InfiniteGrid>), With<DeveloperDiagnosticsVisual>>,
) {
    if !views.is_changed() {
        return;
    }

    *picking_mode = if views.picking_overlay_is_visible() {
        DebugPickingMode::Normal
    } else {
        DebugPickingMode::Disabled
    };
    fps_overlay.enabled = views.frame_diagnostics_are_visible();
    fps_overlay.frame_time_graph_config.enabled = views.frame_diagnostics_are_visible();

    let (physics_gizmo_config, _) = gizmos.config_mut::<PhysicsGizmos>();
    physics_gizmo_config.enabled = views.physics_wireframes_are_visible();

    for (mut visibility, infinite_grid) in &mut overlays {
        let show = infinite_grid.is_some() && views.infinite_grid_is_visible();
        *visibility = if show {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
}

pub(super) fn spawn_developer_diagnostics_visuals(mut commands: Commands) {
    commands.spawn((
        InfiniteGrid,
        Visibility::Inherited,
        DeveloperDiagnosticsVisual,
    ));
}

pub(super) fn spawn_developer_diagnostics_hotkey_legend(
    mut commands: Commands,
    views: Res<DeveloperDiagnosticsViewState>,
) {
    commands.spawn((
        DeveloperDiagnosticsHotkeyLegend,
        Name::new("Diagnostics Hotkey Legend"),
        Text::new(developer_diagnostics_hotkey_legend_text(&views)),
        TextFont {
            font_size: FontSize::Px(13.0),
            ..default()
        },
        TextColor(Color::srgb(0.92, 0.95, 0.96)),
        Node {
            position_type: PositionType::Absolute,
            top: px(12.0),
            right: px(12.0),
            padding: UiRect::all(px(10.0)),
            border: UiRect::all(px(1.0)),
            border_radius: BorderRadius::all(px(4.0)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.025, 0.035, 0.04, 0.88)),
        BorderColor::all(Color::srgba(0.55, 0.7, 0.72, 0.8)),
        GlobalZIndex(i32::MAX - 1),
        Pickable::IGNORE,
    ));
}

pub(super) fn update_developer_diagnostics_hotkey_legend(
    views: Res<DeveloperDiagnosticsViewState>,
    mut legend: Query<&mut Text, With<DeveloperDiagnosticsHotkeyLegend>>,
) {
    if !views.is_changed() {
        return;
    }
    let text = developer_diagnostics_hotkey_legend_text(&views);
    for mut legend in &mut legend {
        **legend = text.clone();
    }
}

fn developer_diagnostics_hotkey_legend_text(views: &DeveloperDiagnosticsViewState) -> String {
    format!(
        "DEVELOPER VIEWS\n\
         F3  Master views       {}\n\
         F4  Physics wireframes {}\n\
         F5  World inspectors   {}\n\
         F6  Frame diagnostics  {}\n\
         F7  Infinite grid      {}\n\
        F9  Picking overlay    {}",
        visible_or_hidden(views.all_views_are_visible()),
        visible_or_hidden(views.physics_wireframes_are_visible()),
        visible_or_hidden(views.world_inspectors_are_visible()),
        visible_or_hidden(views.frame_diagnostics_are_visible()),
        visible_or_hidden(views.infinite_grid_is_visible()),
        visible_or_hidden(views.picking_overlay_is_visible()),
    )
}

const fn visible_or_hidden(visible: bool) -> &'static str {
    if visible {
        "ON"
    } else {
        "OFF"
    }
}
