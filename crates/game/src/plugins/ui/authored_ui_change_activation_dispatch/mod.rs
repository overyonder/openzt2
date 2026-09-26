use bevy::prelude::*;
use openzt2_game_data::ui_document::action::UiTrigger;

use crate::plugins::input::input_types::ActionSource;

use super::authored_ui_focus_state::UiFocusScope;
use super::authored_ui_node_projection_components::UiValue;
use super::authored_ui_selection_state::UiSelected;
use crate::plugins::ui::authored_ui_activation_contracts::UiNodeActivated;

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct UiPreviousFocus(pub(super) Option<Entity>);

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct UiPreviousVisibility(pub(super) bool);

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct UiPreviousSelection(bool);

impl UiPreviousSelection {
    pub(crate) fn from_current_selection(selected: bool) -> Self {
        Self(selected)
    }

    pub(crate) fn synchronize_with_non_authored_selection_change(&mut self, selected: bool) {
        self.0 = selected;
    }
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
