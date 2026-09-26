use bevy::prelude::*;
use openzt2_game_data::ui_document::node_property_binding::{
    UiBooleanPropertyBindingSource, UiIntegerPropertyBindingSource,
};

use crate::plugins::{
    simulation_time::simulation_clock_types::ZooCalendar,
    staff::{
        staff_assignment_types::{
            CleansFilters, CleansRecycling, EmptiesTrash, StaffAssignment, SweepsTrash,
        },
        staff_employment_types::{Employment, Staff},
        staff_job_types::{CurrentJob, JobProgress, StaffJob},
    },
    ui::{
        authored_ui_change_activation_dispatch::UiPreviousSelection,
        authored_ui_node_projection_components::UiDocumentOwner,
        authored_ui_node_projection_components::UiValue,
        authored_ui_node_projection_components::UiValueBinding,
        authored_ui_node_projection_components::UiVisibleBinding,
        authored_ui_selection_binding::UiSelectedBinding, authored_ui_selection_state::UiSelected,
    },
};

use super::super::entity_selection_types::InfoPanel;

/// Updates the selected maintenance worker’s duty checkboxes.
pub(in crate::plugins::information) fn project_maintenance_worker_duties_to_information_panel_controls(
    panels: Query<(Entity, Ref<InfoPanel>, &InheritedVisibility)>,
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
    mut nodes: Query<(
        &UiDocumentOwner,
        &UiSelectedBinding,
        &mut UiSelected,
        &mut UiPreviousSelection,
    )>,
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
    for (panel_entity, panel, visible) in &panels {
        if !visible.get() || (!panel.is_changed() && !duties_changed) {
            continue;
        }
        let Ok((filters, recycling, trash, sweeping)) = workers.get(panel.subject) else {
            continue;
        };
        for (owner, binding, mut selected, mut previous) in &mut nodes {
            if owner.0 != panel_entity {
                continue;
            }
            let current = match &binding.0 {
                UiBooleanPropertyBindingSource::WorkerCleansFilters => filters,
                UiBooleanPropertyBindingSource::WorkerCleansRecycling => recycling,
                UiBooleanPropertyBindingSource::WorkerEmptiesTrash => trash,
                UiBooleanPropertyBindingSource::WorkerSweepsTrash => sweeping,
                _ => continue,
            };
            if selected.0 != current {
                selected.0 = current;
                previous.synchronize_with_non_authored_selection_change(current);
            }
        }
    }
}

pub(in crate::plugins::information) fn project_live_staff_status_to_visible_information_panels(
    calendar: Res<ZooCalendar>,
    panels: Query<(Entity, Ref<InfoPanel>, &InheritedVisibility)>,
    workers: Query<
        (
            Ref<Employment>,
            Ref<StaffAssignment>,
            Option<Ref<CurrentJob>>,
        ),
        With<Staff>,
    >,
    jobs: Query<(&StaffJob, Option<&JobProgress>)>,
    mut bools: Query<(&UiDocumentOwner, &UiVisibleBinding, &mut Visibility)>,
    mut values: Query<(&UiDocumentOwner, &UiValueBinding, &mut UiValue)>,
) {
    let current_month_ordinal = u32::from(calendar.year)
        .saturating_mul(12)
        .saturating_add(u32::from(calendar.month.saturating_sub(1)));
    for (panel_entity, panel, visible) in &panels {
        if !visible.get() {
            continue;
        }
        let Ok((employment, assignment, current_job)) = workers.get(panel.subject) else {
            continue;
        };
        let job = current_job
            .as_ref()
            .and_then(|current| jobs.get(current.job).ok());
        for (owner, binding, mut value) in &mut values {
            if owner.0 != panel_entity {
                continue;
            }
            value.0 = match &binding.0 {
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
        }
        for (owner, binding, mut visibility) in &mut bools {
            if owner.0 != panel_entity {
                continue;
            }
            let shown = match &binding.0 {
                UiBooleanPropertyBindingSource::StaffAssigned => {
                    assignment.area.is_some() || assignment.target.is_some()
                }
                UiBooleanPropertyBindingSource::StaffWorking => current_job.is_some(),
                _ => continue,
            };
            *visibility = if shown {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
        }
    }
}
