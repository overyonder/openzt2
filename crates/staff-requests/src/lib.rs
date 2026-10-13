//! Authored controller requests evaluated against canonical world quantities.
mod staff_request_state_types;
mod staff_request_threshold_comparison;

mod threshold_evaluation;

use bevy::prelude::*;

use openzt2_game_assets::application_lifecycle::GamePhase;
use openzt2_world_state::application_schedule::FixedGameSet;
use openzt2_zoo_state::feeding::ContainerQuantityHydration;
use openzt2_zoo_state::staff::StaffJobRequestCreation;

pub struct StaffRequestRuntimePlugin;

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
                .before(StaffJobRequestCreation)
                .in_set(FixedGameSet::Think)
                .run_if(in_state(GamePhase::InGame)),
        );
    }
}
