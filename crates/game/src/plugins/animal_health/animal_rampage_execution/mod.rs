use bevy::prelude::*;

use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::animal_behavior::behavior_random_stream_state::BehaviorRandomStream;
use crate::plugins::animal_behavior::behavior_set_start_request_types::StartBehaviorSet;
use crate::plugins::animal_lifecycle::types::Animal;
use crate::plugins::animal_lifecycle::types::SpeciesHandle;
use crate::plugins::animal_welfare::types::AnimalWelfare;

use super::types::{Dead, Disease, Rampaging, Tranquilized};

fn authored_rampage_rule_starts_for_animal_state_and_random_sample(
    welfare_permille: u16,
    disease_permille: Option<u16>,
    welfare_below: u16,
    disease_above: u16,
    random_sample: u32,
    probability: u32,
) -> bool {
    let authored_trigger = welfare_permille < welfare_below
        || disease_permille.is_some_and(|severity| severity > disease_above);
    authored_trigger && random_sample < probability
}

pub(super) fn evaluate_animal_rampage_start_and_end_conditions(
    mut commands: Commands,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    mut animals: Query<
        (
            Entity,
            &SpeciesHandle,
            &AnimalWelfare,
            Option<&mut BehaviorRandomStream>,
            Option<&Disease>,
            Option<&Rampaging>,
        ),
        (
            With<Animal>,
            Without<Dead>,
            Or<(Changed<AnimalWelfare>, Changed<Disease>, Changed<Rampaging>)>,
        ),
    >,
    mut behavior_set_start_requests: MessageWriter<StartBehaviorSet>,
) {
    let Some(world_definitions) = active_world_definitions.get(&world_definition_assets) else {
        return;
    };
    for (animal, species, welfare, random, disease, rampaging) in &mut animals {
        let mut matching_rampage_rules = world_definitions
            .rampage_rules()
            .filter(|rule| rule.species == species.species);
        if let Some(current_rampage) = rampaging {
            let rampage_should_continue = matching_rampage_rules.any(|rule| {
                current_rampage.elapsed_ticks < u64::from(rule.minimum_ticks)
                    || welfare.0 < rule.welfare_below
                    || disease.is_some_and(|disease| disease.severity_permille > rule.disease_above)
            });
            if !rampage_should_continue {
                commands.entity(animal).remove::<Rampaging>();
            }
        } else if let Some(behavior) = random.and_then(|mut random| {
            matching_rampage_rules.find_map(|rule| {
                authored_rampage_rule_starts_for_animal_state_and_random_sample(
                    welfare.0,
                    disease.map(|disease| disease.severity_permille),
                    rule.welfare_below,
                    rule.disease_above,
                    random.next_u32(),
                    rule.probability,
                )
                .then_some(rule.behavior)
            })
        }) {
            commands
                .entity(animal)
                .insert(Rampaging { elapsed_ticks: 0 });
            behavior_set_start_requests.write(StartBehaviorSet {
                actor: animal,
                program: behavior,
                target: None,
            });
        }
    }
}

pub(super) fn advance_elapsed_ticks_for_active_untranquilized_animal_rampages(
    mut rampaging_animals: Query<
        &mut Rampaging,
        (With<Animal>, Without<Dead>, Without<Tranquilized>),
    >,
) {
    for mut rampage in &mut rampaging_animals {
        rampage.elapsed_ticks = rampage.elapsed_ticks.saturating_add(1);
    }
}
