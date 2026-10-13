//! Authored hiding keeps an actor out of sight for a drawn time, as a guest
//! inside a gift shop, then shows it again where it went in.

use bevy::prelude::*;
use openzt2_game_data::behavior::action_record::BehaviorAction;

use super::behavior_random_stream_state::BehaviorRandomStream;
use super::behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation;
use openzt2_game_assets::behavior::behavior_asset_types::BehaviorDocumentAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::plugins::behavior_task_execution_types::{
    advance_behavior_task_to_next_action, find_current_behavior_task_action,
    BehaviorTaskExecutionState, BehaviorTaskReturnFrame, PendingBehaviorTaskFailure,
};
use crate::plugins::guests::guest_simulation_types::GuestRng;
use crate::plugins::simulation_time::simulation_clock_types::ZooClock;

#[derive(Component)]
struct HiddenForBehavior {
    execution_id: u64,
    origin: BehaviorTaskReturnFrame,
    depth: usize,
    until_tick: u64,
}

impl HiddenForBehavior {
    fn owns(&self, task: &BehaviorTaskExecutionState) -> bool {
        task.execution_id == self.execution_id
            && task.stack.len() == self.depth
            && task.is_at_or_nested_under(&self.origin, self.depth)
    }
}

pub(super) fn register_hide_execution(application: &mut App) {
    use crate::application_schedule::FixedGameSet;
    use openzt2_game_assets::application_lifecycle::GamePhase;
    application.add_systems(
        FixedUpdate,
        (show_actors_whose_hiding_ended, hide_actors_for_current_hide_actions)
            .chain()
            .before(super::behavior_task_phase_transition_execution::advance_behavior_tasks_through_return_failure_completion_and_final_outcome)
            .in_set(FixedGameSet::Think)
            .run_if(in_state(GamePhase::InGame)),
    );
}

/// A failed, replaced or finished task shows its actor again.
fn show_actors_whose_hiding_ended(
    mut commands: Commands,
    hidden: Query<(
        Entity,
        &HiddenForBehavior,
        Option<&BehaviorTaskExecutionState>,
    )>,
) {
    for (actor, hiding, task) in &hidden {
        if task.is_none_or(|task| !hiding.owns(task)) {
            commands
                .entity(actor)
                .remove::<HiddenForBehavior>()
                .insert(Visibility::Inherited);
        }
    }
}

fn hide_actors_for_current_hide_actions(
    mut commands: Commands,
    clock: Res<ZooClock>,
    documents: Res<Assets<BehaviorDocumentAsset>>,
    definitions: Res<WorldDefinitions>,
    definition_assets: Res<Assets<WorldDefinitionAsset>>,
    mut actors: Query<
        (
            Entity,
            &mut BehaviorTaskExecutionState,
            Option<&HiddenForBehavior>,
            Option<&mut GuestRng>,
            Option<&mut BehaviorRandomStream>,
        ),
        Without<PendingBehaviorTaskFailure>,
    >,
) {
    let Some(definitions) = definitions.get(&definition_assets) else {
        return;
    };
    for (actor, mut task, hiding, guest_random, behavior_random) in &mut actors {
        if task.next_action_tick > clock.tick {
            continue;
        }
        let Some(&BehaviorAction::Hide { minimum, maximum }) = documents
            .get(&task.document)
            .and_then(|document| find_current_behavior_task_action(document, &task))
        else {
            continue;
        };
        if let Some(hiding) = hiding.filter(|hiding| hiding.owns(&task)) {
            if clock.tick >= hiding.until_tick {
                commands
                    .entity(actor)
                    .remove::<HiddenForBehavior>()
                    .insert(Visibility::Inherited);
                advance_behavior_task_to_next_action(&mut task, clock.tick);
            }
            continue;
        }
        let duration = if minimum == maximum {
            minimum
        } else {
            let unit = guest_random
                .map(|mut random| random.0.unit_f32())
                .or_else(|| behavior_random.map(|mut random| random.0.unit_f32()));
            let Some(unit) = unit else {
                mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
                continue;
            };
            minimum + (maximum - minimum).mul_f64(f64::from(unit))
        };
        let ticks = duration
            .as_nanos()
            .saturating_mul(u128::from(definitions.timing().fixed_hz))
            .div_ceil(1_000_000_000);
        let until_tick = clock
            .tick
            .saturating_add(u64::try_from(ticks).unwrap_or(u64::MAX));
        commands.entity(actor).insert((
            HiddenForBehavior {
                execution_id: task.execution_id,
                origin: task.create_return_frame_after_action(task.action),
                depth: task.stack.len(),
                until_tick,
            },
            Visibility::Hidden,
        ));
        task.next_action_tick = until_tick;
    }
}
