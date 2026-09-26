use bevy::prelude::*;
use openzt2_game_data::ui_document::node_property_binding::{
    UiBooleanPropertyBindingSource, UiIntegerPropertyBindingSource,
};

use crate::plugins::{
    shows::show_schedule_types::ScheduledShowPerformancePlan,
    ui::{
        authored_ui_node_projection_components::UiDocumentOwner,
        authored_ui_node_projection_components::UiValue,
        authored_ui_node_projection_components::UiValueBinding,
        authored_ui_node_projection_components::UiVisibleBinding,
    },
};

use super::super::entity_selection_types::InfoPanel;

pub(in crate::plugins::information) fn project_selected_show_schedule_status_to_visible_information_panels(
    information_panels: Query<(Entity, &InfoPanel, &InheritedVisibility)>,
    show_subjects: Query<Option<&ScheduledShowPerformancePlan>>,
    mut authored_integer_values: Query<(&UiDocumentOwner, &UiValueBinding, &mut UiValue)>,
    mut authored_boolean_visibility: Query<(&UiDocumentOwner, &UiVisibleBinding, &mut Visibility)>,
) {
    for (panel_entity, information_panel, inherited_visibility) in &information_panels {
        if !inherited_visibility.get() {
            continue;
        }
        let Ok(scheduled_performance_plan) = show_subjects.get(information_panel.subject) else {
            continue;
        };

        for (document_owner, property_binding, mut projected_value) in &mut authored_integer_values
        {
            if document_owner.0 != panel_entity {
                continue;
            }
            projected_value.0 = match &property_binding.0 {
                UiIntegerPropertyBindingSource::ShowScheduleCount => scheduled_performance_plan
                    .map_or(0, |plan| plan.trick_performances.len() as i64),
                UiIntegerPropertyBindingSource::ShowScheduleCapacity => scheduled_performance_plan
                    .map_or(0, |plan| i64::from(plan.trick_performance_capacity)),
                UiIntegerPropertyBindingSource::ShowRemainingTicks => 0,
                UiIntegerPropertyBindingSource::ShowSlotDurationTicks { slot } => {
                    scheduled_performance_plan
                        .and_then(|plan| plan.trick_performances.get(usize::from(*slot)))
                        .map_or(0, |performance| i64::from(performance.duration_ticks))
                }
                _ => continue,
            };
        }

        for (document_owner, property_binding, mut projected_visibility) in
            &mut authored_boolean_visibility
        {
            if document_owner.0 != panel_entity {
                continue;
            }
            let should_be_visible = match &property_binding.0 {
                UiBooleanPropertyBindingSource::ShowScheduled => scheduled_performance_plan
                    .is_some_and(|plan| !plan.trick_performances.is_empty()),
                UiBooleanPropertyBindingSource::ShowRunning => false,
                _ => continue,
            };
            *projected_visibility = if should_be_visible {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
        }
    }
}
