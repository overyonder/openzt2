use bevy::prelude::*;
use openzt2_game_assets::{
    behavior::behavior_asset_types::{BehaviorDocumentAsset, LoadedBehaviorDocumentCollection},
    world_definitions::{
        world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
        world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
    },
};
use openzt2_game_data::{
    behavior::eligibility::{
        BehaviorCandidateRole, BehaviorEligibilityFactInput, BehaviorEligibilityFactValue,
        BehaviorSpatialEligibilityFact, BehaviorWelfareEligibilityFact,
    },
    world_definitions::world_objects::WorldObjectEggDefinition,
    AssetId,
};
use openzt2_world_state::{
    simulation_time::{
        deterministic_random_stream::{DeterministicRng, RngDomain, ZooSeed},
        simulation_clock_types::ZooClock,
    },
    world_spawn::{
        persistent_id_types::PersistentId,
        world_membership_types::{DefinitionId, WorldMember},
    },
};
use openzt2_zoo_state::{
    animal_behavior::{
        behavior_candidate_selection::{
            candidate_requirements_are_satisfied, score_supported_behavior_task,
            WeightedBehaviorTaskRanking,
        },
        behavior_random_stream_state::BehaviorRandomStream,
        egg_incubation::EggIncubation,
    },
    behavior_task_execution_types::{
        BehaviorTaskExecutionPhase, BehaviorTaskExecutionState, BehaviorTaskGeneration,
    },
    need_adjustment::advance_need_adjustment_timer,
    terrain::dry_surface_eligibility::DrySurfaceEligibility,
};

#[derive(Component, Clone, Copy, PartialEq)]
pub(super) struct AuthoredEgg(WorldObjectEggDefinition);

#[derive(Component)]
pub(super) struct EggTasks(Vec<(Handle<BehaviorDocumentAsset>, usize)>);

pub(super) fn initialize_live_eggs(
    mut commands: Commands,
    definitions: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    mut events: MessageReader<AssetEvent<WorldDefinitionAsset>>,
    seeds: Query<&ZooSeed>,
    mut entities: Query<(
        Entity,
        Ref<DefinitionId>,
        &WorldMember,
        &PersistentId,
        Has<EggIncubation>,
        Has<BehaviorRandomStream>,
        Option<&mut AuthoredEgg>,
    )>,
) {
    let changed = definitions.is_changed() || events.read().next().is_some();
    events.clear();
    let Some(view) = definitions.get(&assets) else {
        return;
    };
    for (entity, definition, member, id, incubating, has_random, policy) in &mut entities {
        let Ok(seed) = seeds.get(member.root) else {
            continue;
        };
        if !changed && !definition.is_changed() {
            continue;
        }
        let authored = view
            .find_object(definition.0)
            .and_then(|object| object.authored_runtime.egg);
        let Some(authored) = authored else {
            if definitions.is_complete() && policy.is_some() {
                commands
                    .entity(entity)
                    .remove::<(AuthoredEgg, EggIncubation, EggTasks)>();
            }
            continue;
        };
        if let Some(mut policy) = policy {
            policy.set_if_neq(AuthoredEgg(authored));
        } else {
            commands.entity(entity).insert(AuthoredEgg(authored));
        }
        if !incubating {
            commands.entity(entity).insert(EggIncubation {
                amount: authored.initial_incubation,
                remaining_ns: 0,
                random: DeterministicRng::from_entity(*seed, *id, RngDomain::AnimalNeeds),
            });
        }
        if !has_random {
            commands
                .entity(entity)
                .insert(BehaviorRandomStream(DeterministicRng::from_entity(
                    *seed,
                    *id,
                    RngDomain::Behavior,
                )));
        }
    }
}

pub(super) fn index_egg_tasks(
    mut commands: Commands,
    collection: Res<LoadedBehaviorDocumentCollection>,
    assets: Res<Assets<BehaviorDocumentAsset>>,
    mut events: MessageReader<AssetEvent<BehaviorDocumentAsset>>,
    definitions: Res<WorldDefinitions>,
    definition_assets: Res<Assets<WorldDefinitionAsset>>,
    eggs: Query<(Entity, &DefinitionId, Ref<AuthoredEgg>, Has<EggTasks>)>,
) {
    let changed =
        collection.is_changed() || definitions.is_changed() || events.read().next().is_some();
    events.clear();
    let (Some(declarations), Some(definitions)) = (
        collection.create_declaration_index_view(&assets),
        definitions.get(&definition_assets),
    ) else {
        return;
    };
    for (egg, definition, policy, indexed) in &eggs {
        if indexed && !changed && !policy.is_changed() {
            continue;
        }
        let Some(object) = definitions.find_object(definition.0) else {
            continue;
        };
        let tasks = declarations
            .behavior_tasks()
            .filter(|(_, _, task)| {
                let matches_type = |name: &str| {
                    let id = AssetId::from_key(&name.trim().to_ascii_lowercase());
                    id == definition.0 || object.behavior_type_identifiers.contains(&id)
                };
                task.subjects.iter().any(|name| matches_type(name))
                    && (task.targets.is_empty()
                        || task
                            .targets
                            .iter()
                            .all(|target| target.eq_ignore_ascii_case("self")))
                    && task.objects.is_empty()
            })
            .map(|(document, declaration, _)| (document.clone(), declaration))
            .collect();
        commands.entity(egg).insert(EggTasks(tasks));
    }
}

