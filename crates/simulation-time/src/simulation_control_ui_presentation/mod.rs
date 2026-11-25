use bevy::prelude::*;
use openzt2_interface_state::ui::authored_ui_selection_state::UiSelected;
use openzt2_ui::authored_ui_layout_participation::UiAuthoredLayoutDisplay;
use openzt2_world_state::{
    simulation_time::simulation_control_types::{
        SimulationPaused, UiSimulationPauseToggle, UiSimulationPausedVisibility,
    },
    world_spawn::world_membership_types::WorldRoot,
};

type ChangedWorldPause = (With<WorldRoot>, Changed<SimulationPaused>);
type NewPauseToggles = (
    With<UiSimulationPauseToggle>,
    With<UiSelected>,
    Or<(Added<UiSimulationPauseToggle>, Added<UiSelected>)>,
);
type ChangedPauseIndicators = (
    With<UiSimulationPausedVisibility>,
    Or<(
        Added<UiSimulationPausedVisibility>,
        Changed<UiAuthoredLayoutDisplay>,
        Added<Node>,
        Added<Visibility>,
    )>,
);

pub(super) fn pause_toggles_changed(
    pause: Query<(), ChangedWorldPause>,
    toggles: Query<(), NewPauseToggles>,
) -> bool {
    !pause.is_empty() || !toggles.is_empty()
}

pub(super) fn pause_indicators_changed(
    pause: Query<(), ChangedWorldPause>,
    indicators: Query<(), ChangedPauseIndicators>,
) -> bool {
    !pause.is_empty() || !indicators.is_empty()
}

pub(super) fn project_simulation_pause_toggle_selection(
    roots: Query<Ref<SimulationPaused>, With<WorldRoot>>,
    mut toggles: Query<(Ref<UiSimulationPauseToggle>, &mut UiSelected)>,
) {
    let Ok(control) = roots.single() else {
        return;
    };
    for (toggle, mut selected) in &mut toggles {
        if (control.is_changed() || toggle.is_added() || selected.is_added())
            && selected.0 != control.0
        {
            // The authored on/off actions still run when the selection changes.
            selected.0 = control.0;
        }
    }
}

pub(super) fn project_simulation_paused_node_visibility(
    roots: Query<Ref<SimulationPaused>, With<WorldRoot>>,
    mut indicators: Query<(
        Ref<UiSimulationPausedVisibility>,
        Ref<UiAuthoredLayoutDisplay>,
        &mut Node,
        &mut Visibility,
    )>,
) {
    let Ok(control) = roots.single() else {
        return;
    };
    for (indicator, display, mut node, mut visibility) in &mut indicators {
        if !(control.is_changed()
            || indicator.is_added()
            || display.is_changed()
            || node.is_added()
            || visibility.is_added())
        {
            continue;
        }
        let (next_visibility, next_display) = if control.0 {
            (Visibility::Inherited, display.authored_display())
        } else {
            (Visibility::Hidden, Display::None)
        };
        visibility.set_if_neq(next_visibility);
        if node.display != next_display {
            node.display = next_display;
        }
    }
}
