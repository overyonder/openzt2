//! Starts authored animal tasks through the existing behavior executor.

mod animal_targets;
pub(crate) mod candidate_facts;
mod scoring;
pub(super) mod target_types;
pub(super) mod task_cooldowns;
mod world_object_targets;

use super::{
    behavior_candidate_selection::{
        candidate_requirements_are_satisfied, BehaviorCandidateRanking,
    },
    behavior_random_stream_state::BehaviorRandomStream,
    behavior_set_start_request_types::PendingBehaviorSet,
};
use openzt2_game_assets::behavior::behavior_asset_types::BehaviorDocumentAsset;
use openzt2_game_assets::behavior::behavior_asset_types::LoadedBehaviorDocumentCollection;
use openzt2_game_assets::species::species_asset_types::SpeciesAsset;
use openzt2_game_assets::species::species_asset_types::SpeciesAssets;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_zoo_state::animal_health::types::Dead;
use openzt2_zoo_state::animal_health::types::Frozen;
use openzt2_zoo_state::animal_health::types::Tranquilized;
use openzt2_zoo_state::animal_lifecycle::types::Animal;
use openzt2_zoo_state::animal_behavior::behavior_task_effective_score_types::BehaviorTaskEffectiveScore;
use openzt2_zoo_state::animal_behavior::behavior_target_retry_delays::BehaviorTargetRejections;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionPhase;
use openzt2_zoo_state::behavior_task_execution_types::{allocate_behavior_execution_id, BehaviorTaskGeneration};
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionState;
use openzt2_zoo_state::locomotion::locomotion_types::Destination;
use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_world_state::world_spawn::world_entity_crating::UncratedWorldEntity;
use bevy::{ecs::system::ParallelCommands, prelude::*};
use target_types::BehaviorTargetTypeMemberships;
use openzt2_zoo_state::animal_behavior::behavior_natural_water_target_types::SelectedNaturalWaterTarget;
use candidate_facts::{
    AnimalCandidateFacts, AnimalCandidateFactsItem, animal_subject_type_can_match,
};
use openzt2_game_data::{
    behavior::{document::BehaviorTask, eligibility::{
        BehaviorCandidateEligibilityFact, BehaviorCandidateRole, BehaviorConditionEligibilityFact,
        BehaviorEligibilityFactInput, BehaviorEligibilityFactValue,
    }},
    AssetId,
};
use task_cooldowns::{AnimalTaskCooldowns, RunningDelayedAnimalTask};
use world_object_targets::{
    indexed_world_object_target_requirement, WorldObjectTargetLocation, WorldObjectTargetSources,
};
use openzt2_zoo_state::animal_behavior::behavior_token_types::{BehaviorSelfToken, ClaimedBehaviorSelfToken};

#[derive(Default)]
pub(super) struct AnimalSubjectTaskIndex {
    by_variant:
        bevy::platform::collections::HashMap<AssetId, Vec<(Handle<BehaviorDocumentAsset>, usize)>>,
}

pub(super) type IdleAnimalTaskFilter = (
    With<Animal>,
    Without<BehaviorTaskExecutionState>,
    Without<PendingBehaviorSet>,
    Without<Dead>,
    Without<Frozen>,
    Without<Tranquilized>,
    UncratedWorldEntity,
    Without<Destination>,
    Without<openzt2_zoo_state::behavior_task_execution_types::SynchronizedBehaviorParticipant>,
);

#[derive(Component, Debug)]
pub(super) struct PendingAnimalTaskStart {
    document: Handle<BehaviorDocumentAsset>,
    program: AssetId,
    declaration: usize,
    target: Entity,
    object: Option<Entity>,
    shore: Option<SelectedNaturalWaterTarget>,
    priority: f32,
    score: f32,
    delay_ticks: Option<u64>,
}

