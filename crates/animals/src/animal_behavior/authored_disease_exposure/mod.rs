//! The shipped disease manager (`ZTDiseaseMgr`). Every `diseaseCheckInterval`
//! it gives one qualifying animal a disease's `token`; the disease's own tasks
//! and scripts then make the animal sick, spread it and heal it.

use bevy::platform::collections::HashSet;
use bevy::prelude::*;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::{
    WorldDefinitions, WorldDefinitionsView,
};
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_data::world_definitions::animal_health::{
    AuthoredDisease, DiseaseAnimalFilter, DiseaseZooTest,
};
use openzt2_game_data::AssetId;
use openzt2_world_state::simulation_time::deterministic_random_stream::{
    DeterministicRng, RngDomain, ZooSeed,
};
use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_world_state::world_spawn::world_membership_types::DefinitionId;
use openzt2_zoo_state::animal_behavior::behavior_token_types::BehaviorSelfToken;
use openzt2_zoo_state::animal_health::types::{Dead, Disease};
use openzt2_zoo_state::animal_lifecycle::types::Animal;
use openzt2_zoo_state::progression::fame_types::Fame;

use super::autonomous_task_selection::candidate_facts::AnimalCandidateFacts;
use super::behavior_subject_type_resolution::AnimalBehaviorSubjectTypes;

#[allow(clippy::too_many_arguments)]
pub(super) fn expose_animals_to_authored_diseases(
    mut commands: Commands,
    clock: Res<ZooClock>,
    seed: Res<ZooSeed>,
    definition_assets: Res<Assets<WorldDefinitionAsset>>,
    definitions: Res<WorldDefinitions>,
    fame: Res<Fame>,
    subject_types: AnimalBehaviorSubjectTypes,
    animals: Query<(Entity, AnimalCandidateFacts, Has<Disease>), (With<Animal>, Without<Dead>)>,
    objects: Query<(&'static DefinitionId, &'static GlobalTransform), Without<Animal>>,
    tokens: Query<(&BehaviorSelfToken, &ChildOf)>,
) {
    let Some(view) = definitions.get(&definition_assets) else {
        return;
    };
    let Some(policy) = view.disease_exposure() else {
        return;
    };
    let interval = ((f64::from(policy.check_interval_seconds) * f64::from(view.timing().fixed_hz))
        .ceil() as u64)
        .max(1);
    // Checks fall on multiples of the interval, each with its own stream, so
    // a loaded game replays them without saving any schedule.
    if clock.tick == 0 || clock.tick % interval != 0 {
        return;
    }
    let random = &mut DeterministicRng::from_entity(
        *seed,
        PersistentId(clock.tick / interval),
        RngDomain::Disease,
    );
    let held = tokens
        .iter()
        .map(|(token, parent)| (parent.parent(), token.name))
        .collect::<HashSet<_>>();
    let world = DiseaseExposureWorld {
        view,
        subject_types: &subject_types,
        animals: &animals,
        objects: &objects,
        fame: fame.half_stars,
    };
    for disease in &policy.diseases {
        // ExtinctFlu's `chanceMultiplier` formula is unknown, so it isn't run
        // with the multiplier left out.
        if disease.force_only
            || !disease.search_for_targets
            || disease.chance_multiplier.is_some()
            || disease.chance_percent <= 0.0
            || !disease.zoo_tests.iter().all(|test| world.passes(test))
        {
            continue;
        }
        let candidates = animals
            .iter()
            .filter(|(animal, facts, diseased)| {
                !diseased
                    && !held.contains(&(*animal, disease.token))
                    && world.qualifies(*animal, facts, disease)
            })
            .map(|(animal, ..)| animal)
            .collect::<Vec<_>>();
        debug!(
            disease = ?disease.name,
            candidates = candidates.len(),
            "disease exposure check"
        );
        if candidates.is_empty() || random.unit_f32() * 100.0 >= disease.chance_percent {
            continue;
        }
        let Some(index) = random.range_u32(candidates.len() as u32) else {
            continue;
        };
        commands.spawn((
            BehaviorSelfToken {
                name: disease.token,
                available_at: clock.tick,
                expires_at: u64::MAX,
            },
            ChildOf(candidates[index as usize]),
        ));
    }
}

