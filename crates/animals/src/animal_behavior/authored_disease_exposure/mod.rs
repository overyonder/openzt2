use bevy::{platform::collections::{HashMap, HashSet}, prelude::*};
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
use openzt2_world_state::world_spawn::world_membership_types::{DefinitionId, WorldMember, WorldRoot};
use openzt2_zoo_state::animal_behavior::disease_exposure_state::{
    ActiveZooDisease, DiseaseExposureDeadline, DiseaseResearchFunding,
};
use openzt2_zoo_state::animal_behavior::behavior_token_types::BehaviorSelfToken;
use openzt2_zoo_state::animal_health::types::{Dead, Disease};
use openzt2_zoo_state::animal_lifecycle::types::Animal;
use openzt2_zoo_state::progression::fame_types::Fame;

use super::autonomous_task_selection::candidate_facts::{
    AnimalCandidateFacts, AnimalCandidateFactsItem,
};
use super::behavior_subject_type_resolution::AnimalBehaviorSubjectTypes;

fn disease_check_interval(definitions: WorldDefinitionsView<'_>) -> Option<u64> {
    let policy = definitions.disease_exposure()?;
    Some(
        ((f64::from(policy.check_interval_seconds) * f64::from(definitions.timing().fixed_hz))
            .ceil() as u64)
            .max(1),
    )
}

pub(super) fn disease_exposure_is_due(
    clock: Res<ZooClock>,
    worlds: Query<Option<&DiseaseExposureDeadline>, With<WorldRoot>>,
) -> bool {
    worlds
        .single()
        .is_ok_and(|deadline| deadline.is_none_or(|deadline| deadline.0 <= clock.tick))
}

fn disease_probability(chance_percent: f32, funding: f32) -> u32 {
    let adjusted_percent = if funding > 0.666 {
        chance_percent * 0.1
    } else if funding < 0.333 {
        chance_percent * 2.0
    } else {
        chance_percent
    };
    let chance = adjusted_percent / 100.0;
    if chance < 0.000_01 {
        0
    } else {
        (chance * 10_000.0).round_ties_even() as u32
    }
}

pub(super) type DiseaseAnimals<'w, 's> =
    Query<'w, 's, (Entity, AnimalCandidateFacts, Has<Disease>), (With<Animal>, Without<Dead>)>;