pub(super) fn select_idle_animal_tasks(
    (documents, assets, mut subject_tasks): (
        Res<LoadedBehaviorDocumentCollection>,
        Res<Assets<BehaviorDocumentAsset>>,
        Local<AnimalSubjectTaskIndex>,
    ),
    world_assets: Res<Assets<WorldDefinitionAsset>>,
    world_definitions: Res<WorldDefinitions>,
    species_assets: Res<Assets<SpeciesAsset>>,
    species_index: Res<SpeciesAssets>,
    clock: Single<&ZooClock>,
    target_retry_delays: BehaviorTargetRejections,
    mut animals: Query<
        (
            Entity,
            AnimalCandidateFacts,
            &mut BehaviorRandomStream,
            Option<&mut AnimalTaskCooldowns>,
        ),
        IdleAnimalTaskFilter,
    >,
    world_objects: WorldObjectTargetSources,
    target_types: BehaviorTargetTypeMemberships,
    shores: super::natural_water_targets::NaturalWaterTargets,
    (tokens, token_owners, memberships): (
        Query<(Entity, &BehaviorSelfToken), Without<ClaimedBehaviorSelfToken>>,
        Query<&Children>,
        openzt2_zoo_state::animal_behavior::interaction_container_membership::InteractionContainerMemberships,
    ),
    animal_targets: animal_targets::AnimalTargetSources,
    biomes: openzt2_zoo_state::animal_behavior::behavior_candidate_selection::BehaviorTargetBiome,
    token_facts: Query<
        &openzt2_zoo_state::animal_behavior::behavior_token_types::BehaviorTokenFacts,
    >,
    commands: ParallelCommands,
) {
    let authored_tasks_changed = documents.is_changed()
        || assets.is_changed()
        || species_assets.is_changed()
        || species_index.is_changed();
    if authored_tasks_changed {
        subject_tasks.by_variant.clear();
    }
    if animals.is_empty() {
        return;
    }
    let Some(view) = documents.create_declaration_index_view(&assets) else {
        return;
    };
    let Some(definitions) = world_definitions.get(&world_assets) else {
        return;
    };
    let Some(species) = species_index.get(&species_assets) else {
        return;
    };
    let Some(policy) = definitions.behavior_selection_policy() else {
        return;
    };
    if !policy.enabled {
        return;
    }
    if subject_tasks.by_variant.is_empty() {
        for definition in species.species() {
            for variant in species.variants(definition.id) {
                let authored_types = &variant.behavior_subject_type_identifiers;
                let declarations = view
                    .behavior_tasks()
                    .filter(|(_, _, task)| {
                        task_has_typed_subject_and_target(task)
                            && candidate_requirements_are_satisfied(
                                &task.candidate_eligibility_requirements,
                                |role, required| {
                                    role != BehaviorCandidateRole::Subject
                                        || animal_subject_type_can_match(
                                            required,
                                            authored_types,
                                            definition.id,
                                            variant.id,
                                        )
                                },
                                |_, _| true,
                            )
                    })
                    .map(|(document, index, _)| (document.clone(), index))
                    .collect();
                subject_tasks.by_variant.insert(variant.id, declarations);
            }
        }
    }
    let subject_tasks = &subject_tasks.by_variant;
    animals.par_iter_mut().for_each_init(
        || (Vec::<Entity>::new(), Vec::<SelectedNaturalWaterTarget>::new()),
        |(candidates, shore_candidates), (animal, facts, mut random, cooldowns)| {
        let Some((definition, variant)) = species.find_variant_with_species(facts.variant_id()) else {
            return;
        };
        if definition.id != facts.species.species {
            return;
        }
        let Some(mut cooldowns) = cooldowns else {
            commands.command_scope(|mut commands| {
                commands.entity(animal).insert(AnimalTaskCooldowns::default());
            });
            return;
        };
        let Some(declarations) = subject_tasks.get(&variant.id) else {
            return;
        };
        let authored_types = &variant.behavior_subject_type_identifiers;
        let area_restricted = authored_types.contains(&AssetId::from_key("b_generatestraversabilityinfo"))
            || authored_types.contains(&AssetId::from_key("unspecified"));
        let animal_habitat = world_objects.habitat_at(facts.world.root, facts.transform.translation());
        let mut ranking = BehaviorCandidateRanking::new(policy.minimum_score);
        let mut considered_candidates = 0_u32;
        let held_tokens: Vec<_> = tokens
            .iter_many(token_owners.get(animal).ok().into_iter().flatten().copied())
            .flatten()
            .filter(|(_, token)| clock.tick >= token.available_at && clock.tick < token.expires_at)
            .collect();
        let ticks_per_day = definitions.timing().ticks_per_day;
        let held_items = memberships.members_of_container(animal).collect::<Vec<_>>();
        let slot_of = |object| {
            let (_, slot) = memberships.container_for_member(object)?;
            definitions.find_object(variant.id).or_else(|| definitions.find_object(definition.id))?
                .interaction_slots.get(slot).map(|slot| slot.reservation_tag)
        };
        let object_type = |object: Option<Entity>, required| {
            let Some(object) = object else { return false; };
            if let Ok((_, token)) = tokens.get(object) { return token.name == required; }
            if let Ok((_, object)) = animal_targets.animals.get(object) {
                return species.find_variant_with_species(object.variant_id()).is_some_and(|(_, variant)| {
                    object.matches_type(required, &variant.behavior_subject_type_identifiers)
                });
            }
            world_objects.objects.get(object).ok().is_some_and(|object| {
                definitions.find_object(object.definition.0).is_some_and(|definition| object.matches_type(required, definition))
            })
        };
        let object_fact = |object: Option<Entity>, fact: &BehaviorCandidateEligibilityFact| {
            let Some(object) = object else { return false; };
            if let (BehaviorEligibilityFactInput::Condition(BehaviorConditionEligibilityFact::ObjectContainer),
                BehaviorEligibilityFactValue::Symbol(required)) = (fact.fact_input, fact.expected_value)
            {
                return slot_of(object) == Some(required);
            }
            if let Ok(facts) = token_facts.get(object) { return facts.satisfies(fact); }
            if let Ok((_, object_facts)) = animal_targets.animals.get(object) {
                return species.find_variant_with_species(object_facts.variant_id()).is_some_and(|(_, variant)| {
                    animal_targets.target_satisfies(animal, &facts, object, &object_facts, fact,
                        clock.tick, ticks_per_day, clock.tick_in_day, &variant.behavior_subject_type_identifiers)
                });
            }
            world_objects.objects.get(object).ok().is_some_and(|object| {
                let Some(definition) = definitions.find_object(object.definition.0) else { return false; };
                let position = object.transform.translation();
                object.satisfies(fact, &WorldObjectTargetLocation {
                    in_habitat: animal_habitat == world_objects.habitat_at(facts.world.root, position),
                    on_land: || world_objects.terrain_surfaces.on_land_at_position(facts.world.root, position),
                    distance: facts.transform.translation().distance(position),
                    building_strength: definition.destruction.as_ref().and_then(|rule| rule.strength),
                    definition,
                    spatial: |kind, expected| animal_targets.spatial_value_at(facts.world.root, position, kind, expected),
                })
            })
        };
        for (document, declaration) in declarations.iter() {
            let declaration = *declaration;
            let Some(task) = assets
                .get(document)
                .and_then(|asset| asset.behavior_task_at_index(declaration))
            else {
                continue;
            };
            if !subject_can_consider_task(task, &facts, authored_types, &animal_targets, &clock, ticks_per_day) {
                cooldowns.disarm(task.id);
                continue;
            }
            if let Some(delay) = task
                .task_delay_seconds
                .filter(|delay| *delay != [0.0, 0.0])
                .filter(|_| !cooldowns.is_armed(task.id))
            {
                let delay_ticks = task_cooldowns::sample_task_delay_ticks(
                    delay,
                    random.0.unit_f32(),
                    definitions.timing().fixed_hz,
                );
                debug!(?animal, task = %task.name, delay_ticks,
                    "animal task became possible and waits out its delay");
                cooldowns.arm(task.id, clock.tick, delay_ticks);
                continue;
            }
            if cooldowns.is_cooling_down(task.id, clock.tick) {
                debug!(?animal, task = %task.name, id = ?task.id, "animal task cooling down");
                continue;
            }
            let from_token = task.targets.len() == 1 && task.targets[0].eq_ignore_ascii_case("fromtoken");
            let objects_for = |target| {
                std::iter::once(None).filter(move |_| task.objects.is_empty() && !from_token)
                    .chain(held_tokens.iter().filter(move |(_, token)| {
                        if from_token { token.payload == Some(target) }
                        else { !task.objects.is_empty() }
                    }).map(|(token, _)| Some(*token)))
                    .chain(held_items.iter().copied().filter(move |_| !task.objects.is_empty() && !from_token).map(Some))
            };
            if task.targets.len() == 1 && task.targets[0] == "self" {
                for object in objects_for(animal) {
                    if !candidate_requirements_are_satisfied(
                        &task.candidate_eligibility_requirements,
                        |role, required| match role {
                            BehaviorCandidateRole::Subject => true,
                            BehaviorCandidateRole::Target => required == AssetId::from_key("self"),
                            BehaviorCandidateRole::Object => object_type(object, required),
                        },
                        |role, fact| match role {
                            BehaviorCandidateRole::Subject => true,
                            BehaviorCandidateRole::Target => animal_targets.spatial_satisfies(&facts, fact, clock.tick_in_day, ticks_per_day).unwrap_or_else(|| facts.satisfies(fact, clock.tick, ticks_per_day, authored_types)),
                            BehaviorCandidateRole::Object => object_fact(object, fact),
                        },
                    ) {
                        trace!(?animal, task = %task.name, "animal self task qualifiers refused");
                        continue;
                    }
                    let Some(score) = scoring::score_animal_task(
                        task,
                        &facts,
                        (clock.tick, ticks_per_day),
                        policy.outside_range_need_value,
                        0.0,
                        || None,
                        &mut random,
                    ) else {
                        trace!(?animal, task = %task.name, "animal self task has no supported score");
                        continue;
                    };
                    considered_candidates += 1;
                    trace!(?animal, task = %task.name, score, priority = task.priority.unwrap_or_default(),
                    "animal self task scored");
                    ranking.consider(
                        (
                            document,
                            declaration,
                            task,
                            animal,
                            object,
                            None,
                        ),
                        task.priority.unwrap_or_default(),
                        score,
                    );
                }
                continue;
            }
            if from_token {
                candidates.clear();
                candidates.extend(held_tokens.iter().filter_map(|(_, token)| token.payload));
                target_types.sort_by_table_order(candidates);
                candidates.dedup();
            } else {
                let requirement = task.candidate_eligibility_requirements.iter().find(|requirement| {
                    requirement.candidate_role == BehaviorCandidateRole::Target
                        && !requirement.candidate_type_identifiers.is_empty()
                });
                target_types.candidates(facts.world.root, requirement, true, candidates);
            }
            for (target, target_facts) in animal_targets.animals.iter_many(candidates.iter().copied()).flatten() {
                if target == animal || target_facts.world.root != facts.world.root {
                    continue;
                }
                let Some((_, target_variant)) = species.find_variant_with_species(target_facts.variant_id()) else {
                    continue;
                };
                let target_types = &target_variant.behavior_subject_type_identifiers;
                for object in objects_for(target) {
                if !candidate_requirements_are_satisfied(
                    &task.candidate_eligibility_requirements,
                    |role, required| match role {
                        BehaviorCandidateRole::Subject => true,
                        BehaviorCandidateRole::Target => {
                            if from_token {
                                required == AssetId::from_key("fromtoken")
                            } else {
                                target_facts.matches_type(required, target_types)
                            }
                        }
                        BehaviorCandidateRole::Object => object_type(object, required),
                    },
                    |role, fact| match role {
                        BehaviorCandidateRole::Subject => true,
                        BehaviorCandidateRole::Target => animal_targets.target_satisfies(
                            animal,
                            &facts,
                            target,
                            &target_facts,
                            fact,
                            clock.tick,
                            ticks_per_day,
                            clock.tick_in_day,
                            target_types,
                        ),
                        BehaviorCandidateRole::Object => object_fact(object, fact),
                    },
                ) {
                    continue;
                }
                let distance = facts
                    .transform
                    .translation()
                    .distance(target_facts.transform.translation());
                let Some(score) = scoring::score_animal_task(
                    task,
                    &facts,
                    (clock.tick, ticks_per_day),
                    policy.outside_range_need_value,
                    distance,
                    || biomes.at(facts.world.root, target_facts.transform.translation()),
                    &mut random,
                ) else {
                    continue;
                };
                considered_candidates += 1;
                debug!(?animal, task = %task.name, ?target, score, distance,
                    "animal task animal target scored");
                ranking.consider(
                    (
                        document,
                        declaration,
                        task,
                        target,
                        object,
                        None,
                    ),
                    task.priority.unwrap_or_default(),
                    score,
                );
            }
            }
            if task.targets.len() == 1
                && (task.targets[0].eq_ignore_ascii_case("shore")
                    || task.targets[0].eq_ignore_ascii_case("water"))
            {
                let water_target = task.targets[0].eq_ignore_ascii_case("water");
                if water_target && variant.surface_water_locomotion.is_none() {
                    continue;
                }
                use openzt2_game_data::behavior::eligibility::{
                    BehaviorEligibilityFactInput as Input, BehaviorEligibilityFactValue as Value,
                    BehaviorEntityStateEligibilityFact as State,
                    BehaviorSpatialEligibilityFact as Spatial,
                };
                let radius = task
                    .candidate_eligibility_requirements
                    .iter()
                    .filter(|requirement| {
                        requirement.candidate_role == BehaviorCandidateRole::Target
                    })
                    .flat_map(|requirement| &requirement.eligibility_facts)
                    .filter_map(|fact| match (fact.fact_input, fact.expected_value) {
                        (Input::Spatial(Spatial::InSight), Value::Q16(radius)) if radius > 0 => {
                            Some(radius as f32 / 65_536.0)
                        }
                        _ => None,
                    })
                    .reduce(f32::max);
                let Some(radius) = radius else {
                    continue;
                };
                if water_target {
                    shores.collect_water(
                        facts.world.root,
                        facts.transform.translation(),
                        radius,
                        facts.escaped,
                        shore_candidates,
                    );
                } else {
                    shores.collect(
                        animal,
                        facts.world.root,
                        facts.transform.translation(),
                        radius,
                        facts.escaped,
                        shore_candidates,
                    );
                }
                for shore in shore_candidates.iter().copied() {
                    for object in objects_for(shore.region) {
                    let distance = facts
                        .transform
                        .translation()
                        .xz()
                        .distance(shore.water.xz());
                    if !candidate_requirements_are_satisfied(
                        &task.candidate_eligibility_requirements,
                        |role, required| match role {
                            BehaviorCandidateRole::Subject => true,
                            BehaviorCandidateRole::Target => {
                                required
                                    == AssetId::from_key(if water_target {
                                        "water"
                                    } else {
                                        "shore"
                                    })
                            }
                            BehaviorCandidateRole::Object => object_type(object, required),
                        },
                        |role, fact| match role {
                            BehaviorCandidateRole::Subject => true,
                            BehaviorCandidateRole::Target => {
                                match (fact.fact_input, fact.expected_value) {
                                    (Input::Spatial(Spatial::InSight), Value::Q16(radius)) => {
                                        distance < radius as f32 / 65_536.0
                                    }
                                    (Input::State(State::InHabitat), Value::Q16(expected)) => {
                                        candidate_facts::compare_q16(
                                            1 << 16,
                                            expected,
                                            fact.comparison,
                                        )
                                    }
                                    (Input::Spatial(Spatial::WaterDepth), Value::Q16(expected)) => {
                                        if let openzt2_zoo_state::animal_behavior::behavior_natural_water_target_types::NaturalWaterTargetKind::Surface { depth } = shore.kind {
                                            candidate_facts::compare_q16((depth * 65_536.0) as i32, expected, fact.comparison)
                                        } else { false }
                                    }
                                    _ => false,
                                }
                            }
                            BehaviorCandidateRole::Object => object_fact(object, fact),
                        },
                    ) {
                        continue;
                    }
                    let Some(score) = scoring::score_animal_task(
                        task,
                        &facts,
                        (clock.tick, ticks_per_day),
                        policy.outside_range_need_value,
                        distance,
                        || biomes.at(facts.world.root, shore.water),
                        &mut random,
                    ) else {
                        continue;
                    };
                    considered_candidates += 1;
                    ranking.consider(
                        (document, declaration, task, shore.region, object, Some(shore)),
                        task.priority.unwrap_or_default(),
                        score,
                    );
                    }
                }
                continue;
            }
            if task.candidate_eligibility_requirements.iter().any(|requirement| {
                requirement.candidate_role == BehaviorCandidateRole::Target
                    && target_types.includes_fence(requirement)
            }) {
                use openzt2_game_data::behavior::action_record::BehaviorAction;
                let target_morph = task.execution.lowered_actions().iter().find_map(|action| {
                    match action {
                        BehaviorAction::MorphEntity {
                            recipient: openzt2_game_data::behavior::action_record::BehaviorMorphRecipient::Target,
                            definition, ..
                        } => Some(*definition),
                        BehaviorAction::AnimationKill { radius: Some((_, kind)), destroy: true, .. }
                            if *kind == AssetId::from_key("fence") => Some(AssetId::from_key("s_broken")),
                        _ => None,
                    }
                });
                let boundary = animal_habitat
                    .into_iter()
                    .flat_map(|habitat| world_objects.habitat_boundaries.fences(habitat));
                for fence in boundary {
                    for selected_object in objects_for(fence) {
                    let Ok((edge, transform)) = world_objects.fences.get(fence) else {
                        continue;
                    };
                    let (Some(definition), Some(object)) = (
                        definitions.find_fence(edge.definition),
                        definitions.find_object(edge.definition),
                    ) else {
                        continue;
                    };
                    if target_morph.is_some_and(|selector| {
                        super::behavior_target_morph_execution::resolve_fence_morph(
                            definitions, edge.definition, selector,
                        ).is_none()
                    }) {
                        continue;
                    }
                    let position = world_objects.fence_middle(facts.world.root, &edge).unwrap_or_else(|| transform.translation());
                    let distance = facts.transform.translation().distance(position);
                    let electric = object
                        .behavior_type_identifiers
                        .contains(&AssetId::from_key("b_electric"));
                    if !candidate_requirements_are_satisfied(
                        &task.candidate_eligibility_requirements,
                        |role, required| match role {
                            BehaviorCandidateRole::Subject => true,
                            BehaviorCandidateRole::Target => {
                                object.id == required || object.behavior_type_identifiers.contains(&required)
                            }
                            BehaviorCandidateRole::Object => object_type(selected_object, required),
                        },
                        |role, fact| match role {
                            BehaviorCandidateRole::Subject => true,
                            BehaviorCandidateRole::Target => {
                                fence_satisfies(fact, definition.strength, electric, distance)
                            }
                            BehaviorCandidateRole::Object => object_fact(selected_object, fact),
                        },
                    ) || target_retry_delays.rejects(animal, fence)
                    {
                        debug!(?animal, task = %task.name, ?fence, strength = definition.strength,
                            electric, "animal fence task refused the fence");
                        continue;
                    }
                    let Some(score) = scoring::score_animal_task(
                        task,
                        &facts,
                        (clock.tick, ticks_per_day),
                        policy.outside_range_need_value,
                        distance,
                        || biomes.at(facts.world.root, position),
                        &mut random,
                    ) else {
                        debug!(?animal, task = %task.name, ?fence,
                            "animal fence task has no score for the fence");
                        continue;
                    };
                    debug!(?animal, task = %task.name, ?fence, score, "animal fence task scored");
                    considered_candidates += 1;
                    ranking.consider(
                        (document, declaration, task, fence, selected_object, None),
                        task.priority.unwrap_or_default(),
                        score,
                    );
                    }
                }
            }
            if !from_token {
                let Some(target_requirement) =
                    indexed_world_object_target_requirement(&task.candidate_eligibility_requirements)
                else {
                    continue;
                };
                target_types.candidates(facts.world.root, Some(target_requirement), false, candidates);
            }
            for candidate in candidates.iter().copied() {
                let Ok(target) = world_objects.objects.get(candidate) else {
                    debug!(?animal, task = %task.name, ?candidate,
                        "animal task candidate is crated or not a world object");
                    continue;
                };
                let Some(object) = definitions.find_object(target.definition.0) else {
                    continue;
                };
                if target.world.root != facts.world.root
                    || !target.has_consistent_container_state(object)
                {
                    debug!(?animal, task = %task.name, ?candidate,
                        same_world = target.world.root == facts.world.root,
                        "animal task candidate has another world or an unhydrated container");
                    continue;
                }
                let target_position = target.transform.translation();
                let location = WorldObjectTargetLocation {
                    // An escaped animal reaches what its breach leads to.
                    in_habitat: !area_restricted
                        || if facts.escaped {
                            world_objects.escaped_animal_reaches(facts.world.root, target_position)
                        } else {
                            animal_habitat.is_some_and(|habitat| {
                                world_objects.habitat_at(facts.world.root, target_position) == Some(habitat)
                            })
                        },
                    on_land: || world_objects.terrain_surfaces.on_land_at_position(facts.world.root, target_position),
                    distance: facts.transform.translation().distance(target_position),
                    building_strength: object
                        .destruction
                        .as_ref()
                        .and_then(|destruction| destruction.strength),
                    definition: object,
                    spatial: |kind, expected| animal_targets.spatial_value_at(facts.world.root, target_position, kind, expected),
                };
                for selected_object in objects_for(candidate) {
                if !candidate_requirements_are_satisfied(
                    &task.candidate_eligibility_requirements,
                    |role, required| match role {
                        BehaviorCandidateRole::Subject => true,
                        BehaviorCandidateRole::Target if from_token => required == AssetId::from_key("fromtoken"),
                        BehaviorCandidateRole::Target => target.matches_type(required, object),
                        BehaviorCandidateRole::Object => object_type(selected_object, required),
                    },
                    |role, fact| match role {
                        BehaviorCandidateRole::Subject => true,
                        BehaviorCandidateRole::Target => target.satisfies(fact, &location),
                        BehaviorCandidateRole::Object => object_fact(selected_object, fact),
                    },
                ) {
                    if bevy::log::tracing::enabled!(bevy::log::Level::DEBUG) {
                        let refused_target_facts = task
                            .candidate_eligibility_requirements
                            .iter()
                            .filter(|requirement| {
                                requirement.candidate_role == BehaviorCandidateRole::Target
                            })
                            .flat_map(|requirement| &requirement.eligibility_facts)
                            .filter(|fact| !target.satisfies(fact, &location))
                            .map(|fact| (fact.fact_input, fact.comparison, fact.expected_value))
                            .collect::<Vec<_>>();
                        debug!(?animal, task = %task.name, ?candidate,
                            in_habitat = location.in_habitat, area_restricted, ?animal_habitat,
                            target_habitat = ?world_objects.habitat_at(facts.world.root, target_position),
                            ?refused_target_facts,
                            "animal task target refused by qualifiers");
                    }
                    continue;
                }
                let distance = location.distance;
                if target_retry_delays.rejects(animal, target.entity) {
                    continue;
                }
                if !distance.is_finite() {
                    continue;
                }
                let Some(score) = scoring::score_animal_task(
                    task,
                    &facts,
                    (clock.tick, ticks_per_day),
                    policy.outside_range_need_value,
                    distance,
                    || biomes.at(facts.world.root, target_position),
                    &mut random,
                ) else {
                    debug!(?animal, task = %task.name, ?candidate,
                        "animal task has no supported score");
                    continue;
                };
                considered_candidates += 1;
                debug!(?animal, task = %task.name, ?candidate, score, distance,
                    priority = task.priority.unwrap_or_default(),
                    minimum_score = policy.minimum_score,
                    "animal task target scored");
                ranking.consider(
                    (document, declaration, task, target.entity, selected_object, None),
                    task.priority.unwrap_or_default(),
                    score,
                );
                }
            }
        }
        let Some(((document, declaration, task, target, object, shore), score)) =
            ranking.finish(|| random.next_u32())
        else {
            debug!(?animal, considered_candidates,
                thirst = ?facts.source_need(AssetId::from_key("thirst")),
                hunger = ?facts.source_need(AssetId::from_key("hunger")),
                outside_range_need_value = policy.outside_range_need_value,
                minimum_score = policy.minimum_score,
                "animal selected no task; needs are (deprivation q16, triggered)");
            return;
        };
        let delay_ticks = task
            .task_delay_seconds
            .filter(|delay| *delay != [0.0, 0.0])
            .map(|delay| {
                task_cooldowns::sample_task_delay_ticks(
                    delay,
                    random.0.unit_f32(),
                    definitions.timing().fixed_hz,
                )
            });
        commands.command_scope(|mut commands| {
            commands.entity(animal).insert(PendingAnimalTaskStart {
                document: document.clone(),
                program: task.id,
                declaration,
                target,
                object,
                shore,
                priority: task.priority.unwrap_or_default(),
                score,
                delay_ticks,
            });
        });
    });
}

