use openzt2_zoo_state::behavior_task_execution_types::running_actions::{RunBehaviorTermination, RunningBehaviorInstruction};
use bevy::prelude::*;
#[cfg(test)]
use openzt2_game_assets::behavior::behavior_asset_types::BehaviorDocumentAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
#[cfg(test)]
use openzt2_game_data::behavior::action_record::BehaviorAction;
use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_world_state::world_spawn::world_membership_types::{DefinitionId, WorldMember};
use openzt2_zoo_state::animal_health::types::{AnimalDied, Dead, DeathCause, Disease};
use openzt2_zoo_state::animal_lifecycle::types::Animal;
use openzt2_zoo_state::behavior_task_execution_types::{
    BehaviorTaskExecutionState, PendingBehaviorTaskFailure,

};

use super::behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation;
use crate::animal_health::disease_mortality_execution::finalize_animal_death;

#[cfg(test)]
mod tests;

#[derive(Component)]
pub(super) struct AuthoredAnimalRemovalTick(pub(super) u64);

pub(super) fn execute_animal_termination_actions(
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
                &RunBehaviorTermination,
            ),
            Option<&WorldMember>,
        ),
        (
            With<Animal>,
            Without<Dead>,
            Without<PendingBehaviorTaskFailure>,
        ),
    >,
    animals: Query<(Option<&Disease>, Option<&WorldMember>), (With<Animal>, Without<Dead>)>,
    objects: Query<&WorldMember, (With<DefinitionId>, Without<Animal>)>,
    mut deaths: MessageWriter<AnimalDied>,
) {
    let Some(definitions) = definitions.get(&definition_assets) else {
        return;
    };
    let mut killed = std::collections::HashSet::new();
    for (actor, (task, instruction, running_action), membership) in &mut actors {
        if task.next_action_tick > clock.tick || killed.contains(&actor) {
            continue;
        }
        if !instruction.is_current(task) {
            continue;
        }
        let policy = &running_action.0;
        let target = policy.kill_target.then(|| task.target()).flatten();
        let target_is_animal = target.is_some_and(|entity| {
            animals.get(entity).is_ok_and(|(_, world)| {
                world.map(|world| world.root) == membership.map(|world| world.root)
            })
        });
        let target_is_object = target.is_some_and(|entity| {
            objects
                .get(entity)
                .is_ok_and(|world| Some(world.root) == membership.map(|world| world.root))
        });
        if policy.kill_target && !target_is_animal && !target_is_object {
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            continue;
        }
        let ticks = policy
            .fade_out_period
            .as_nanos()
            .saturating_mul(u128::from(definitions.timing().fixed_hz))
            .div_ceil(1_000_000_000);
        let until = clock
            .tick
            .saturating_add(u64::try_from(ticks).unwrap_or(u64::MAX));
        for animal in policy
            .kill_subject
            .then_some(actor)
            .into_iter()
            .chain(target)
        {
            if !killed.insert(animal) {
                continue;
            }
            let Ok((disease, _)) = animals.get(animal) else {
                super::behavior_animation_key_execution::fade_and_remove_authored_object(
                    &mut commands,
                    animal,
                    clock.tick,
                    definitions.timing().fixed_hz,
                    policy.fade_out_period,
                );
                continue;
            };
            let cause = disease.map_or(DeathCause::Authored(task.program), |disease| {
                DeathCause::Disease(disease.definition)
            });
            finalize_animal_death(&mut commands, &mut deaths, animal, cause, clock.tick);
            commands
                .entity(animal)
                .insert(AuthoredAnimalRemovalTick(until));
        }
        if !policy.kill_subject {
            instruction.complete_with_target(actor, None, clock.tick, &mut commands);
        }
    }
}

pub(super) fn hide_animals_after_authored_removal_period(
    mut commands: Commands,
    clock: Res<ZooClock>,
    mut animals: Query<(Entity, &AuthoredAnimalRemovalTick, Option<&mut Visibility>), With<Dead>>,
) {
    for (animal, removal, visibility) in &mut animals {
        if clock.tick >= removal.0 {
            // Keep the canonical identity for genealogy and death-event consumers.
            commands
                .entity(animal)
                .remove::<AuthoredAnimalRemovalTick>();
            if let Some(mut visibility) = visibility {
                visibility.set_if_neq(Visibility::Hidden);
            } else {
                commands.entity(animal).insert(Visibility::Hidden);
            }
        }
    }
}
