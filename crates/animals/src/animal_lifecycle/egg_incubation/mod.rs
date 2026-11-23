use std::sync::Arc;

use bevy::{ecs::query::QueryData, platform::collections::HashMap, prelude::*};
use openzt2_game_assets::{
    behavior::behavior_asset_types::{BehaviorDocumentAsset, LoadedBehaviorDocumentCollection},
    world_definitions::{
        world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
        world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
    },
};
use openzt2_game_data::{
    AssetId,
    behavior::eligibility::{
        BehaviorCandidateRole, BehaviorEligibilityFactInput, BehaviorEligibilityFactValue,
        BehaviorSpatialEligibilityFact, BehaviorWelfareEligibilityFact,
    },
    world_definitions::world_objects::WorldObjectEggDefinition,
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
            BehaviorCandidateRanking, candidate_requirements_are_satisfied,
            score_supported_behavior_task,
        },
        behavior_random_stream_state::BehaviorRandomStream,
        egg_incubation::EggIncubation,
    },
    behavior_task_execution_types::{
        BehaviorTaskExecutionPhase, BehaviorTaskExecutionState, BehaviorTaskGeneration,
    },
    terrain::dry_surface_eligibility::DrySurfaceEligibility,
};

use super::egg_incubation_types::EggIncubationState;

#[derive(Component, Clone, Copy, PartialEq)]
pub(super) struct AuthoredEgg(WorldObjectEggDefinition);

type EggTaskLocations = Arc<[(Handle<BehaviorDocumentAsset>, usize)]>;

#[derive(Component)]
pub(super) struct EggTasks(EggTaskLocations);

#[derive(QueryData)]
#[query_data(mutable)]
pub(super) struct LiveEggPolicy {
    entity: Entity,
    definition: &'static DefinitionId,
    member: &'static WorldMember,
    id: &'static PersistentId,
    incubating: Option<&'static mut EggIncubationState>,
    restored: Option<&'static EggIncubation>,
    has_random: Has<BehaviorRandomStream>,
    policy: Option<&'static mut AuthoredEgg>,
}

pub(super) fn initialize_live_eggs(
    mut commands: Commands,
    definitions: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    mut events: MessageReader<AssetEvent<WorldDefinitionAsset>>,
    seeds: Query<&ZooSeed>,
    mut entities: ParamSet<(
        Query<LiveEggPolicy>,
        Query<
            LiveEggPolicy,
            Or<(
                Changed<DefinitionId>,
                Changed<WorldMember>,
                Added<PersistentId>,
                Added<EggIncubation>,
            )>,
        >,
    )>,
) {
    let changed = definitions.is_changed() || events.read().any(changed_definition_asset);
    events.clear();
    let Some(view) = definitions.get(&assets) else {
        return;
    };
    let mut apply = |egg: LiveEggPolicyItem<'_, '_>| {
        let Ok(seed) = seeds.get(egg.member.root) else {
            return;
        };
        let mut commands = commands.entity(egg.entity);
        let Some(authored) = view
            .find_object(egg.definition.0)
            .and_then(|object| object.authored_runtime.egg)
        else {
            if definitions.is_complete() && egg.policy.is_some() {
                commands.remove::<(AuthoredEgg, EggIncubationState, EggTasks)>();
            }
            return;
        };
        if let Some(mut policy) = egg.policy {
            policy.set_if_neq(AuthoredEgg(authored));
        } else {
            commands.insert(AuthoredEgg(authored));
        }
        match (egg.incubating, egg.restored) {
            (Some(mut incubation), Some(restored)) => {
                *incubation = EggIncubationState::from_saved_remaining(
                    restored.amount,
                    restored.remaining_ns,
                    restored.random,
                );
                commands.remove::<EggIncubation>();
            }
            (None, Some(restored)) => {
                commands.insert(EggIncubationState::from_saved_remaining(
                    restored.amount,
                    restored.remaining_ns,
                    restored.random,
                ));
                commands.remove::<EggIncubation>();
            }
            (None, None) => {
                commands.insert(EggIncubationState::from_saved_remaining(
                    authored.initial_incubation,
                    0,
                    DeterministicRng::from_entity(*seed, *egg.id, RngDomain::AnimalNeeds),
                ));
            }
            (Some(_), None) => {}
        }
        if !egg.has_random {
            commands.insert(BehaviorRandomStream(DeterministicRng::from_entity(
                *seed,
                *egg.id,
                RngDomain::Behavior,
            )));
        }
    };
    if changed {
        entities.p0().iter_mut().for_each(&mut apply);
    } else {
        entities.p1().iter_mut().for_each(apply);
    }
}