pub(super) fn start_selected_idle_animal_tasks(
    clock: Single<&ZooClock>,
    pending_starts: Query<(Entity, &PendingAnimalTaskStart)>,
    mut idle_animals: Query<Option<&mut BehaviorTaskGeneration>, IdleAnimalTaskFilter>,
    tokens: Query<(), With<BehaviorSelfToken>>,
    mut commands: Commands,
) {
    for (animal, pending) in &pending_starts {
        commands.entity(animal).remove::<PendingAnimalTaskStart>();
        let Ok(generation) = idle_animals.get_mut(animal) else {
            continue;
        };
        let PendingAnimalTaskStart {
            ref document,
            program,
            declaration,
            target,
            object,
            shore,
            priority,
            score,
            delay_ticks,
        } = *pending;
        let execution_id = allocate_behavior_execution_id(animal, generation, &mut commands);
        if let Some(token) = object.filter(|token| tokens.contains(*token)) {
            commands
                .entity(token)
                .insert(ClaimedBehaviorSelfToken { execution_id });
        }
        debug!(?animal, ?program, ?target, execution_id,
            cooldown_ticks = ?delay_ticks, "animal started task");
        let mut started = commands.entity(animal);
        started.insert(BehaviorTaskEffectiveScore {
            execution_id,
            template_score: Some(priority),
            linked_score: None,
            selected_candidate_score: Some(score),
        });
        if let Some(shore) = shore {
            started.insert(shore);
        } else {
            started.remove::<openzt2_zoo_state::animal_behavior::behavior_natural_water_target_types::SelectedNaturalWaterTarget>();
        }
        started.insert(BehaviorTaskExecutionState {
            execution_id,
            program,
            target: Some(target),
            document: document.clone(),
            declaration,
            phase: BehaviorTaskExecutionPhase::Execution,
            action: 0,
            repetitions: 0,
            next_action_tick: clock.tick,
            object,
            interaction_slot: None,
            depth: 0,
        });
        match delay_ticks {
            Some(delay_ticks) => started.insert(RunningDelayedAnimalTask {
                execution_id,
                task: program,
                delay_ticks,
            }),
            None => started.remove::<RunningDelayedAnimalTask>(),
        };
    }
}