pub(super) fn expose_animals_to_authored_diseases(
    mut commands: Commands,
    clock: Res<ZooClock>,
    seed: Res<ZooSeed>,
    definition_assets: Res<Assets<WorldDefinitionAsset>>,
    definitions: Res<WorldDefinitions>,
    subject_types: AnimalBehaviorSubjectTypes,
    animals: DiseaseAnimals,
    objects: Query<(&DefinitionId, &GlobalTransform, &WorldMember), Without<Animal>>,
    mut worlds: Query<
        (
            Entity,
            Option<&mut DiseaseExposureDeadline>,
            Option<&DiseaseResearchFunding>,
            Option<&ActiveZooDisease>,
            &Fame,
        ),
        With<WorldRoot>,
    >,
    infected: Query<&WorldMember, (With<Animal>, With<Disease>, Without<Dead>)>,
    tokens: Query<&BehaviorSelfToken>,
    token_owners: Query<&Children>,
) {
    let Some(view) = definitions.get(&definition_assets) else {
        return;
    };
    let (Some(policy), Some(interval)) = (view.disease_exposure(), disease_check_interval(view))
    else {
        return;
    };
    let Ok((root, deadline, funding, active, fame)) = worlds.single_mut() else {
        return;
    };
    let update_interval = (f64::from(policy.update_interval_seconds)
        * f64::from(view.timing().fixed_hz))
    .ceil() as u64;
    let mut deadline = match deadline {
        Some(deadline) => deadline,
        None => {
            let first_check = clock
                .tick
                .checked_div(interval)
                .unwrap_or_default()
                .saturating_add(1)
                .saturating_mul(interval);
            commands
                .entity(root)
                .insert(DiseaseExposureDeadline(first_check));
            return;
        }
    };
    let has_disease = infected.iter().any(|member| member.root == root);
    if let Some(active) = active {
        let pending = policy
            .diseases
            .iter()
            .find(|disease| disease.name == active.definition)
            .is_some_and(|disease| {
                animals.iter().any(|(animal, facts, _)| {
                    facts.world.root == root
                        && tokens
                            .iter_many(token_owners.get(animal).ok().into_iter().flatten().copied())
                            .flatten()
                            .any(|token| {
                                token.name == disease.token
                                    || disease.clear_tokens.contains(&token.name)
                            })
                })
            });
        if clock.tick < active.cure_due_tick && (has_disease || pending) {
            deadline.0 = clock
                .tick
                .saturating_add(update_interval.max(1))
                .min(active.cure_due_tick);
        } else {
            commands.entity(root).remove::<ActiveZooDisease>();
            deadline.0 = clock.tick.saturating_add(interval);
        }
        return;
    }
    deadline.0 = clock.tick.saturating_add(interval);
    let cloning_center_present = objects.iter().any(|(definition, _, member)| {
        member.root == root
            && (view.find_cloning_center(definition.0).is_some()
                || view.find_object(definition.0).is_some_and(|object| {
                    object
                        .behavior_type_identifiers
                        .contains(&AssetId::from_key("cloningcenter"))
                }))
    });
    if has_disease
        || fame.half_stars < policy.half_star_minimum
        || !cloning_center_present
        || policy.diseases.is_empty()
    {
        return;
    }
    let funding = funding.map_or_else(|| DiseaseResearchFunding::default().0, |funding| funding.0);
    let mut random =
        DeterministicRng::from_entity(*seed, PersistentId(clock.tick), RngDomain::Disease);
    let first = random.next_u32() as usize % policy.diseases.len();
    let authored = policy.diseases[first..]
        .iter()
        .chain(&policy.diseases[..first])
        .filter(|disease| {
            !disease.force_only && disease.search_for_targets && disease.chance_percent > 0.0
        });
    let mut object_types = HashSet::new();
    let mut count_keys = HashSet::new();
    for disease in authored.clone() {
        if let Some(entity_type) = disease.chance_multiplier {
            count_keys.insert(EntityPopulationKey {
                entity_type,
                attribute: None,
                different: false,
            });
            object_types.insert(entity_type);
        }
        for test in &disease.zoo_tests {
            if let DiseaseZooTest::EntityCount {
                entity_type,
                attribute,
                different,
                ..
            } = *test
            {
                count_keys.insert(EntityPopulationKey {
                    entity_type,
                    attribute,
                    different,
                });
                if attribute.is_none() {
                    object_types.insert(entity_type);
                }
            }
        }
        object_types.extend(
            disease
                .animal_filters
                .iter()
                .filter_map(|filter| match filter {
                    DiseaseAnimalFilter::Proximity { entity_type, .. } => Some(*entity_type),
                    _ => None,
                }),
        );
    }
    let mut populations: HashMap<AssetId, ObjectExposurePopulation> = HashMap::default();
    if !object_types.is_empty() {
        for (definition, transform, member) in &objects {
            if member.root != root {
                continue;
            }
            let tags = view
                .find_object(definition.0)
                .into_iter()
                .flat_map(|object| object.behavior_type_identifiers.iter().copied())
                .filter(|tag| *tag != definition.0);
            for tag in std::iter::once(definition.0)
                .chain(tags)
                .filter(|tag| object_types.contains(tag))
            {
                populations
                    .entry(tag)
                    .or_default()
                    .insert(definition.0, transform.translation());
            }
        }
    }
    let mut counts: HashMap<_, ExposureCount> = count_keys
        .into_iter()
        .map(|key| {
            let mut count = if key.different {
                ExposureCount::Distinct(HashSet::new())
            } else {
                ExposureCount::Total(0)
            };
            if let Some(population) = key
                .attribute
                .is_none()
                .then(|| populations.get(&key.entity_type))
                .flatten()
            {
                match &mut count {
                    ExposureCount::Distinct(definitions) => {
                        definitions.extend(population.definitions.iter().copied())
                    }
                    ExposureCount::Total(total) => *total = population.count,
                }
            }
            (key, count)
        })
        .collect();
    if !counts.is_empty() {
        for (animal, facts, _) in &animals {
            if facts.world.root != root {
                continue;
            }
            let types = subject_types.borrow_for(animal);
            for (key, count) in &mut counts {
                if facts.matches_type(key.entity_type, types)
                    && key.attribute.is_none_or(|(attribute, value)| {
                        facts.matches_type(attribute, types) == value
                    })
                {
                    match count {
                        ExposureCount::Distinct(definitions) => {
                            definitions.insert(facts.variant_id());
                        }
                        ExposureCount::Total(total) => *total = total.saturating_add(1),
                    }
                }
            }
        }
    }
    let world = DiseaseExposureWorld {
        subject_types: &subject_types,
        populations,
        counts,
        fame: fame.half_stars,
    };
    for disease in authored {
        if !disease.zoo_tests.iter().all(|test| world.passes(test)) {
            continue;
        }
        let chance = disease.chance_percent
            * disease.chance_multiplier.map_or(1.0, |entity_type| {
                world.count(EntityPopulationKey {
                    entity_type,
                    attribute: None,
                    different: false,
                }) as f32
            });
        let probability = disease_probability(chance, funding);
        if probability == 0 {
            continue;
        }
        let mut assigned = false;
        for (animal, facts, diseased) in &animals {
            if diseased
                || facts.world.root != root
                || !facts.is_adult()
                || !world.qualifies(animal, &facts, disease)
                || tokens
                    .iter_many(token_owners.get(animal).ok().into_iter().flatten().copied())
                    .flatten()
                    .any(|held| held.name == disease.token)
            {
                continue;
            }
            if random.next_u32() % 10_000 >= probability {
                continue;
            }
            assigned = true;
            commands.spawn((
                BehaviorSelfToken {
                    name: disease.token,
                    available_at: clock.tick,
                    expires_at: u64::MAX,
                    payload: None,
                    retain_on_failure: false,
                },
                ChildOf(animal),
            ));
            if disease.single_qualifier {
                break;
            }
        }
        if assigned {
            let cure_duration = (f64::from(policy.cure_timeout_seconds)
                * f64::from(view.timing().fixed_hz))
            .ceil() as u64;
            commands.entity(root).insert(ActiveZooDisease {
                definition: disease.name,
                cure_due_tick: clock.tick.saturating_add(cure_duration),
            });
            deadline.0 = clock.tick.saturating_add(update_interval.max(1));
            break;
        }
    }
}

