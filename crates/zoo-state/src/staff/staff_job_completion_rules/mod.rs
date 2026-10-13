use openzt2_game_data::{
    behavior::action::{
        entity_role::BehaviorEntityRole,
        modification::{BehaviorFact, BehaviorFactModification},
    },
    world_definitions::staff_management::StaffJobKind,
};

pub fn behavior_fact_modification_is_owned_by_terminal_maintenance_system(
    modification: &BehaviorFactModification,
    current_staff_job_kind: Option<StaffJobKind>,
) -> bool {
    modification.affected_entity_role == BehaviorEntityRole::Target
        && matches!(
            (current_staff_job_kind, modification.modified_fact),
            (Some(StaffJobKind::EmptyBin), BehaviorFact::TrashLevel)
                | (
                    Some(StaffJobKind::MaintainTank),
                    BehaviorFact::FilterDirtiness
                )
                | (
                    Some(StaffJobKind::MaintainTank),
                    BehaviorFact::WaterDirtiness
                )
        )
}
