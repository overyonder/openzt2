use bevy::prelude::*;
use openzt2_game_assets::behavior::behavior_asset_types::BehaviorDocumentAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_data::behavior::action_record::BehaviorAction;
use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_zoo_state::animal_health::types::{AnimalDied, Dead, DeathCause, Disease};
use openzt2_zoo_state::animal_lifecycle::types::Animal;
use openzt2_zoo_state::behavior_task_execution_types::{
    BehaviorTaskExecutionState, PendingBehaviorTaskFailure,
    advance_behavior_task_to_next_action, find_current_behavior_task_action,
};

use super::behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation;
use crate::animal_health::disease_mortality_execution::finalize_animal_death;

#[cfg(test)]
mod tests;

#[derive(Component)]
pub(super) struct AuthoredAnimalRemovalTick(u64);

pub(super) fn execute_animal_termination_actions(
    mut commands: Commands,
    clock: Res<ZooClock>,
    documents: Res<Assets<BehaviorDocumentAsset>>,
    definitions: Res<WorldDefinitions>,
    definition_assets: Res<Assets<WorldDefinitionAsset>>,
    mut actors: Query<
        (Entity, &mut BehaviorTaskExecutionState),
        (
            With<Animal>,
            Without<Dead>,
            Without<PendingBehaviorTaskFailure>,
        ),
    >,
    animals: Query<Option<&Disease>, (With<Animal>, Without<Dead>)>,
    mut deaths: MessageWriter<AnimalDied>,
) {
    let Some(definitions) = definitions.get(&definition_assets) else {
        return;
    };
    let mut killed = std::collections::HashSet::new();
    for (actor, mut task) in &mut actors {
        if task.next_action_tick > clock.tick || killed.contains(&actor) {
            continue;
        }
        let Some(BehaviorAction::Termination(policy)) = documents
            .get(&task.document)
            .and_then(|document| find_current_behavior_task_action(document, &task))
        else {
            continue;
        };
        let target = policy.kill_target.then(|| task.target()).flatten();
        if policy.kill_target && target.is_none_or(|entity| animals.get(entity).is_err()) {
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
            let Ok(disease) = animals.get(animal) else {
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
            advance_behavior_task_to_next_action(&mut task, clock.tick);
        }
    }
}

pub(super) fn hide_animals_after_authored_removal_period(
    mut commands: Commands,
    clock: Res<ZooClock>,
    animals: Query<(Entity, &AuthoredAnimalRemovalTick), With<Dead>>,
) {
    for (animal, removal) in &animals {
        if clock.tick >= removal.0 {
            // Keep the canonical identity for genealogy and death-event consumers.
            commands
                .entity(animal)
                .insert(Visibility::Hidden)
                .remove::<AuthoredAnimalRemovalTick>();
        }
    }
}
