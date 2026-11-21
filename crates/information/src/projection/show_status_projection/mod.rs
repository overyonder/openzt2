use bevy::{ecs::relationship::RelationshipTarget, prelude::*};
use openzt2_game_data::ui_document::node_property_binding::{
    UiBooleanPropertyBindingSource, UiIntegerPropertyBindingSource,
};
use openzt2_interface_state::{
    information::entity_selection_types::{InfoPanel, SelectedSubject},
    ui::authored_ui_node_projection_components::{UiValue, UiValueBinding, UiVisibleBinding},
};
use openzt2_zoo_state::shows::{
    running_show_types::{RunningShow, RunningShowFromRow, ShowPhase, StageRunningShow},
    show_schedule_types::{
        ScheduledShowPerformancePlan, ScheduledShowRow, ScheduledShowTrickPerformance,
        ShowTrickSlots,
    },
};

use super::panel_binding_families::{
    ShowField, ShowFields, ShowVisibilityField, ShowVisibilityFields,
};

pub(crate) fn project_selected_show_schedule_status_to_visible_information_panels(
    panels: Query<
        (
            &ChildOf,
            &InheritedVisibility,
            Option<&ShowFields>,
            Option<&ShowVisibilityFields>,
        ),
        With<InfoPanel>,
    >,
    selections: Query<&SelectedSubject>,
    rows: Query<(
        &ScheduledShowRow,
        Option<&ScheduledShowPerformancePlan>,
        Option<&ShowTrickSlots>,
    )>,
    stages: Query<&StageRunningShow>,
    running: Query<(&RunningShow, &RunningShowFromRow)>,
    performances: Query<&ScheduledShowTrickPerformance>,
    mut values: Query<(&UiValueBinding, &mut UiValue), ShowField>,
    mut visibility: Query<(&UiVisibleBinding, &mut Visibility), ShowVisibilityField>,
) {
    for (parent, visible, integer_fields, boolean_fields) in &panels {
        if !visible.get() {
            continue;
        }
        let Ok(selection) = selections.get(parent.parent()) else {
            continue;
        };
        let selected_row = rows.get(selection.0).ok();
        let stage = selected_row.map_or(selection.0, |(row, _, _)| row.stage);
        let active = stages
            .get(stage)
            .ok()
            .and_then(|stage| running.get(*stage.collection()).ok())
            .filter(|(_, row)| selected_row.is_none() || row.0 == selection.0);
        let plan = selected_row.or_else(|| active.and_then(|(_, row)| rows.get(row.0).ok()));
        let slots = plan.and_then(|(_, _, slots)| slots);

        for entity in integer_fields
            .into_iter()
            .flat_map(RelationshipTarget::iter)
        {
            let Ok((binding, mut value)) = values.get_mut(entity) else {
                continue;
            };
            let next = match &binding.0 {
                UiIntegerPropertyBindingSource::ShowScheduleCount => {
                    i64::try_from(slots.map_or(0, RelationshipTarget::len)).unwrap_or(i64::MAX)
                }
                UiIntegerPropertyBindingSource::ShowScheduleCapacity => plan
                    .and_then(|(_, plan, _)| plan)
                    .map_or(0, |plan| i64::from(plan.trick_performance_capacity)),
                UiIntegerPropertyBindingSource::ShowRemainingTicks => {
                    active.map_or(0, |(show, _)| i64::from(show.remaining_ticks))
                }
                UiIntegerPropertyBindingSource::ShowSlotDurationTicks { slot } => slots
                    .and_then(|slots| slots.collection().get(usize::from(*slot)))
                    .and_then(|&entity| performances.get(entity).ok())
                    .map_or(0, |performance| i64::from(performance.duration_ticks)),
                _ => continue,
            };
            value.set_if_neq(UiValue(next));
        }
        for entity in boolean_fields
            .into_iter()
            .flat_map(RelationshipTarget::iter)
        {
            let Ok((binding, mut visibility)) = visibility.get_mut(entity) else {
                continue;
            };
            let visible = match &binding.0 {
                UiBooleanPropertyBindingSource::ShowScheduled => {
                    slots.is_some_and(|slots| !slots.is_empty())
                }
                UiBooleanPropertyBindingSource::ShowRunning => {
                    active.is_some_and(|(show, _)| show.phase != ShowPhase::BetweenShows)
                }
                _ => continue,
            };
            visibility.set_if_neq(if visible {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            });
        }
    }
}
