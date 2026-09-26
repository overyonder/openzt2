//! Authored controller requests evaluated against canonical world quantities.
mod staff_request_state_types;
mod staff_request_threshold_comparison;

mod threshold_evaluation;

use bevy::prelude::*;

use crate::{
    application_lifecycle::GamePhase,
    application_schedule::FixedGameSet,
    plugins::{feeding::ContainerQuantityHydration, staff::staff_job_request_creation},
};

pub(crate) struct StaffRequestRuntimePlugin;

impl Plugin for StaffRequestRuntimePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            FixedUpdate,
            (
                threshold_evaluation::bind_staff_request_rows_on_definition_change,
                threshold_evaluation::reconcile_staff_request_rows_on_definition_revision,
                threshold_evaluation::evaluate_staff_request_rows,
            )
                .chain()
                .after(ContainerQuantityHydration)
                .before(
                    staff_job_request_creation::create_or_raise_priority_of_requested_staff_jobs,
                )
                .in_set(FixedGameSet::Think)
                .run_if(in_state(GamePhase::InGame)),
        );
    }
}
