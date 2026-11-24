use bevy::prelude::*;
use openzt2_game_data::ui_document::action::UiTrigger;

use openzt2_interface_state::input::input_types::ActionSource;

use super::authored_ui_node_projection_components::UiNodeId;
use super::authored_ui_node_projection_components::{UiSliderValueChanged, UiValue};
use openzt2_interface_state::ui::authored_ui_activation_contracts::UiNodeActivated;

pub(super) fn dispatch_authored_value_change_activations(
    values: Query<
        (Entity, Ref<UiValue>),
        (Changed<UiValue>, Without<crate::slider::UiSliderPolicy>),
    >,
    mut slider_edits: MessageReader<UiSliderValueChanged>,
    mut activated: MessageWriter<UiNodeActivated>,
) {
    for (node, value) in &values {
        if !value.is_added() {
            activated.write(UiNodeActivated {
                source: ActionSource::System,
                node,
                trigger: UiTrigger::Change,
            });
        }
    }
    let _ = activated.write_batch(slider_edits.read().map(|edit| UiNodeActivated {
        source: ActionSource::System,
        node: edit.slider,
        trigger: UiTrigger::Change,
    }));
}
#[derive(Component)]
pub(super) struct UiInitialDeselection;

pub(super) fn activate_initial_deselection(
    event: On<Add<UiInitialDeselection>>,
    mut activations: MessageWriter<UiNodeActivated>,
) {
    activations.write(UiNodeActivated {
        source: ActionSource::System,
        node: event.entity,
        trigger: UiTrigger::Off,
    });
}

pub(super) fn activate_initial_visibility(
    event: On<Add<UiNodeId>>,
    visibility: Query<&Visibility>,
    mut activations: MessageWriter<UiNodeActivated>,
) {
    if visibility
        .get(event.entity)
        .is_ok_and(|visibility| *visibility != Visibility::Hidden)
    {
        activations.write(UiNodeActivated {
            source: ActionSource::System,
            node: event.entity,
            trigger: UiTrigger::Show,
        });
    }
}

pub(super) fn apply_visibility_transition(
    node: Entity,
    visibility: &mut Mut<Visibility>,
    next: Visibility,
    authored: bool,
    activations: &mut MessageWriter<UiNodeActivated>,
) {
    if let Some(event) = set_authored_visibility(node, visibility, next, authored) {
        activations.write(event);
    }
}

pub(super) fn set_authored_visibility(
    node: Entity,
    visibility: &mut Mut<Visibility>,
    next: Visibility,
    authored: bool,
) -> Option<UiNodeActivated> {
    let was_visible = **visibility != Visibility::Hidden;
    (visibility.set_if_neq(next) && authored && was_visible != (next != Visibility::Hidden))
        .then_some(UiNodeActivated {
            source: ActionSource::System,
            node,
            trigger: if was_visible {
                UiTrigger::Hide
            } else {
                UiTrigger::Show
            },
        })
}
