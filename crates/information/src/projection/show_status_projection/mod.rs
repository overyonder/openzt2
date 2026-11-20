use bevy::{ecs::relationship::RelationshipTarget, prelude::*};
use openzt2_game_data::ui_document::node_property_binding::{
    UiBooleanPropertyBindingSource, UiIntegerPropertyBindingSource,
};

use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentOwner;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiValue;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiValueBinding;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiVisibleBinding;
use openzt2_zoo_state::shows::show_schedule_types::{
    ScheduledShowPerformancePlan, ScheduledShowTrickPerformance, ShowTrickSlots,
};

use openzt2_interface_state::information::entity_selection_types::{InfoPanel, SelectedSubject};

pub(crate) fn project_selected_show_schedule_status_to_visible_information_panels(
    information_panels: Query<(Entity, &ChildOf, Ref<InheritedVisibility>), With<InfoPanel>>,
    selections: Query<Ref<SelectedSubject>>,
    show_subjects: Query<(
        Option<&ScheduledShowPerformancePlan>,
        Option<&ShowTrickSlots>,
    )>,
    performances: Query<&ScheduledShowTrickPerformance>,
    mut authored_integer_values: Query<(&UiDocumentOwner, &UiValueBinding, &mut UiValue)>,
    mut authored_boolean_visibility: Query<(&UiDocumentOwner, &UiVisibleBinding, &mut Visibility)>,
) {
    for (panel_entity, parent, inherited_visibility) in &information_panels {
        let Ok(selection) = selections.get(parent.parent()) else {
            continue;
        };
        if !inherited_visibility.get() {
            continue;
        }
        let Ok((scheduled_performance_plan, slots)) = show_subjects.get(selection.0) else {
            continue;
        };

        for (document_owner, property_binding, mut projected_value) in &mut authored_integer_values
        {
            if document_owner.0 != panel_entity {
                continue;
            }
            projected_value.0 = match &property_binding.0 {
                UiIntegerPropertyBindingSource::ShowScheduleCount => {
                    i64::try_from(slots.map_or(0, RelationshipTarget::len)).unwrap_or(i64::MAX)
                }
                UiIntegerPropertyBindingSource::ShowScheduleCapacity => scheduled_performance_plan
                    .map_or(0, |plan| i64::from(plan.trick_performance_capacity)),
                UiIntegerPropertyBindingSource::ShowRemainingTicks => 0,
                UiIntegerPropertyBindingSource::ShowSlotDurationTicks { slot } => slots
                    .and_then(|slots| slots.collection().get(usize::from(*slot)))
                    .and_then(|&entity| performances.get(entity).ok())
                    .map_or(0, |performance| i64::from(performance.duration_ticks)),
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
                UiBooleanPropertyBindingSource::ShowScheduled => {
                    slots.is_some_and(|slots| !slots.is_empty())
                }
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
