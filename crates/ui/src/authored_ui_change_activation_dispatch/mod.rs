use bevy::prelude::*;
use openzt2_game_data::ui_document::action::UiTrigger;

use openzt2_interface_state::input::input_types::ActionSource;

use super::authored_ui_focus_state::UiFocusScope;
use super::authored_ui_node_projection_components::UiValue;
use super::authored_ui_selection_state::UiSelected;
use openzt2_interface_state::ui::authored_ui_activation_contracts::UiNodeActivated;

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct UiPreviousFocus(pub(super) Option<Entity>);

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct UiPreviousVisibility(pub(super) bool);

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiPreviousSelection(bool);

impl UiPreviousSelection {
    pub fn from_current_selection(selected: bool) -> Self {
        Self(selected)
    }

    pub fn synchronize_with_non_authored_selection_change(&mut self, selected: bool) {
        self.0 = selected;
    }
}

pub(super) fn apply_explicit_authored_selection(
    selected: &mut UiSelected,
    previous: &mut UiPreviousSelection,
    active: bool,
    is_type_list: bool,
) -> Option<UiTrigger> {
    let changed = selected.0 != active;
    selected.0 = active;
    previous.synchronize_with_non_authored_selection_change(active);
    (changed || (is_type_list && active)).then_some(if active {
        UiTrigger::On
    } else {
        UiTrigger::Off
    })
}

pub(super) fn dispatch_authored_focus_change_activations(
    mut scopes: Query<(&UiFocusScope, &mut UiPreviousFocus), Changed<UiFocusScope>>,
    mut activated: MessageWriter<UiNodeActivated>,
) {
    for (scope, mut previous) in &mut scopes {
        if previous.0 == scope.focused {
            continue;
        }
        if let Some(node) = previous.0 {
            activated.write(UiNodeActivated {
                source: ActionSource::System,
                node,
                trigger: UiTrigger::Blur,
            });
        }
        if let Some(node) = scope.focused {
            activated.write(UiNodeActivated {
                source: ActionSource::System,
                node,
                trigger: UiTrigger::Focus,
            });
        }
        previous.0 = scope.focused;
    }
}

pub(super) fn dispatch_authored_value_change_activations(
    values: Query<(Entity, Ref<UiValue>)>,
    mut activated: MessageWriter<UiNodeActivated>,
) {
    for (node, value) in &values {
        if value.is_changed() && !value.is_added() {
            activated.write(UiNodeActivated {
                source: ActionSource::System,
                node,
                trigger: UiTrigger::Change,
            });
        }
    }
}

pub(super) fn dispatch_authored_visibility_change_activations(
    mut nodes: Query<(Entity, &Visibility, &mut UiPreviousVisibility), Changed<Visibility>>,
    mut activated: MessageWriter<UiNodeActivated>,
) {
    for (node, visibility, mut previous) in &mut nodes {
        let visible = *visibility != Visibility::Hidden;
        if previous.0 == visible {
            continue;
        }
        previous.0 = visible;
        activated.write(UiNodeActivated {
            source: ActionSource::System,
            node,
            trigger: if visible {
                UiTrigger::Show
            } else {
                UiTrigger::Hide
            },
        });
    }
}

pub(super) fn dispatch_authored_selection_change_activations(
    mut nodes: Query<(Entity, &UiSelected, &mut UiPreviousSelection), Changed<UiSelected>>,
    mut activated: MessageWriter<UiNodeActivated>,
) {
    for (node, selected, mut previous) in &mut nodes {
        if previous.0 == selected.0 {
            continue;
        }
        previous.0 = selected.0;
        activated.write(UiNodeActivated {
            source: ActionSource::System,
            node,
            trigger: if selected.0 {
                UiTrigger::On
            } else {
                UiTrigger::Off
            },
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_frame_selection_cycle_keeps_each_activation_without_observer_echo() {
        let mut app = App::new();
        app.add_message::<UiNodeActivated>()
            .add_systems(Update, dispatch_authored_selection_change_activations);
        let node = app
            .world_mut()
            .spawn((
                UiSelected(true),
                UiPreviousSelection::from_current_selection(true),
            ))
            .id();
        for active in [false, true] {
            let trigger = {
                let world = app.world_mut();
                let mut query = world.query::<(&mut UiSelected, &mut UiPreviousSelection)>();
                let (mut selected, mut previous) = query.get_mut(world, node).expect("toggle");
                apply_explicit_authored_selection(&mut selected, &mut previous, active, false)
                    .expect("selection transition")
            };
            app.world_mut().write_message(UiNodeActivated {
                source: ActionSource::System,
                node,
                trigger,
            });
        }
        app.update();
        let triggers = app
            .world_mut()
            .resource_mut::<Messages<UiNodeActivated>>()
            .drain()
            .map(|activation| activation.trigger)
            .collect::<Vec<_>>();
        assert_eq!(triggers, [UiTrigger::Off, UiTrigger::On]);
        let mut selected = UiSelected(true);
        let mut previous = UiPreviousSelection::from_current_selection(true);
        assert_eq!(
            apply_explicit_authored_selection(&mut selected, &mut previous, true, false),
            None
        );
        assert_eq!(
            apply_explicit_authored_selection(&mut selected, &mut previous, true, true),
            Some(UiTrigger::On)
        );
    }
}