struct DiseaseExposureWorld<'a, 'w, 's> {
    view: WorldDefinitionsView<'a>,
    subject_types: &'a AnimalBehaviorSubjectTypes<'w, 's>,
    animals: &'a Query<
        'w,
        's,
        (Entity, AnimalCandidateFacts, Has<Disease>),
        (With<Animal>, Without<Dead>),
    >,
    objects: &'a Query<'w, 's, (&'static DefinitionId, &'static GlobalTransform), Without<Animal>>,
    fame: u8,
}

impl DiseaseExposureWorld<'_, '_, '_> {
    fn animal_is(
        &self,
        animal: Entity,
        facts: &<AnimalCandidateFacts as bevy::ecs::query::QueryData>::Item<'_, '_>,
        required: AssetId,
    ) -> bool {
        facts.matches_type(required, self.subject_types.borrow_for(animal))
    }

    fn object_is(&self, definition: AssetId, required: AssetId) -> bool {
        definition == required
            || self
                .view
                .find_object(definition)
                .is_some_and(|object| object.behavior_type_identifiers.contains(&required))
    }

    // ponytail: every test must pass; `singleQualifier` and `different`
    // are kept unused until the manager's use of them is confirmed.
    fn passes(&self, test: &DiseaseZooTest) -> bool {
        match *test {
            DiseaseZooTest::EntityCount {
                entity_type,
                attribute,
                quantity,
                comparison,
                ..
            } => {
                let animals = self
                    .animals
                    .iter()
                    .filter(|(animal, facts, _)| {
                        self.animal_is(*animal, facts, entity_type)
                            && attribute.is_none_or(|(attribute, value)| {
                                self.animal_is(*animal, facts, attribute) == value
                            })
                    })
                    .count();
                let objects = if attribute.is_some() {
                    0
                } else {
                    self.objects
                        .iter()
                        .filter(|(definition, _)| self.object_is(definition.0, entity_type))
                        .count()
                };
                comparison.holds((animals + objects) as u32, quantity)
            }
            DiseaseZooTest::Fame {
                half_stars,
                comparison,
            } => comparison.holds(u32::from(self.fame), half_stars),
        }
    }

    fn qualifies(
        &self,
        animal: Entity,
        facts: &<AnimalCandidateFacts as bevy::ecs::query::QueryData>::Item<'_, '_>,
        disease: &AuthoredDisease,
    ) -> bool {
        disease
            .target_types
            .iter()
            .any(|&required| self.animal_is(animal, facts, required))
            && disease.animal_filters.iter().all(|filter| match *filter {
                DiseaseAnimalFilter::Attribute { attribute, value } => {
                    self.animal_is(animal, facts, attribute) == value
                }
                DiseaseAnimalFilter::Proximity {
                    entity_type,
                    distance_m,
                    closer,
                } => {
                    let position = facts.transform.translation();
                    let near = self.objects.iter().any(|(definition, transform)| {
                        self.object_is(definition.0, entity_type)
                            && transform.translation().distance_squared(position)
                                <= distance_m * distance_m
                    });
                    near == closer
                }
            })
    }
}

/// A cured animal drops the disease's pending tokens, its authored
/// `clearTokens`, so it doesn't fall ill or spread the disease again at once.
pub(super) fn clear_authored_disease_tokens_when_cured(
    cured: On<Remove, Disease>,
    mut commands: Commands,
    diseases: Query<&Disease>,
    tokens: Query<(Entity, &BehaviorSelfToken, &ChildOf)>,
    definition_assets: Res<Assets<WorldDefinitionAsset>>,
    definitions: Res<WorldDefinitions>,
) {
    let Some(disease) = diseases
        .get(cured.entity)
        .ok()
        .zip(definitions.get(&definition_assets))
        .and_then(|(disease, view)| {
            view.disease_exposure()?
                .diseases
                .iter()
                .find(|authored| authored.name == disease.definition)
        })
    else {
        return;
    };
    for (token, held, parent) in &tokens {
        if parent.parent() == cured.entity && disease.clear_tokens.contains(&held.name) {
            commands.entity(token).despawn();
        }
    }
}
