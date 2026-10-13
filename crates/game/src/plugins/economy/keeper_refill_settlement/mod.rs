use bevy::{ecs::system::SystemParam, prelude::*};
use openzt2_game_data::{
    behavior::{
        action::{
            entity_role::BehaviorEntityRole,
            modification::{BehaviorFact, BehaviorFactModificationOperation},
        },
        action_record::BehaviorAction,
        scalar::BehaviorScalarQ16,
    },
    world_definitions::staff_management::StaffJobKind,
    AssetId,
};

use crate::plugins::{
    behavior_task_execution_types::{
        advance_behavior_task_to_next_action, find_current_behavior_task_action,
        BehaviorTaskExecutionPhase, BehaviorTaskExecutionState,
    },
    feeding::container_quantity::{
        apply_authored_container_quantity_write, DrinkContainer, FoodContainer,
    },
    simulation_time::simulation_clock_types::ZooClock,
    staff::{
        staff_employment_types::Staff,
        staff_job_types::{CurrentJob, JobClaim, StaffJob, StaffJobBehaviorTask},
        staff_lifecycle_messages::CancelStaffJobRequest,
    },
    world_spawn::world_membership_types::{DefinitionId, WorldMember},
};
use openzt2_game_assets::behavior::behavior_asset_types::BehaviorDocumentAsset;

use super::behavior_transaction_state::KeeperRefillPayment;

#[derive(SystemParam)]
pub(super) struct KeeperRefillSettlement<'w, 's> {
    documents: Option<Res<'w, Assets<BehaviorDocumentAsset>>>,
    clock: Option<Res<'w, ZooClock>>,
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
        task: &mut BehaviorTaskExecutionState,
        payment: &KeeperRefillPayment,
    ) -> Option<()> {
        let current = self.staff.get(actor).ok()?;
        let (job, claim, behavior) = self.jobs.get(payment.job).ok()?;
        if current.job != payment.job
            || claim.staff != actor
            || behavior.execution_id != Some(task.execution_id)
            || job.kind != StaffJobKind::Feed
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
        let document = self.documents.as_ref()?.get(&task.document)?;
        let declaration = document.behavior_task_at_index(task.declaration)?;
        if declaration.id != task.program || declaration.id != behavior.task {
            return None;
        }
        let Some(BehaviorAction::Economy {
            transaction,
            general: false,
            tracker: None,
            ..
        }) = find_current_behavior_task_action(document, task)
        else {
            return None;
        };
        if *transaction != AssetId::from_key("refill")
            || task.phase != BehaviorTaskExecutionPhase::Execution
            || !task.stack.is_empty()
        {
            return None;
        }
        let tick = self.clock.as_ref()?.tick;
        let mut continuation = task.clone();
        advance_behavior_task_to_next_action(&mut continuation, tick);
        if continuation.phase == BehaviorTaskExecutionPhase::Execution
            && find_current_behavior_task_action(document, &continuation).is_none()
        {
            if declaration.execution.has_unsupported_suffix() {
                return None;
            }
            continuation.phase = BehaviorTaskExecutionPhase::Completion;
            continuation.action = 0;
        }
        let BehaviorAction::FactModifications(modifications) =
            find_current_behavior_task_action(document, &continuation)?
        else {
            return None;
        };
        let [modification] = modifications.as_slice() else {
            return None;
        };
        let BehaviorScalarQ16::FixedQ16(delta) = modification.modification_value else {
            return None;
        };
        if modification.affected_entity_role != BehaviorEntityRole::Target
            || modification.modified_fact != BehaviorFact::FoodLevel
            || modification.modification_operation != BehaviorFactModificationOperation::Add
            || delta <= 0
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
        *amount = apply_authored_container_quantity_write(*amount, capacity, delta);
        advance_behavior_task_to_next_action(&mut continuation, tick);
        *task = continuation;
        Some(())
    }
}
