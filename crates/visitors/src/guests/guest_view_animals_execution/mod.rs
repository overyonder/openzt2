//! Authored animal viewing: a docked guest watches the nearest animal in view.

use bevy::prelude::*;
use openzt2_game_data::behavior::action_record::BehaviorAction;

use openzt2_game_assets::behavior::behavior_asset_types::BehaviorDocumentAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_zoo_state::animal_behavior::behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation;
use openzt2_zoo_state::animal_lifecycle::types::Animal;
use openzt2_zoo_state::behavior_task_execution_types::advance_behavior_task_to_next_action;
use openzt2_zoo_state::behavior_task_execution_types::find_current_behavior_task_action;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionState;
use openzt2_zoo_state::behavior_task_execution_types::PendingBehaviorTaskFailure;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskReturnFrame;
use openzt2_zoo_state::habitat::habitat_types::HabitatMember;
use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;

use super::guest_simulation_types::{Guest, GuestArchetype, GuestPhase, GuestRng, Viewing};

#[derive(Component)]
pub(super) struct ActiveGuestAnimalViewingMeter {
    execution_id: u64,
    origin: BehaviorTaskReturnFrame,
    stack_depth: usize,
    remaining: f32,
    maximum: f32,
}

impl ActiveGuestAnimalViewingMeter {
    pub(super) fn owns(&self, task: &BehaviorTaskExecutionState) -> bool {
        self.execution_id == task.execution_id
            && task.is_at_or_nested_under(&self.origin, self.stack_depth)
    }

    pub(super) fn apply_view_score(&mut self, score: f32) {
        self.remaining = (self.remaining + score).min(self.maximum);
    }
}

/// Starts, holds and ends the native observation for the running view action.
/// The observation's area is the task's target, the path viewed from.
pub(super) fn watch_animals_for_current_view_actions(
    clock: Res<ZooClock>,
    time: Res<Time<Fixed>>,
    assets: Res<Assets<BehaviorDocumentAsset>>,
    world_assets: Res<Assets<WorldDefinitionAsset>>,
    world_definitions: Res<WorldDefinitions>,
    mut guests: Query<
        (
            Entity,
            &mut BehaviorTaskExecutionState,
            &GlobalTransform,
            &WorldMember,
            &GuestPhase,
            Option<&Viewing>,
            &GuestArchetype,
            &mut GuestRng,
            Option<&mut ActiveGuestAnimalViewingMeter>,
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
    for (guest, mut task, transform, world, phase, viewing, archetype, mut random, meter) in
        &mut guests
    {
        if task.next_action_tick > clock.tick {
            continue;
        }
        let Some(BehaviorAction::ViewAnimals(action)) = assets
            .get(&task.document)
            .and_then(|document| find_current_behavior_task_action(document, &task))
        else {
            continue;
        };
        let Some(definition) = definitions.find_guest(archetype.0) else {
            continue;
        };
        let mut meter = meter.filter(|meter| meter.owns(&task));
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
                        .remove::<(Viewing, ActiveGuestAnimalViewingMeter)>();
                    advance_behavior_task_to_next_action(&mut task, clock.tick);
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
            commands
                .entity(guest)
                .insert(ActiveGuestAnimalViewingMeter {
                    execution_id: task.execution_id,
                    origin: task.create_return_frame_after_action(task.action),
                    stack_depth: task.stack.len(),
                    remaining: start,
                    maximum: start,
                });
        }
    }
}
