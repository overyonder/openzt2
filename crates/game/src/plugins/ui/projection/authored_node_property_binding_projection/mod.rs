use bevy::prelude::*;
use openzt2_game_data::ui_document::node_property_binding::{
    UiBooleanPropertyBindingSource, UiImagePropertyBindingSource, UiIntegerPropertyBindingSource,
    UiNodePropertyBinding,
};

use crate::plugins::simulation_time::simulation_control_ui_presentation::UiSimulationPausedVisibility;
use crate::plugins::ui::authored_ui_change_activation_dispatch::UiPreviousSelection;
use crate::plugins::ui::authored_ui_image_content_binding::UiImageBinding;
use crate::plugins::ui::authored_ui_integer_range_components::{UiMaximum, UiMinimum};
use crate::plugins::ui::authored_ui_interaction_enabled_binding::UiEnabledBinding;
use crate::plugins::ui::authored_ui_node_projection_components::{
    UiValue, UiValueBinding, UiVisibleBinding,
};
use crate::plugins::ui::authored_ui_presentation_slot_bindings::{
    UiShellPresentationBinding, UiTranquilizerHudBinding,
};
use crate::plugins::ui::authored_ui_selection_binding::UiSelectedBinding;
use crate::plugins::ui::authored_ui_selection_state::UiSelected;
use crate::plugins::ui::authored_ui_text_content_binding::UiTextBinding;

pub(super) fn authored_constant_visibility_and_interaction_enabled_state(
    bindings: &[UiNodePropertyBinding],
) -> (Option<bool>, Option<bool>) {
    let mut visible = None;
    let mut enabled = None;
    for binding in bindings {
        match binding.clone() {
            UiNodePropertyBinding::Visibility(UiBooleanPropertyBindingSource::Constant(value)) => {
                visible = Some(value)
            }
            UiNodePropertyBinding::InteractionEnabled(
                UiBooleanPropertyBindingSource::Constant(value),
            ) => enabled = Some(value),
            _ => {}
        }
    }
    (visible, enabled)
}

pub(super) fn insert_authored_node_property_bindings(
    commands: &mut Commands,
    entity: Entity,
    bindings: &[UiNodePropertyBinding],
) {
    for binding in bindings {
        match binding.clone() {
            UiNodePropertyBinding::Visibility(source) => {
                commands.entity(entity).insert(UiVisibleBinding(source));
            }
            UiNodePropertyBinding::InteractionEnabled(source) => {
                commands.entity(entity).insert(UiEnabledBinding(source));
            }
            UiNodePropertyBinding::TextContent(source) => {
                commands.entity(entity).insert(UiTextBinding(source));
            }
            UiNodePropertyBinding::ImageContent(source) => {
                let dynamic = !matches!(source, UiImagePropertyBindingSource::Constant { .. });
                let mut entity = commands.entity(entity);
                entity.insert(UiImageBinding(source));
                if dynamic {
                    entity.insert(ImageNode::default());
                }
            }
            UiNodePropertyBinding::IntegerValue(source) => {
                let initial = match source {
                    UiIntegerPropertyBindingSource::Constant(value) => value,
                    _ => 0,
                };
                commands
                    .entity(entity)
                    .insert((UiValueBinding(source), UiValue(initial)));
            }
            UiNodePropertyBinding::MinimumIntegerValue(value) => {
                commands.entity(entity).insert(UiMinimum(value));
            }
            UiNodePropertyBinding::MaximumIntegerValue(value) => {
                commands.entity(entity).insert(UiMaximum(value));
            }
            UiNodePropertyBinding::SelectionState(source) => {
                let selected = matches!(source, UiBooleanPropertyBindingSource::Constant(true));
                commands.entity(entity).insert((
                    UiSelectedBinding(source),
                    UiSelected(selected),
                    UiPreviousSelection::from_current_selection(selected),
                ));
            }
            UiNodePropertyBinding::TranquilizerHeadsUpDisplaySlot(binding) => {
                commands
                    .entity(entity)
                    .insert(UiTranquilizerHudBinding(binding));
            }
            UiNodePropertyBinding::ShellPresentationSlot(binding) => {
                commands
                    .entity(entity)
                    .insert(UiShellPresentationBinding(binding));
            }
            UiNodePropertyBinding::SimulationPausedVisibility => {
                commands.entity(entity).insert(UiSimulationPausedVisibility);
            }
        }
    }
}