/// A fence target's qualifiers: its strength, whether it is electric, how
/// far it is and that it bounds the animal's habitat.
fn fence_satisfies(
    fact: &openzt2_game_data::behavior::eligibility::BehaviorCandidateEligibilityFact,
    strength: u16,
    electric: bool,
    distance: f32,
) -> bool {
    use openzt2_game_data::behavior::eligibility::{
        BehaviorConditionEligibilityFact as Condition, BehaviorEligibilityFactInput as Input,
        BehaviorEligibilityFactValue as Value, BehaviorEntityStateEligibilityFact as State,
        BehaviorSpatialEligibilityFact as Spatial,
    };
    match (fact.fact_input, fact.expected_value) {
        (Input::Condition(Condition::FenceStrength), Value::Q16(expected)) => {
            candidate_facts::compare_q16(i32::from(strength) << 16, expected, fact.comparison)
        }
        (Input::State(State::Electric), Value::Q16(expected)) => {
            candidate_facts::compare_q16(i32::from(electric) << 16, expected, fact.comparison)
        }
        (Input::State(State::InHabitat), Value::Q16(expected)) => {
            candidate_facts::compare_q16(1 << 16, expected, fact.comparison)
        }
        (Input::Spatial(Spatial::InSight), Value::Q16(radius)) => {
            distance < radius as f32 / 65_536.0
        }
        _ => false,
    }
}

