//! Authored animal viewing: a docked guest watches the nearest animal in view.
use openzt2_zoo_state::behavior_task_execution_types::running_actions::BehaviorViewingProgress;
use openzt2_zoo_state::behavior_task_execution_types::running_actions::{
    RunBehaviorViewAnimals, RunningBehaviorInstruction,
};

use bevy::prelude::*;
use openzt2_zoo_state::behavior_task_execution_types::running_actions::behavior_action_actors;

use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_zoo_state::behavior_task_execution_types::running_actions::mark_behavior_task_for_failure_and_stop_navigation;
use openzt2_zoo_state::animal_lifecycle::types::Animal;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionState;
use openzt2_zoo_state::behavior_task_execution_types::PendingBehaviorTaskFailure;
use openzt2_zoo_state::habitat::habitat_types::HabitatMember;
use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;

use super::guest_simulation_types::{Guest, GuestArchetype, GuestPhase, GuestRng, Viewing};

/// Starts, holds and ends the native observation for the running view action.
/// The observation's area is the task's target, the path viewed from.
pub(super) fn watch_animals_for_current_view_actions(
    activation: In<Option<Entity>>,
    clock: Single<&ZooClock>,
    time: Res<Time<Fixed>>,
    world_assets: Res<Assets<WorldDefinitionAsset>>,
    world_definitions: Res<WorldDefinitions>,
    mut guests: Query<
        (
            Entity,
            (
                &BehaviorTaskExecutionState,
                &RunningBehaviorInstruction,
                &RunBehaviorViewAnimals,
            ),
            &GlobalTransform,
            &WorldMember,
            &GuestPhase,
            Option<&Viewing>,
            &GuestArchetype,
            &mut GuestRng,
            Option<&mut BehaviorViewingProgress>,
        ),
        (With<Guest>, Without<PendingBehaviorTaskFailure>),
    >,
    animals: Query<
        (Entity, &GlobalTransform, &Visibility, &WorldMember),
        (With<Animal>, With<HabitatMember>),
    >,
    mut commands: Commands,
) {
    let Some(definitions) = world_definitions.get(&world_assets) else {
        return;
    };
    let Some(range_m) = definitions
        .guest_viewing()
        .map(|viewing| viewing.ground_range_m)
    else {
        return;
    };
    for (
        guest,
        (task, instruction, running_action),
        transform,
        world,
        phase,
        viewing,
        archetype,
        mut random,
        meter,
    ) in behavior_action_actors(&mut guests, activation.0)
    {
        if activation.0.is_none() && !instruction.can_update_after_activation(clock.tick) {
            continue;
        }
        if task.next_action_tick > clock.tick {
            continue;
        }
        if !instruction.is_current(task) {
            continue;
        }
        let action = &running_action.0;
        let Some(definition) = definitions.find_guest(archetype.0) else {
            continue;
        };
        let mut meter = meter.filter(|meter| meter.instruction == instruction.id());
        let origin = transform.translation().xz();
        let in_view = |animal: Entity| {
            animals
                .get(animal)
                .is_ok_and(|(_, animal, visibility, animal_world)| {
                    animal_world.root == world.root
                        && *visibility != Visibility::Hidden
                        && animal.translation().xz().distance_squared(origin) <= range_m * range_m
                })
        };
        let viewing = viewing.filter(|viewing| {
            viewing.area == task.target() && viewing.task_execution_id == Some(task.execution_id)
        });
        match viewing {
            None => {
                let nearest = animals
                    .iter()
                    .filter(|(animal, ..)| in_view(*animal))
                    .min_by(|(_, left, ..), (_, right, ..)| {
                        left.translation()
                            .xz()
                            .distance_squared(origin)
                            .total_cmp(&right.translation().xz().distance_squared(origin))
                    });
                let (Some((subject, ..)), GuestPhase::Visiting) = (nearest, phase) else {
                    mark_behavior_task_for_failure_and_stop_navigation(guest, &mut commands);
                    continue;
                };
                commands.entity(guest).insert(Viewing {
                    subject,
                    area: task.target(),
                    started_tick: clock.tick,
                    task_execution_id: Some(task.execution_id),
                });
            }
            Some(viewing) => {
                let watched = meter.as_deref_mut().is_some_and(|meter| {
                    let completed = meter.remaining < 0.0;
                    meter.remaining -= time.delta_secs() * definition.view_meter_decay_per_second;
                    completed
                });
                if watched || !in_view(viewing.subject) {
                    commands
                        .entity(guest)
                        .remove::<(Viewing, BehaviorViewingProgress)>();
                    instruction.complete(guest, clock.tick, &mut commands);
                    continue;
                }
            }
        }
        if meter.is_none() {
            let [minimum, maximum] = action.view_meter_start_range;
            let start = if minimum > 0.0 && maximum > minimum {
                #[allow(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    reason = "positive authored view-meter ranges are sampled in whole units"
                )]
                let width = (maximum - minimum) as u32;
                let Some(upper) = width.checked_add(1) else {
                    mark_behavior_task_for_failure_and_stop_navigation(guest, &mut commands);
                    continue;
                };
                #[allow(
                    clippy::cast_precision_loss,
                    reason = "the authored meter uses float values for its whole-unit random samples"
                )]
                let offset = random.0.range_u32(upper).unwrap_or_default() as f32;
                minimum + offset
            } else {
                action.view_meter_start
            };
            commands.entity(guest).insert(BehaviorViewingProgress {
                instruction: instruction.id(),
                remaining: start,
                maximum: start,
            });
        }
    }
}