fn changed_definition_asset<A: Asset>(event: &AssetEvent<A>) -> bool {
    matches!(
        event,
        AssetEvent::Added { .. } | AssetEvent::Modified { .. } | AssetEvent::Removed { .. }
    )
}

type EggTaskProjection = (Entity, &'static DefinitionId, Option<&'static mut EggTasks>);

pub(super) fn index_egg_tasks(
    mut commands: Commands,
    collection: Res<LoadedBehaviorDocumentCollection>,
    assets: Res<Assets<BehaviorDocumentAsset>>,
    mut events: MessageReader<AssetEvent<BehaviorDocumentAsset>>,
    definitions: Res<WorldDefinitions>,
    definition_assets: Res<Assets<WorldDefinitionAsset>>,
    mut definition_events: MessageReader<AssetEvent<WorldDefinitionAsset>>,
    mut index: Local<HashMap<AssetId, EggTaskLocations>>,
    mut eggs: ParamSet<(
        Query<EggTaskProjection, With<AuthoredEgg>>,
        Query<
            EggTaskProjection,
            (
                With<AuthoredEgg>,
                Or<(Changed<DefinitionId>, Added<AuthoredEgg>, Without<EggTasks>)>,
            ),
        >,
    )>,
) {
    let changed = collection.is_changed()
        || definitions.is_changed()
        || events.read().any(changed_definition_asset)
        || definition_events.read().any(changed_definition_asset);
    events.clear();
    definition_events.clear();
    if changed {
        index.clear();
    }
    let (Some(declarations), Some(definitions)) = (
        collection.create_declaration_index_view(&assets),
        definitions.get(&definition_assets),
    ) else {
        return;
    };
    let mut apply = |(egg, definition, current): (Entity, &DefinitionId, Option<Mut<EggTasks>>)| {
        let Some(object) = definitions.find_object(definition.0) else {
            return;
        };
        let tasks = index.entry(definition.0).or_insert_with(|| {
            declarations
                .behavior_tasks()
                .filter(|(_, _, task)| {
                    let matches_type = |name: &str| {
                        let id = AssetId::from_key(&name.trim().to_ascii_lowercase());
                        id == definition.0 || object.behavior_type_identifiers.contains(&id)
                    };
                    task.subjects.iter().any(|name| matches_type(name))
                        && task
                            .targets
                            .iter()
                            .all(|target| target.eq_ignore_ascii_case("self"))
                        && task.objects.is_empty()
                })
                .map(|(document, declaration, _)| (document.clone(), declaration))
                .collect::<EggTaskLocations>()
        });
        if let Some(mut current) = current {
            if !Arc::ptr_eq(&current.0, tasks) {
                current.0 = Arc::clone(tasks);
            }
        } else {
            commands.entity(egg).insert(EggTasks(Arc::clone(tasks)));
        }
    };
    if changed {
        eggs.p0().iter_mut().for_each(&mut apply);
    } else {
        eggs.p1().iter_mut().for_each(apply);
    }
}

pub(super) fn update_egg_incubation(
    time: Res<Time<Fixed>>,
    definitions: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    mut eggs: Query<(&AuthoredEgg, &mut EggIncubationState)>,
) {
    let Some(timing) = definitions
        .get(&assets)
        .and_then(|view| view.need_adjustment())
        .filter(|timing| timing.enabled)
    else {
        return;
    };
    let step = time.timestep();
    eggs.par_iter_mut().for_each(|(policy, mut incubation)| {
        if incubation.cadence.tick(step, timing.delay_steps) {
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
            &EggIncubationState,
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
        let mut ranking = BehaviorCandidateRanking::new(0.0);
        for (document, declaration) in tasks.0.iter() {
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
                (document, *declaration, task),
                task.priority.unwrap_or_default(),
                score,
            );
        }
        if let Some(((document, declaration, task), _)) = ranking.finish(|| random.next_u32()) {
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
