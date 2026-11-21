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
use openzt2_zoo_state::staff::staff_assignment_types::CleansFilters;
use openzt2_zoo_state::staff::staff_assignment_types::CleansRecycling;
use openzt2_zoo_state::staff::staff_assignment_types::EmptiesTrash;
use openzt2_zoo_state::staff::staff_assignment_types::SweepsTrash;
use openzt2_zoo_state::staff::staff_assignment_types::{
    StaffAssignmentArea, StaffAssignmentTarget,
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
    workers: Query<
        (
            Has<CleansFilters>,
            Has<CleansRecycling>,
            Has<EmptiesTrash>,
            Has<SweepsTrash>,
        ),
        With<Staff>,
    >,
    changed_workers: Query<
        (),
        Or<(
            Added<CleansFilters>,
            Added<CleansRecycling>,
            Added<EmptiesTrash>,
            Added<SweepsTrash>,
        )>,
    >,
    mut removed_filters: RemovedComponents<CleansFilters>,
    mut removed_recycling: RemovedComponents<CleansRecycling>,
    mut removed_trash: RemovedComponents<EmptiesTrash>,
    mut removed_sweeping: RemovedComponents<SweepsTrash>,
    mut nodes: Query<(&UiSelectedBinding, &mut UiSelected), StaffDutyField>,
) {
    let filters_removed = removed_filters.read().count() != 0;
    let recycling_removed = removed_recycling.read().count() != 0;
    let trash_removed = removed_trash.read().count() != 0;
    let sweeping_removed = removed_sweeping.read().count() != 0;
    let duties_changed = !changed_workers.is_empty()
        || filters_removed
        || recycling_removed
        || trash_removed
        || sweeping_removed;
    for (parent, visible, fields) in &panels {
        let Ok(selection) = selections.get(parent.parent()) else {
            continue;
        };
        if !visible.get()
            || (!selection.is_changed()
                && !visible.is_changed()
                && !duties_changed
                && !fields.as_ref().is_some_and(Ref::is_changed))
        {
            continue;
        }
        let Ok((filters, recycling, trash, sweeping)) = workers.get(selection.0) else {
            continue;
        };
        for entity in fields
            .as_deref()
            .into_iter()
            .flat_map(RelationshipTarget::iter)
        {
            let Ok((binding, mut selected)) = nodes.get_mut(entity) else {
                continue;
            };

            let current = match &binding.0 {
                UiBooleanPropertyBindingSource::WorkerCleansFilters => filters,
                UiBooleanPropertyBindingSource::WorkerCleansRecycling => recycling,
                UiBooleanPropertyBindingSource::WorkerEmptiesTrash => trash,
                UiBooleanPropertyBindingSource::WorkerSweepsTrash => sweeping,
                _ => continue,
            };
            if selected.0 != current {
                selected.0 = current;
            }
        }
    }
}

pub(crate) fn project_live_staff_status_to_visible_information_panels(
    calendar: Res<ZooCalendar>,
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
