//! Authored hiding keeps an actor out of sight for a drawn time, as a guest
//! inside a gift shop, then shows it again where it went in.

use bevy::prelude::*;
use openzt2_zoo_state::behavior_task_execution_types::running_actions::{
    BehaviorInstructionId, RunBehaviorHide, RunningBehaviorInstruction,
};

use super::behavior_random_stream_state::BehaviorRandomStream;
use super::behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionState;
use openzt2_zoo_state::behavior_task_execution_types::PendingBehaviorTaskFailure;
use openzt2_zoo_state::guests::guest_simulation_types::GuestRng;
use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;

#[derive(Component)]
struct HiddenForBehavior {
    instruction: BehaviorInstructionId,
    until_tick: u64,
}

pub(super) fn register_hide_execution(application: &mut App) {
    use openzt2_game_assets::application_lifecycle::GamePhase;
    use openzt2_world_state::application_schedule::FixedGameSet;
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
    mut hidden: Query<(
        Entity,
        &HiddenForBehavior,
        Option<(&BehaviorTaskExecutionState, &RunningBehaviorInstruction)>,
        Option<&mut Visibility>,
    )>,
) {
    for (actor, hiding, task, visibility) in &mut hidden {
        if task.is_none_or(|(task, step)| hiding.instruction != step.id() || !step.is_current(task))
        {
            commands.entity(actor).remove::<HiddenForBehavior>();
            set_hiding_visibility(actor, visibility, Visibility::Inherited, &mut commands);
        }
    }
}

fn hide_actors_for_current_hide_actions(
    mut commands: Commands,
    clock: Res<ZooClock>,
    definitions: Res<WorldDefinitions>,
    definition_assets: Res<Assets<WorldDefinitionAsset>>,
    mut actors: Query<
        (
            Entity,
            (
                &BehaviorTaskExecutionState,
                &RunningBehaviorInstruction,
                &RunBehaviorHide,
            ),
            Option<&HiddenForBehavior>,
            Option<&mut GuestRng>,
            Option<&mut BehaviorRandomStream>,
            Option<&mut Visibility>,
        ),
        Without<PendingBehaviorTaskFailure>,
    >,
) {
    let Some(definitions) = definitions.get(&definition_assets) else {
        return;
    };
    for (
        actor,
        (task, instruction, running_action),
        hiding,
        guest_random,
        behavior_random,
        visibility,
    ) in &mut actors
    {
        if task.next_action_tick > clock.tick {
            continue;
        }
        if !instruction.is_current(task) {
            continue;
        }
        let &RunBehaviorHide { minimum, maximum } = running_action;
        if let Some(hiding) = hiding.filter(|hiding| hiding.instruction == instruction.id()) {
            if clock.tick >= hiding.until_tick {
                commands.entity(actor).remove::<HiddenForBehavior>();
                set_hiding_visibility(actor, visibility, Visibility::Inherited, &mut commands);
                instruction.complete(actor, clock.tick, &mut commands);
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
        commands.entity(actor).insert(HiddenForBehavior {
            instruction: instruction.id(),
            until_tick,
        });
        set_hiding_visibility(actor, visibility, Visibility::Hidden, &mut commands);
        instruction.resume_at(actor, until_tick, &mut commands);
    }
}

fn set_hiding_visibility(
    actor: Entity,
    visibility: Option<Mut<Visibility>>,
    value: Visibility,
    commands: &mut Commands,
) {
    if let Some(mut visibility) = visibility {
        visibility.set_if_neq(value);
    } else {
        commands.entity(actor).insert(value);
    }
}