#[derive(Default)]
struct ObjectExposurePopulation {
    count: u32,
    definitions: HashSet<AssetId>,
    cells: HashMap<IVec3, Vec<Vec3>>,
}

impl ObjectExposurePopulation {
    const CELL_WIDTH_M: f32 = 8.0;

    fn insert(&mut self, definition: AssetId, position: Vec3) {
        self.count = self.count.saturating_add(1);
        self.definitions.insert(definition);
        if position.is_finite() {
            let cell = (position / Self::CELL_WIDTH_M).floor().as_ivec3();
            self.cells.entry(cell).or_default().push(position);
        }
    }

    fn any_within(&self, origin: Vec3, radius: f32) -> bool {
        let radius = radius.abs();
        let min = ((origin - Vec3::splat(radius)) / Self::CELL_WIDTH_M)
            .floor()
            .as_ivec3();
        let max = ((origin + Vec3::splat(radius)) / Self::CELL_WIDTH_M)
            .floor()
            .as_ivec3();
        let contains = |position: &Vec3| position.distance_squared(origin) <= radius * radius;
        let volume = min
            .to_array()
            .into_iter()
            .zip(max.to_array())
            .map(|(min, max)| (f64::from(max) - f64::from(min) + 1.0).max(0.0))
            .product::<f64>();
        if volume > self.cells.len() as f64 {
            return self.cells.values().flatten().any(contains);
        }
        for x in min.x..=max.x {
            for y in min.y..=max.y {
                for z in min.z..=max.z {
                    if self
                        .cells
                        .get(&IVec3::new(x, y, z))
                        .is_some_and(|points| points.iter().any(contains))
                    {
                        return true;
                    }
                }
            }
        }
        false
    }
}

#[derive(Clone, Copy, Hash, PartialEq, Eq)]
struct EntityPopulationKey {
    entity_type: AssetId,
    attribute: Option<(AssetId, bool)>,
    different: bool,
}

enum ExposureCount {
    Distinct(HashSet<AssetId>),
    Total(u32),
}

struct DiseaseExposureWorld<'a, 'w, 's> {
    subject_types: &'a AnimalBehaviorSubjectTypes<'w, 's>,
    populations: HashMap<AssetId, ObjectExposurePopulation>,
    counts: HashMap<EntityPopulationKey, ExposureCount>,
    fame: u8,
}

impl DiseaseExposureWorld<'_, '_, '_> {
    fn animal_is(
        &self,
        animal: Entity,
        facts: &AnimalCandidateFactsItem,
        required: AssetId,
    ) -> bool {
        facts.matches_type(required, self.subject_types.borrow_for(animal))
    }

    fn passes(&self, test: &DiseaseZooTest) -> bool {
        match *test {
            DiseaseZooTest::EntityCount {
                entity_type,
                attribute,
                quantity,
                comparison,
                different,
            } => comparison.holds(
                self.count(EntityPopulationKey {
                    entity_type,
                    attribute,
                    different,
                }),
                quantity,
            ),
            DiseaseZooTest::Fame {
                half_stars,
                comparison,
            } => comparison.holds(u32::from(self.fame), half_stars),
        }
    }

    fn count(&self, key: EntityPopulationKey) -> u32 {
        match self.counts.get(&key) {
            Some(ExposureCount::Distinct(definitions)) => {
                u32::try_from(definitions.len()).unwrap_or(u32::MAX)
            }
            Some(ExposureCount::Total(total)) => *total,
            None => 0,
        }
    }

    fn qualifies(
        &self,
        animal: Entity,
        facts: &AnimalCandidateFactsItem,
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
                    self.populations
                        .get(&entity_type)
                        .is_some_and(|population| {
                            population.any_within(facts.transform.translation(), distance_m)
                        })
                        == closer
                }
            })
    }
}
/// A cured animal drops the disease's pending tokens, its authored
/// `clearTokens`, so it doesn't fall ill or spread the disease again at once.
pub(super) fn clear_authored_disease_tokens_when_cured(
    cured: On<Remove<Disease>>,
    mut commands: Commands,
    diseases: Query<&Disease>,
    tokens: Query<(Entity, &BehaviorSelfToken)>,
    token_owners: Query<&Children>,
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
    let Ok(children) = token_owners.get(cured.entity) else {
        return;
    };
    for (token, held) in tokens.iter_many(children.into_iter().copied()).flatten() {
        if disease.clear_tokens.contains(&held.name) {
            commands.entity(token).despawn();
        }
    }
}
