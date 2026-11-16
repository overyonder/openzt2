//! A rampaging animal that completes `SmashBuilding` destroys its target,
//! which gives way to its ruin.

use bevy::prelude::*;
use openzt2_game_assets::application_lifecycle::GamePhase;
use openzt2_world_state::application_schedule::FixedGameSet;
use openzt2_zoo_state::{
    behavior_task_execution_types::{BehaviorTaskFailed, BehaviorTaskFinished},
    placement::placement_transaction_types::DestroyPlacedObject,
};

/// The building the animal's current task smashes.
#[derive(Component, Debug, Clone, Copy)]
pub(crate) struct SmashingBuilding {
    pub(crate) execution_id: u64,
    pub(crate) building: Entity,
}

/// `SmashBuilding` destroys its target; `SmashBuilding_Futile`, aimed at a
/// building too strong for the animal, doesn't.
// ponytail: told apart by name, as the fence tasks are; the destruction isn't
// in the task's data.
pub(crate) fn task_smashes_building(task_name: &str) -> bool {
    task_name == "SmashBuilding"
}

pub(super) fn register_building_smash_execution(application: &mut App) {
    application.add_systems(
        FixedUpdate,
        destroy_smashed_buildings
            .after(super::behavior_task_phase_transition_execution::advance_behavior_tasks_through_return_failure_completion_and_final_outcome)
            .in_set(FixedGameSet::Think)
            .run_if(in_state(GamePhase::InGame)),
    );
}

fn destroy_smashed_buildings(
    mut commands: Commands,
    mut finished: MessageReader<BehaviorTaskFinished>,
    mut failed: MessageReader<BehaviorTaskFailed>,
    animals: Query<&SmashingBuilding>,
    mut destroy: MessageWriter<DestroyPlacedObject>,
) {
    let outcomes = failed
        .read()
        .map(|outcome| (outcome.actor, outcome.execution_id, false))
        .chain(
            finished
                .read()
                .map(|outcome| (outcome.actor, outcome.execution_id, true)),
        )
        .collect::<Vec<_>>();
    for (actor, execution_id, completed) in outcomes {
        let Ok(smash) = animals.get(actor) else {
            continue;
        };
        if smash.execution_id != execution_id {
            continue;
        }
        if completed {
            destroy.write(DestroyPlacedObject {
                entity: smash.building,
            });
        }
        commands.entity(actor).remove::<SmashingBuilding>();
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn only_a_smash_that_can_succeed_destroys_its_building() {
        assert!(super::task_smashes_building("SmashBuilding"));
        assert!(!super::task_smashes_building("SmashBuilding_Futile"));
    }
}
