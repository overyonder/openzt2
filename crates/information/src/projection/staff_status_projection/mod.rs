use super::panel_binding_families::*;
use bevy::ecs::relationship::RelationshipTarget;
use bevy::prelude::*;
use openzt2_game_data::ui_document::node_property_binding::{
    UiBooleanPropertyBindingSource, UiIntegerPropertyBindingSource,
};

use openzt2_interface_state::ui::authored_ui_node_projection_components::UiValue;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiValueBinding;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiVisibleBinding;
use openzt2_interface_state::ui::authored_ui_selection_state::UiSelected;
use openzt2_ui::authored_ui_selection_binding::UiSelectedBinding;
use openzt2_world_state::simulation_time::simulation_clock_types::ZooCalendar;
use openzt2_zoo_state::staff::staff_assignment_types::{
    StaffAssignmentArea, StaffAssignmentTarget, WorkerDutyAssignments,
};
use openzt2_zoo_state::staff::staff_employment_types::Employment;
use openzt2_zoo_state::staff::staff_employment_types::Staff;
use openzt2_zoo_state::staff::staff_job_types::CurrentJob;
use openzt2_zoo_state::staff::staff_job_types::JobProgress;
use openzt2_zoo_state::staff::staff_job_types::StaffJob;

use openzt2_interface_state::information::entity_selection_types::{InfoPanel, SelectedSubject};

/// Updates the selected maintenance worker’s duty checkboxes.
pub(crate) fn project_maintenance_worker_duties_to_information_panel_controls(
    panels: Query<
        (
            &ChildOf,
            Ref<InheritedVisibility>,
            Option<Ref<StaffDutyFields>>,
        ),
        With<InfoPanel>,
    >,
    selections: Query<Ref<SelectedSubject>>,
    workers: Query<Ref<WorkerDutyAssignments>, With<Staff>>,
    mut nodes: Query<(&UiSelectedBinding, &mut UiSelected), StaffDutyField>,
) {
    for (parent, visible, fields) in &panels {
        let Ok(selection) = selections.get(parent.parent()) else {
            continue;
        };
        let Ok(duties) = workers.get(selection.0) else {
            continue;
        };
        if !visible.get()
            || (!selection.is_changed()
                && !visible.is_changed()
                && !duties.is_changed()
                && !fields.as_ref().is_some_and(Ref::is_changed))
        {
            continue;
        }
        for entity in fields
            .as_deref()
            .into_iter()
            .flat_map(RelationshipTarget::iter)
        {
            let Ok((binding, mut selected)) = nodes.get_mut(entity) else {
                continue;
            };

            if let UiBooleanPropertyBindingSource::WorkerDutyAssigned(task) = binding.0 {
                selected.set_if_neq(UiSelected(duties.is_assigned(task)));
            }
        }
    }
}

pub(crate) fn project_live_staff_status_to_visible_information_panels(
    calendar: Single<&ZooCalendar>,
    panels: Query<
        (
            &ChildOf,
            Ref<InheritedVisibility>,
            Option<Ref<StaffFields>>,
            Option<Ref<StaffVisibilityFields>>,
        ),
        With<InfoPanel>,
    >,
    selections: Query<Ref<SelectedSubject>>,
    workers: Query<
        (
            Ref<Employment>,
            Has<StaffAssignmentArea>,
            Has<StaffAssignmentTarget>,
            Option<Ref<CurrentJob>>,
        ),
        With<Staff>,
    >,
    jobs: Query<(&StaffJob, Option<&JobProgress>)>,
    mut bools: Query<(&UiVisibleBinding, &mut Visibility), StaffVisibilityField>,
    mut values: Query<(&UiValueBinding, &mut UiValue), StaffField>,
) {
    let calendar = calendar.into_inner();
    let current_month_ordinal = u32::from(calendar.year)
        .saturating_mul(12)
        .saturating_add(u32::from(calendar.month.saturating_sub(1)));
    for (parent, visible, fields, visible_fields) in &panels {
        let Ok(selection) = selections.get(parent.parent()) else {
            continue;
        };
        if !visible.get() {
            continue;
        }
        let Ok((employment, assigned_area, assigned_target, current_job)) =
            workers.get(selection.0)
        else {
            continue;
        };
        let job = current_job
            .as_ref()
            .and_then(|current| jobs.get(current.job()).ok());
        for entity in fields
            .as_deref()
            .into_iter()
            .flat_map(RelationshipTarget::iter)
        {
            let Ok((binding, mut value)) = values.get_mut(entity) else {
                continue;
            };
            let next = match &binding.0 {
                UiIntegerPropertyBindingSource::StaffWageCentsPerDay => employment.wage.0,
                UiIntegerPropertyBindingSource::StaffMonthsEmployed => {
                    i64::from(current_month_ordinal.saturating_sub(employment.hired_month_ordinal))
                }
                UiIntegerPropertyBindingSource::StaffJobUrgency => {
                    job.map_or(0, |(job, _)| i64::from(job.urgency))
                }
                UiIntegerPropertyBindingSource::StaffJobRemainingTicks => 0,
                _ => continue,
            };
            value.set_if_neq(UiValue(next));
        }
        for entity in visible_fields
            .as_deref()
            .into_iter()
            .flat_map(RelationshipTarget::iter)
        {
            let Ok((binding, mut visibility)) = bools.get_mut(entity) else {
                continue;
            };
            let shown = match &binding.0 {
                UiBooleanPropertyBindingSource::StaffAssigned => assigned_area || assigned_target,
                UiBooleanPropertyBindingSource::StaffWorking => current_job.is_some(),
                _ => continue,
            };
            visibility.set_if_neq(if shown {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            });
        }
    }
}
