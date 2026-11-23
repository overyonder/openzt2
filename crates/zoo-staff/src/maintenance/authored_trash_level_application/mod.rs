use bevy::prelude::*;
use openzt2_zoo_state::behavior_task_execution_types::running_actions::WhenBehaviorAction;
use openzt2_zoo_state::behavior_task_execution_types::running_actions::{
    RunBehaviorFactModifications, RunningBehaviorInstruction,
};

use openzt2_zoo_state::maintenance::maintenance_types::{
    AdjustFacilityContainedWaste, FacilityContainedWaste,
};

pub(super) fn request_authored_trash_level_changes(
    activation: On<WhenBehaviorAction<RunBehaviorFactModifications>>,
    mut commands: Commands,
    clock: Single<&openzt2_world_state::simulation_time::simulation_clock_types::ZooClock>,
    price_effect: openzt2_zoo_state::economy::price_effect::BehaviorPriceEffectContext,
    containers: Query<&FacilityContainedWaste>,
    mut actors: Query<(
        Entity,
        (&openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionState,
            &RunningBehaviorInstruction, &RunBehaviorFactModifications),
        Option<&mut openzt2_zoo_state::animal_behavior::behavior_random_stream_state::BehaviorRandomStream>,
        Option<&mut openzt2_zoo_state::guests::guest_simulation_types::GuestRng>,
    ), (
        Without<openzt2_zoo_state::behavior_task_execution_types::PendingBehaviorTaskFailure>,
        Without<openzt2_zoo_state::animal_lifecycle::types::Animal>,
        Without<openzt2_zoo_state::staff::staff_employment_types::Staff>,
    )>,
    mut changes: MessageWriter<AdjustFacilityContainedWaste>,
) {
    use openzt2_game_data::behavior::action::modification::{
        BehaviorFact, BehaviorFactModificationOperation,
    };
    use openzt2_zoo_state::animal_behavior::behavior_entity_role_resolution::resolve_behavior_entity_role_to_live_entity;
    for (actor, (task, instruction, running_action), mut random, mut guest_random) in
        std::iter::once(actors.get_mut(activation.entity)).flatten()
    {
        if task.next_action_tick > clock.tick {
            continue;
        }
        if !instruction.is_current(task) {
            continue;
        }
        let modifications = &running_action.0;
        if modifications.is_empty()
            || modifications
                .iter()
                .any(|modification| modification.modified_fact != BehaviorFact::TrashLevel)
        {
            continue;
        }
        if !modifications.iter().all(|modification| {
            resolve_behavior_entity_role_to_live_entity(
                modification.affected_entity_role,
                actor,
                task.target,
            )
            .is_some_and(|target| containers.contains(target))
                && (modification.modification_operation == BehaviorFactModificationOperation::Clear
                    || price_effect
                        .sample(modification.modification_value, task.target, |_| 0)
                        .is_some())
        }) {
            openzt2_zoo_state::behavior_task_execution_types::running_actions::mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            continue;
        }
        for modification in modifications {
            let target = resolve_behavior_entity_role_to_live_entity(
                modification.affected_entity_role,
                actor,
                task.target,
            )
            .expect("validated trash-level target");
            let delta_q16 = match modification.modification_operation {
                BehaviorFactModificationOperation::Clear => 0,
                BehaviorFactModificationOperation::Add => price_effect
                    .sample(modification.modification_value, task.target, |upper| {
                        if let Some(random) = random.as_deref_mut() {
                            random.range_u32(upper).unwrap_or(0)
                        } else if let Some(random) = guest_random.as_deref_mut() {
                            random.0.range_u32(upper).unwrap_or(0)
                        } else {
                            0
                        }
                    })
                    .expect("validated trash-level scalar context"),
            };
            changes.write(AdjustFacilityContainedWaste {
                facility: target,
                operation: modification.modification_operation,
                delta_q16,
            });
        }
        instruction.complete(actor, clock.tick, &mut commands);
    }
}

/// Applies authored trash-level writes to their containers, held between
/// empty and full.
pub(super) fn apply_authored_trash_level_changes(
    mut changes: MessageReader<AdjustFacilityContainedWaste>,
    mut containers: Query<&mut FacilityContainedWaste>,
) {
    for change in changes.read() {
        let Ok(container) = containers.get_mut(change.facility) else {
            continue;
        };
        let delta_units = (f64::from(change.delta_q16) / 65_536.0).round() as i64;
        let units = match change.operation {
            openzt2_game_data::behavior::action::modification::BehaviorFactModificationOperation::Clear => 0,
            openzt2_game_data::behavior::action::modification::BehaviorFactModificationOperation::Add =>
                (i64::from(container.contained_waste_units) + delta_units)
                    .clamp(0, i64::from(container.contained_waste_capacity_units)),
        };
        container
            .map_unchanged(|waste| &mut waste.contained_waste_units)
            .set_if_neq(units as u16);
    }
}