pub(super) fn update_egg_incubation(
    time: Res<Time<Fixed>>,
    definitions: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    mut eggs: Query<(&AuthoredEgg, &mut EggIncubation)>,
) {
    let Some(timing) = definitions
        .get(&assets)
        .and_then(|view| view.need_adjustment())
        .filter(|timing| timing.enabled)
    else {
        return;
    };
    let step_ns = u64::try_from(time.timestep().as_nanos()).unwrap_or(u64::MAX);
    eggs.par_iter_mut().for_each(|(policy, mut incubation)| {
        let incubation = &mut *incubation;
        if advance_need_adjustment_timer(
            &mut incubation.remaining_ns,
            step_ns,
            timing.delay_steps,
            &mut incubation.random,
        ) {
            incubation.amount = (incubation.amount + policy.0.incubation_per_update).max(0.0);
        }
    });
}

pub(super) fn start_idle_egg_tasks(
    mut commands: Commands,
    clocks: Query<&ZooClock>,
    mut behavior_generations: Query<&mut BehaviorTaskGeneration>,
    assets: Res<Assets<BehaviorDocumentAsset>>,
    definitions: Res<WorldDefinitions>,
    definition_assets: Res<Assets<WorldDefinitionAsset>>,
    terrain: DrySurfaceEligibility,
    mut eggs: Query<
        (
            Entity,
            &DefinitionId,
            &WorldMember,
            &GlobalTransform,
            &EggIncubation,
            &EggTasks,
            &mut BehaviorRandomStream,
        ),
        Without<BehaviorTaskExecutionState>,
    >,
) {
    let Some(definitions) = definitions.get(&definition_assets) else {
        return;
    };
    for (egg, definition, member, pose, incubation, tasks, mut random) in &mut eggs {
        let Ok(clock) = clocks.get(member.root) else {
            continue;
        };
        let Some(object) = definitions.find_object(definition.0) else {
            continue;
        };
        let mut ranking = WeightedBehaviorTaskRanking::new(0.0);
        for (document, declaration) in &tasks.0 {
            let Some(task) = assets
                .get(document)
                .and_then(|paper| paper.behavior_task_at_index(*declaration))
            else {
                continue;
            };
            if !candidate_requirements_are_satisfied(
                &task.candidate_eligibility_requirements,
                |role, id| {
                    role != BehaviorCandidateRole::Object
                        && (id == definition.0
                            || id == AssetId::from_key("self")
                            || object.behavior_type_identifiers.contains(&id))
                },
                |role, fact| {
                    if role == BehaviorCandidateRole::Object {
                        return false;
                    }
                    let BehaviorEligibilityFactValue::Q16(expected) = fact.expected_value else {
                        return false;
                    };
                    let actual = match fact.fact_input {
                        BehaviorEligibilityFactInput::Welfare(
                            BehaviorWelfareEligibilityFact::Incubation,
                        ) => Some((incubation.amount * 65536.0).round() as i32),
                        BehaviorEligibilityFactInput::Spatial(
                            BehaviorSpatialEligibilityFact::SupportedSurface,
                        ) => Some(65536),
                        BehaviorEligibilityFactInput::Spatial(
                            BehaviorSpatialEligibilityFact::OnLand,
                        ) => terrain
                            .on_land_at_position(member.root, pose.translation())
                            .map(|on_land| i32::from(on_land) * 65536),
                        BehaviorEligibilityFactInput::Spatial(
                            BehaviorSpatialEligibilityFact::InWater,
                        ) => terrain
                            .on_land_at_position(member.root, pose.translation())
                            .map(|on_land| i32::from(!on_land) * 65536),
                        _ => None,
                    };
                    actual.is_some_and(|actual| crate::animal_behavior::autonomous_task_selection::candidate_facts::compare_q16(actual, expected, fact.comparison))
                },
            ) {
                continue;
            }
            let Some(score) =
                score_supported_behavior_task(task, 0.0, 0.0, || None, |_| None, |_| None)
            else {
                continue;
            };
            ranking.consider(
                task.id,
                (document, *declaration, task),
                task.priority.unwrap_or_default(),
                score,
                |limit| random.0.range_u32(limit),
            );
        }
        if let Some(((document, declaration, task), _)) =
            ranking.finish(|limit| random.0.range_u32(limit))
        {
            let execution_id =
                openzt2_zoo_state::behavior_task_execution_types::allocate_behavior_execution_id(
                    egg,
                    behavior_generations.get_mut(egg).ok(),
                    &mut commands,
                );
            commands.entity(egg).insert(BehaviorTaskExecutionState {
                execution_id,
                program: task.id,
                target: Some(egg),
                object: None,
                document: document.clone(),
                declaration,
                phase: BehaviorTaskExecutionPhase::Execution,
                action: 0,
                repetitions: 0,
                next_action_tick: clock.tick,
                interaction_slot: None,
                depth: 0,
            });
        }
    }
}
