use bevy::{ecs::system::SystemParam, prelude::*};
use openzt2_game_data::{world_definitions::staff_management::StaffJobKind, AssetId};
use openzt2_world_state::world_spawn::world_membership_types::{DefinitionId, WorldMember};
use openzt2_zoo_state::behavior_task_execution_types::{
    running_actions::BehaviorRefillContinuation, BehaviorTaskExecutionState,
};
use openzt2_zoo_state::feeding::container_quantity::apply_authored_container_quantity_write;
use openzt2_zoo_state::feeding::container_quantity::DrinkContainer;
use openzt2_zoo_state::feeding::container_quantity::FoodContainer;
use openzt2_zoo_state::staff::staff_employment_types::Staff;
use openzt2_zoo_state::staff::staff_job_types::CurrentJob;
use openzt2_zoo_state::staff::staff_job_types::JobClaim;
use openzt2_zoo_state::staff::staff_job_types::StaffJob;
use openzt2_zoo_state::staff::staff_job_types::StaffJobBehaviorTask;
use openzt2_zoo_state::staff::staff_lifecycle_messages::CancelStaffJobRequest;

#[derive(Component)]
pub(super) struct StagedBehaviorRefill {
    pub(super) job: Entity,
    pub(super) definition: AssetId,
    pub(super) continuation: BehaviorRefillContinuation,
}

#[derive(SystemParam)]
pub(super) struct KeeperRefillSettlement<'w, 's> {
    cancellations: Option<Res<'w, Messages<CancelStaffJobRequest>>>,
    staff: Query<'w, 's, &'static CurrentJob, With<Staff>>,
    jobs: Query<
        'w,
        's,
        (
            &'static StaffJob,
            &'static JobClaim,
            &'static StaffJobBehaviorTask,
        ),
    >,
    containers: Query<
        'w,
        's,
        (
            &'static DefinitionId,
            Option<&'static mut FoodContainer>,
            Option<&'static mut DrinkContainer>,
        ),
        With<WorldMember>,
    >,
}

impl KeeperRefillSettlement<'_, '_> {
    pub(super) fn commit(
        &mut self,
        actor: Entity,
        task: &BehaviorTaskExecutionState,
        payment: &StagedBehaviorRefill,
    ) -> Option<()> {
        let current = self.staff.get(actor).ok()?;
        let (job, claim, behavior) = self.jobs.get(payment.job).ok()?;
        if current.job() != payment.job
            || claim.staff != actor
            || behavior.execution_id != Some(task.execution_id)
            || behavior.task != task.program
            || job.kind != Some(StaffJobKind::Feed)
            || job.token != Some(AssetId::from_key("t_fillfoodcontainer"))
            || task.target != Some(job.target)
            || self.cancellations.as_ref().is_some_and(|messages| {
                messages
                    .get_cursor()
                    .read(messages)
                    .any(|request| request.job == payment.job)
            })
        {
            return None;
        }
        let (definition, food, drink) = self.containers.get_mut(job.target).ok()?;
        if definition.0 != payment.definition {
            return None;
        }
        let (amount, capacity) = match (food, drink) {
            (Some(food), None) => {
                let food = food.into_inner();
                (&mut food.amount_q16, food.capacity_q16)
            }
            (None, Some(drink)) => {
                let drink = drink.into_inner();
                (&mut drink.amount_q16, drink.capacity_q16)
            }
            _ => return None,
        };
        if capacity <= 0 || *amount < 0 || *amount >= capacity {
            return None;
        }
        *amount = apply_authored_container_quantity_write(
            *amount,
            capacity,
            payment.continuation.amount_q16,
        );
        Some(())
    }
}