fn subject_can_consider_task(
    task: &BehaviorTask,
    facts: &AnimalCandidateFactsItem<'_, '_>,
    authored_types: &[AssetId],
    targets: &animal_targets::AnimalTargetSources,
    clock: &ZooClock,
    ticks_per_day: u32,
) -> bool {
    // Prune subject mismatches before scanning world targets, while retaining
    // all authored target checks for concrete candidate evaluation.
    candidate_requirements_are_satisfied(
        &task.candidate_eligibility_requirements,
        |role, required| {
            role != BehaviorCandidateRole::Subject || facts.matches_type(required, authored_types)
        },
        |role, fact| {
            role != BehaviorCandidateRole::Subject
                || targets
                    .spatial_satisfies(facts, fact, clock.tick_in_day, ticks_per_day)
                    .unwrap_or_else(|| {
                        facts.satisfies(fact, clock.tick, ticks_per_day, authored_types)
                    })
        },
    )
}

fn task_has_typed_subject_and_target(task: &BehaviorTask) -> bool {
    !task.targets.is_empty()
        && !task.subjects.is_empty()
        && task
            .candidate_eligibility_requirements
            .iter()
            .any(|requirements| {
                requirements.candidate_role == BehaviorCandidateRole::Subject
                    && !requirements.candidate_type_identifiers.is_empty()
            })
}

