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
use openzt2_zoo_state::habitat::habitat_types::HabitatMember;
use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;

use super::guest_simulation_types::{Guest, GuestPhase, Viewing};

/// Starts, holds and ends the native observation for the running view action.
/// The observation's area is the task's target, the path viewed from.
pub(super) fn watch_animals_for_current_view_actions(
    clock: Res<ZooClock>,
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
        ),
        (With<Guest>, Without<PendingBehaviorTaskFailure>),
    >,
    animals: Query<
        (Entity, &GlobalTransform, &Visibility, &WorldMember),
        (With<Animal>, With<HabitatMember>),
    >,
    mut commands: Commands,
) {
    let Some(range_m) = world_definitions
        .get(&world_assets)
        .and_then(|definitions| definitions.guest_viewing())
        .map(|viewing| viewing.ground_range_m)
    else {
        return;
    };
    for (guest, mut task, transform, world, phase, viewing) in &mut guests {
        if task.next_action_tick > clock.tick {
            continue;
        }
        let Some(BehaviorAction::ViewAnimals(action)) = assets
            .get(&task.document)
            .and_then(|document| find_current_behavior_task_action(document, &task))
        else {
            continue;
        };
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
        match viewing.filter(|viewing| viewing.area == task.target()) {
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
                });
            }
            Some(viewing) => {
                // The authored view meter starts near 200 and its emote sets
                // are not yet played, so the watch lasts that many ticks.
                let watched = clock.tick.saturating_sub(viewing.started_tick)
                    >= action.view_meter_start.max(1.0) as u64;
                if watched || !in_view(viewing.subject) {
                    commands.entity(guest).remove::<Viewing>();
                    advance_behavior_task_to_next_action(&mut task, clock.tick);
                }
            }
        }
    }
}
