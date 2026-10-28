#[derive(bevy::prelude::SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StaffBehaviorSettlement;

#[derive(bevy::prelude::SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StaffJobRequestCreation;
pub mod staff_assignment_types;
pub mod staff_employment_types;
pub mod staff_job_completion_rules;
pub mod staff_job_types;
pub mod staff_lifecycle_messages;
pub mod staff_presentation_variant_selection;
pub mod staff_tank_gate_types;
