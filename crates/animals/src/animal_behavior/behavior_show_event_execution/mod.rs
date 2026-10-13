use bevy::prelude::*;
use openzt2_game_data::behavior::action_record::BehaviorAction;

use openzt2_game_assets::behavior::behavior_asset_types::BehaviorDocumentAsset;
use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::animal_behavior::behavior_set_start_request_types::PendingBehaviorSet;
use openzt2_zoo_state::animal_lifecycle::types::Animal;
use openzt2_zoo_state::behavior_task_execution_types::advance_behavior_task_to_next_action;
use openzt2_zoo_state::behavior_task_execution_types::find_current_behavior_task_action;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionState;
use openzt2_zoo_state::behavior_task_execution_types::PendingBehaviorTaskFailure;
use openzt2_zoo_state::shows::show_event_types::AuthoredAnimalShowEvent;

#[allow(clippy::type_complexity)]
pub(super) fn register_current_authored_animal_show_events(
    mut commands: Commands,
    documents: Res<Assets<BehaviorDocumentAsset>>,
    clock: Res<ZooClock>,
    mut performers: Query<
        (Entity, &WorldMember, &mut BehaviorTaskExecutionState),
        (
            With<Animal>,
            Without<PendingBehaviorTaskFailure>,
            Without<PendingBehaviorSet>,
        ),
    >,
) {
    for (performer, world, mut task) in &mut performers {
        if task.next_action_tick > clock.tick {
            continue;
        }
        let Some(BehaviorAction::AnimalShowEvent(policy)) = documents
            .get(&task.document)
            .and_then(|document| find_current_behavior_task_action(document, &task))
        else {
            continue;
        };
        commands.spawn((
            *world,
            AuthoredAnimalShowEvent {
                performer,
                execution_id: task.execution_id,
                policy: policy.clone(),
            },
        ));
        advance_behavior_task_to_next_action(&mut task, clock.tick);
    }
}
