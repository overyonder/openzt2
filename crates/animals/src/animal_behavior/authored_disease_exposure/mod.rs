use bevy::{ecs::relationship::RelationshipTarget, platform::collections::{HashMap, HashSet}, prelude::*};
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
    ActiveZooDisease, AssignedDiseaseTarget, AssignedDiseaseTargets,
    DiseaseExposureDeadline, DiseaseResearchFunding,
};
use openzt2_zoo_state::animal_behavior::behavior_token_types::BehaviorSelfToken;
use openzt2_zoo_state::animal_health::types::{Dead, Disease};
use openzt2_zoo_state::animal_lifecycle::types::Animal;
use openzt2_zoo_state::progression::fame_types::Fame;

use super::autonomous_task_selection::candidate_facts::{
    AnimalCandidateFacts, AnimalCandidateFactsItem,
};
use super::autonomous_task_selection::target_types::BehaviorTargetTypeMemberships;
use super::behavior_spatial_candidates::BehaviorSpatialCandidates;
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
    clock: Single<&ZooClock>,
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

#[allow(clippy::too_many_arguments)]
pub(super) fn expose_animals_to_authored_diseases(
    mut commands: Commands,
    clock: Single<&ZooClock>,
    seed: Single<&ZooSeed>,
    definition_assets: Res<Assets<WorldDefinitionAsset>>,
    definitions: Res<WorldDefinitions>,
    subject_types: AnimalBehaviorSubjectTypes,
    animals: DiseaseAnimals,
    objects: Query<(&DefinitionId, &GlobalTransform, &WorldMember), Without<Animal>>,
    types: BehaviorTargetTypeMemberships,
    spatial: BehaviorSpatialCandidates,
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
    (assigned_targets, assignments): (
        Query<&AssignedDiseaseTargets>,
        Query<(Entity, &AssignedDiseaseTarget, &ChildOf)>,
    ),
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
    let mut candidates = Vec::new();
    types.extend_type(root, AssetId::from_key("cloningcenter"), &mut candidates);
    let cloning_center_present =
        objects
            .iter_many(candidates.iter().copied())
            .flatten()
            .any(|(definition, _, member)| {
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
        DeterministicRng::from_entity(**seed, PersistentId(clock.tick), RngDomain::Disease);
    let first = random.next_u32() as usize % policy.diseases.len();
    let registered: Vec<_> = assignments
        .iter_many(
            assigned_targets
                .get(root)
                .ok()
                .into_iter()
                .flat_map(RelationshipTarget::iter),
        )
        .flatten()
        .collect();
    let authored = policy.diseases[first..]
        .iter()
        .chain(&policy.diseases[..first])
        .filter(|disease| {
            !disease.force_only
                && disease.chance_percent > 0.0
                && (disease.search_for_targets
                    || registered
                        .iter()
                        .any(|(_, item, _)| item.disease == disease.name))
        });
    let mut count_keys = HashSet::new();
    for disease in authored.clone() {
        if let Some(entity_type) = disease.chance_multiplier {
            count_keys.insert(EntityPopulationKey {
                entity_type,
                attribute: None,
                different: false,
            });
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
            if key.attribute.is_none() {
                candidates.clear();
                types.extend_type(root, key.entity_type, &mut candidates);
                for (definition, _, member) in
                    objects.iter_many(candidates.iter().copied()).flatten()
                {
                    if member.root != root {
                        continue;
                    }
                    match &mut count {
                        ExposureCount::Distinct(definitions) => {
                            definitions.insert(definition.0);
                        }
                        ExposureCount::Total(total) => *total = total.saturating_add(1),
                    }
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
        counts,
        fame: fame.half_stars,
    };
    let mut nearby = |origin: Vec3, radius: f32, entity_type: AssetId| {
        let radius = radius.abs();
        let extent = Vec2::splat(radius);
        spatial.in_bounds(
            root,
            origin.xz() - extent,
            origin.xz() + extent,
            &mut candidates,
        );
        objects
            .iter_many(candidates.iter().copied())
            .flatten()
            .any(|(definition, pose, member)| {
                member.root == root
                    && pose.translation().distance_squared(origin) <= radius * radius
                    && (definition.0 == entity_type
                        || view.find_object(definition.0).is_some_and(|object| {
                            object.behavior_type_identifiers.contains(&entity_type)
                        }))
            })
    };
    for disease in authored {
        let registered_for_disease = registered
            .iter()
            .filter(|(_, item, _)| item.disease == disease.name);
        let has_registered = registered_for_disease.clone().next().is_some();
        for (assignment, _, _) in registered_for_disease.clone() {
            commands.entity(*assignment).try_despawn();
        }
        let registered_animals =
            registered_for_disease.filter_map(|(_, _, parent)| animals.get(parent.parent()).ok());
        let candidate_animals = registered_animals.chain(
            animals
                .iter()
                .filter(|_| !has_registered && disease.search_for_targets),
        );
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
        for (animal, facts, diseased) in candidate_animals {
            if diseased
                || facts.world.root != root
                || !facts.is_adult()
                || (!has_registered && !world.qualifies(animal, &facts, disease, &mut nearby))
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
        nearby: &mut impl FnMut(Vec3, f32, AssetId) -> bool,
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
                } => nearby(facts.transform.translation(), distance_m, entity_type) == closer,
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

pub(super) fn add_authored_disease_target(
    activation: On<openzt2_zoo_state::behavior_task_execution_types::running_actions::WhenBehaviorAction<
        openzt2_zoo_state::behavior_task_execution_types::running_actions::RunBehaviorAddSubjectAsDiseaseTarget,
    >>,
    clock: Single<&ZooClock>,
    definitions: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    subject_types: AnimalBehaviorSubjectTypes,
    animals: DiseaseAnimals,
    actions: Query<(
        &openzt2_zoo_state::behavior_task_execution_types::running_actions::RunningBehaviorInstruction,
        &openzt2_zoo_state::behavior_task_execution_types::running_actions::RunBehaviorAddSubjectAsDiseaseTarget,
    )>,
    worlds: Query<
        (
            Option<&ActiveZooDisease>,
            Option<&DiseaseExposureDeadline>,
            &Fame,
        ),
        With<WorldRoot>,
    >,
    facilities: Query<(&DefinitionId, &WorldMember), Without<Animal>>,
    infected: Query<&WorldMember, (With<Animal>, With<Disease>, Without<Dead>)>,
    tokens: Query<&BehaviorSelfToken>,
    token_owners: Query<&Children>,
    assignments: Query<&AssignedDiseaseTarget>,
    mut commands: Commands,
) {
    let actor = activation.entity;
    let Ok((instruction, action)) = actions.get(actor) else {
        return;
    };
    let action = &action.0;
    let Some((root, view, policy, disease)) = (|| {
        let (_, facts, _) = animals.get(actor).ok()?;
        let matches = |id| facts.matches_type(id, subject_types.borrow_for(actor));
        if !facts.is_adult()
            || (!action.types.is_empty() && !action.types.iter().copied().any(matches))
            || !action
                .attributes
                .iter()
                .all(|&(id, value)| matches(id) == value)
        {
            return None;
        }
        let view = definitions.get(&assets)?;
        let policy = view.disease_exposure()?;
        let disease = policy
            .diseases
            .iter()
            .find(|disease| disease.name == action.disease)?;
        Some((facts.world.root, view, policy, disease))
    })() else {
        instruction.complete(actor, clock.tick, &mut commands);
        return;
    };

    let Ok((active, _, fame)) = worlds.get(root) else {
        super::behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation(
            actor,
            &mut commands,
        );
        return;
    };
    let has_cloning_center = || {
        facilities.iter().any(|(definition, member)| {
            member.root == root
                && (view.find_cloning_center(definition.0).is_some()
                    || view.find_object(definition.0).is_some_and(|object| {
                        object
                            .behavior_type_identifiers
                            .contains(&AssetId::from_key("cloningcenter"))
                    }))
        })
    };
    let immediate = action.immediate
        && active.is_none()
        && fame.half_stars >= policy.half_star_minimum
        && !infected.iter().any(|member| member.root == root)
        && has_cloning_center()
        && disease.zoo_tests.iter().all(|test| match *test {
            DiseaseZooTest::Fame {
                half_stars,
                comparison,
            } => comparison.holds(u32::from(fame.half_stars), half_stars),
            DiseaseZooTest::EntityCount {
                entity_type,
                attribute,
                quantity,
                comparison,
                different,
            } => {
                let animal_keys = animals.iter().filter_map(|(entity, facts, _)| {
                    let types = subject_types.borrow_for(entity);
                    (facts.world.root == root
                        && facts.matches_type(entity_type, types)
                        && attribute
                            .is_none_or(|(name, state)| facts.matches_type(name, types) == state))
                    .then_some(facts.variant_id())
                });
                let object_keys = facilities.iter().filter_map(|(definition, member)| {
                    (member.root == root
                        && attribute.is_none()
                        && (definition.0 == entity_type
                            || view.find_object(definition.0).is_some_and(|object| {
                                object.behavior_type_identifiers.contains(&entity_type)
                            })))
                    .then_some(definition.0)
                });
                let matching = animal_keys.chain(object_keys);
                let count = if different {
                    matching.collect::<HashSet<_>>().len()
                } else {
                    matching.count()
                };
                comparison.holds(u32::try_from(count).unwrap_or(u32::MAX), quantity)
            }
        });
    if !immediate {
        if !assignments
            .iter_many(token_owners.get(actor).ok().into_iter().flatten().copied())
            .flatten()
            .any(|assignment| assignment.root == root && assignment.disease == disease.name)
        {
            commands.spawn((
                AssignedDiseaseTarget {
                    root,
                    disease: disease.name,
                },
                ChildOf(actor),
            ));
        }
        instruction.complete(actor, clock.tick, &mut commands);
        return;
    }
    if !tokens
        .iter_many(token_owners.get(actor).ok().into_iter().flatten().copied())
        .flatten()
        .any(|token| token.name == disease.token)
    {
        commands.spawn((
            BehaviorSelfToken {
                name: disease.token,
                available_at: clock.tick,
                expires_at: u64::MAX,
                payload: None,
                retain_on_failure: false,
            },
            ChildOf(actor),
        ));
    }
    if immediate {
        let duration = (f64::from(policy.cure_timeout_seconds) * f64::from(view.timing().fixed_hz))
            .ceil() as u64;
        commands.entity(root).insert_if_new(ActiveZooDisease {
            definition: disease.name,
            cure_due_tick: clock.tick.saturating_add(duration),
        });
    }
    instruction.complete(actor, clock.tick, &mut commands);
}
