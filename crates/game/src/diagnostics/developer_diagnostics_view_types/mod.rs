use bevy::prelude::*;

#[derive(Resource, Reflect)]
#[reflect(Resource)]
pub(super) struct DeveloperDiagnosticsViewState {
    all_views_visible: bool,
    world_inspectors_visible: bool,
    frame_diagnostics_visible: bool,
    infinite_grid_visible: bool,
    physics_wireframes_visible: bool,
    picking_overlay_visible: bool,
}

impl Default for DeveloperDiagnosticsViewState {
    fn default() -> Self {
        Self {
            all_views_visible: true,
            world_inspectors_visible: false,
            frame_diagnostics_visible: true,
            infinite_grid_visible: true,
            physics_wireframes_visible: true,
            picking_overlay_visible: false,
        }
    }
}

impl DeveloperDiagnosticsViewState {
    pub(super) const fn all_views_are_visible(&self) -> bool {
        self.all_views_visible
    }

    pub(super) const fn world_inspectors_are_visible(&self) -> bool {
        self.all_views_visible && self.world_inspectors_visible
    }

    pub(super) const fn frame_diagnostics_are_visible(&self) -> bool {
        self.all_views_visible && self.frame_diagnostics_visible
    }

    pub(super) const fn infinite_grid_is_visible(&self) -> bool {
        self.all_views_visible && self.infinite_grid_visible
    }

    pub(super) const fn physics_wireframes_are_visible(&self) -> bool {
        self.all_views_visible && self.physics_wireframes_visible
    }

    pub(super) const fn picking_overlay_is_visible(&self) -> bool {
        self.all_views_visible && self.picking_overlay_visible
    }

    pub(super) fn toggle_all_views_visibility(&mut self) {
        self.all_views_visible = !self.all_views_visible;
    }

    pub(super) fn toggle_world_inspector_visibility(&mut self) {
        self.world_inspectors_visible = !self.world_inspectors_visible;
    }

    pub(super) fn toggle_frame_diagnostics_visibility(&mut self) {
        self.frame_diagnostics_visible = !self.frame_diagnostics_visible;
    }

    pub(super) fn toggle_infinite_grid_visibility(&mut self) {
        self.infinite_grid_visible = !self.infinite_grid_visible;
    }

    pub(super) fn toggle_physics_wireframe_visibility(&mut self) {
        self.physics_wireframes_visible = !self.physics_wireframes_visible;
    }

    pub(super) fn toggle_picking_overlay_visibility(&mut self) {
        self.picking_overlay_visible = !self.picking_overlay_visible;
    }
}

#[derive(Component)]
pub(super) struct DeveloperDiagnosticsVisual;

#[derive(Component)]
pub(super) struct DeveloperDiagnosticsHotkeyLegend;