#[cfg(test)]
mod fence_target_tests {
    use super::fence_satisfies;
    use openzt2_game_data::behavior::eligibility::{
        BehaviorCandidateEligibilityFact as Fact, BehaviorConditionEligibilityFact as Condition,
        BehaviorEligibilityFactComparison as Comparison, BehaviorEligibilityFactInput as Input,
        BehaviorEligibilityFactValue as Value, BehaviorEntityStateEligibilityFact as State,
    };

    #[test]
    fn a_fence_qualifies_by_its_strength_and_whether_it_is_electric() {
        // An elephant bashes fences up to strength 8 (`f_FenceStrength="LE 8"`).
        let up_to_eight = Fact {
            fact_input: Input::Condition(Condition::FenceStrength),
            comparison: Comparison::LessOrEqual,
            expected_value: Value::Q16(8 << 16),
        };
        assert!(fence_satisfies(&up_to_eight, 1, false, 5.0));
        assert!(!fence_satisfies(&up_to_eight, 10, false, 5.0));
        // A raptor bashes only fences that aren't electric (`b_Electric="false"`).
        let not_electric = Fact {
            fact_input: Input::State(State::Electric),
            comparison: Comparison::Equal,
            expected_value: Value::Q16(0),
        };
        assert!(fence_satisfies(&not_electric, 8, false, 5.0));
        assert!(!fence_satisfies(&not_electric, 8, true, 5.0));
    }
}
