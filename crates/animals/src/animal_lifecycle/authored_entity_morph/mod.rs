use avian3d::prelude::RigidBody;
use bevy::{asset::LoadState, prelude::*};
use openzt2_game_assets::{
    scene_prefab::ScenePrefabAsset,
    species::species_asset_types::{SpeciesAsset, SpeciesAssets},
    world_definitions::{
        world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
        world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
    },
};
use openzt2_game_data::behavior::action_record::BehaviorMorphRecipient;
use openzt2_world::world_spawn::prefab_world_instance_spawning::{
    morph_fade_projection::PrefabMorphFade, spawn_loaded_scene_prefab_as_world_instance,
};
use openzt2_world_state::{
    simulation_time::{
        deterministic_random_stream::{DeterministicRng, RngDomain, ZooSeed},
        simulation_clock_types::ZooClock,
    },
    world_spawn::{
        persistent_id_types::{PersistentId, PersistentIdAllocator},
        world_membership_types::{DefinitionId, WorldMember},
    },
};
use openzt2_zoo_state::{
    animal_behavior::{
        animal_lineage::AnimalLineage, behavior_random_stream_state::BehaviorRandomStream,
        behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation,
    },
    animal_health::types::{AnimalDied, Dead, DeathCause},
    animal_lifecycle::{animal_birth_and_release_contracts::AnimalBorn, types::Animal},
    behavior_task_execution_types::{
        running_actions::{
            BehaviorInstructionId, RunBehaviorMorphEntity, RunBehaviorTermination,
            RunningBehaviorInstruction,
        },
        BehaviorTaskExecutionState, PendingBehaviorTaskFailure,
    },
    habitat::habitat_types::HabitatMember,
};

use super::{
    animal_entity_spawning::spawn_animal_entity_from_resolved_components,
    animal_spawn_resolution::resolve_species_and_variant_to_animal_spawn_components,
};

#[derive(Component)]
pub(super) struct PendingAuthoredMorph {
    step: BehaviorInstructionId,
    recipient: Entity,
    prefab: Option<Handle<ScenePrefabAsset>>,
}

#[derive(Component)]
pub(super) struct AuthoredMorphShell;

pub(super) fn morph_authored_entities(
    mut commands: Commands,
    clock: Res<ZooClock>,
    catalogue: Res<WorldDefinitions>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    (species, species_assets): (Res<SpeciesAssets>, Res<Assets<SpeciesAsset>>),
    (server, prefabs): (Res<AssetServer>, Res<Assets<ScenePrefabAsset>>),
    mut allocators: Query<&mut PersistentIdAllocator>,
    fences: Query<(), With<openzt2_zoo_state::topology::topology_graph_types::FenceEdge>>,
    seed: Option<Res<ZooSeed>>,
    mut actions: Query<
        (
            Entity,
            &BehaviorTaskExecutionState,
            &RunningBehaviorInstruction,
            &RunBehaviorMorphEntity,
            Option<&mut BehaviorRandomStream>,
            Option<&mut PendingAuthoredMorph>,
        ),
        Without<PendingBehaviorTaskFailure>,
    >,
    recipients: Query<(
        Entity,
        &DefinitionId,
        &WorldMember,
        &GlobalTransform,
        Option<&PersistentId>,
        Option<&HabitatMember>,
        Option<&Name>,
        Has<Animal>,
        Has<Dead>,
    )>,
    lineage: AnimalLineage,
    mut deaths: MessageWriter<AnimalDied>,
    mut births: MessageWriter<AnimalBorn>,
    mut visibility: Query<&mut Visibility>,
) {
    let (Some(catalogue_view), Some(species)) =
        (catalogue.get(&definitions), species.get(&species_assets))
    else {
        return;
    };
    for (actor, task, step, action, random, pending) in &mut actions {
        if task.next_action_tick > clock.tick || !step.is_current(task) {
            continue;
        }
        let recipient = match action.recipient {
            BehaviorMorphRecipient::Subject => Some(actor),
            BehaviorMorphRecipient::Target => task.target,
            BehaviorMorphRecipient::NamedChild(definition)
                if definition == openzt2_game_data::AssetId::from_key("object") =>
            {
                task.object
            }
            BehaviorMorphRecipient::NamedChild(definition) => {
                [Some(actor), task.object, task.target]
                    .into_iter()
                    .flatten()
                    .find(|entity| {
                        recipients
                            .get(*entity)
                            .is_ok_and(|(_, id, ..)| id.0 == definition)
                    })
                    .or_else(|| {
                        lineage.children(actor).into_iter().find(|child| {
                            lineage.parents(*child).mother == Some(actor)
                                && recipients
                                    .get(*child)
                                    .is_ok_and(|(_, id, ..)| id.0 == definition)
                        })
                    })
            }
        };
        let Some(recipient) = recipient else {
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            continue;
        };
        if fences.contains(recipient) {
            continue;
        }
        let Ok((_, definition, world, transform, persistent_id, habitat, name, is_animal, dead)) =
            recipients.get(recipient)
        else {
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            continue;
        };
        if !recipients
            .get(actor)
            .is_ok_and(|(_, _, actor_world, ..)| actor_world.root == world.root)
        {
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            continue;
        }
        let mut pending =
            pending.filter(|pending| pending.step == step.id() && pending.recipient == recipient);
        if !action.period.is_zero() && pending.is_none() {
            let duration = action
                .period
                .as_nanos()
                .saturating_mul(u128::from(catalogue_view.timing().fixed_hz))
                .div_ceil(1_000_000_000);
            let due = clock
                .tick
                .saturating_add(u64::try_from(duration).unwrap_or(u64::MAX));
            commands.entity(actor).insert(PendingAuthoredMorph {
                step: step.id(),
                recipient,
                prefab: None,
            });
            if action.fade {
                commands
                    .entity(recipient)
                    .insert(PrefabMorphFade::fade_out(action.period));
            }
            step.resume_at(actor, due, &mut commands);
            continue;
        }
        let position = transform.compute_transform();
        let replacement = catalogue_view
            .find_object(definition.0)
            .and_then(|object| {
                object
                    .authored_runtime
                    .morph_targets
                    .iter()
                    .find_map(|(key, value)| (*key == action.definition).then_some(*value))
            })
            .unwrap_or(action.definition);
        if let Some((species_record, variant)) = species.find_variant_with_species(replacement) {
            let mut fallback;
            let mut random = random;
            let random = if let Some(random) = random.as_deref_mut() {
                &mut random.0
            } else {
                let Some((seed, id)) = seed.as_deref().zip(persistent_id) else {
                    mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
                    continue;
                };
                fallback = DeterministicRng::from_entity(*seed, *id, RngDomain::Reproduction);
                &mut fallback
            };
            let Some(components) = resolve_species_and_variant_to_animal_spawn_components(
                species,
                species_record,
                Some(variant.id),
                None,
                variant.life_stage,
                catalogue_view.timing().ticks_per_day,
                random,
            ) else {
                mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
                continue;
            };
            let Some(id) = allocators
                .get_mut(world.root)
                .ok()
                .and_then(|mut allocator| allocator.allocate().ok())
            else {
                mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
                continue;
            };
            let in_habitat = habitat.map(|member| member.habitat_entity);
            let young = spawn_animal_entity_from_resolved_components(
                &mut commands,
                species_record.id,
                components,
                id,
                *world,
                in_habitat,
                Transform {
                    scale: Vec3::ONE,
                    ..position
                },
                lineage.parents(recipient),
                clock.tick,
            );
            if action.fade && !action.period.is_zero() {
                commands
                    .entity(young)
                    .insert(PrefabMorphFade::fade_in(action.period));
            }
            if action.name_it {
                if let Some(name) = name {
                    commands.entity(young).insert(name.clone());
                }
            }
            births.write(AnimalBorn { child: young });
        } else if let Some(object) = catalogue_view.find_object(replacement) {
            let handle = pending
                .as_ref()
                .and_then(|pending| pending.prefab.clone())
                .or_else(|| catalogue_view.scene(object.prefab));
            let Some(handle) = handle else {
                if catalogue.is_complete() {
                    mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
                }
                continue;
            };
            let Some(prefab) = prefabs.get(&handle) else {
                if matches!(
                    server.get_load_state(handle.id()),
                    Some(LoadState::Failed(_))
                ) {
                    mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
                } else if let Some(pending) = pending.as_deref_mut() {
                    pending.prefab = Some(handle);
                } else {
                    commands.entity(actor).insert(PendingAuthoredMorph {
                        step: step.id(),
                        recipient,
                        prefab: Some(handle),
                    });
                }
                continue;
            };
            let Some(id) = allocators
                .get_mut(world.root)
                .ok()
                .and_then(|mut allocator| allocator.allocate().ok())
            else {
                mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
                continue;
            };
            let replacement = spawn_loaded_scene_prefab_as_world_instance(
                &mut commands,
                prefab,
                handle,
                world.root,
                object.id,
                id,
                Transform {
                    scale: Vec3::splat(object.prefab_scale),
                    ..position
                },
                true,
                None,
                RigidBody::Kinematic,
            );
            if action.fade && !action.period.is_zero() {
                commands
                    .entity(replacement)
                    .insert(PrefabMorphFade::fade_in(action.period));
            }
            if action.name_it {
                if let Some(name) = name {
                    commands.entity(replacement).insert(name.clone());
                }
            }
            if is_animal {
                if !dead {
                    crate::animal_health::disease_mortality_execution::finalize_animal_death(
                        &mut commands,
                        &mut deaths,
                        recipient,
                        DeathCause::Authored(task.program),
                        clock.tick,
                    );
                }
            }
        } else {
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            continue;
        }
        if !is_animal {
            commands.entity(recipient).insert(AuthoredMorphShell);
        }
        if let Ok(mut visible) = visibility.get_mut(recipient) {
            visible.set_if_neq(Visibility::Hidden);
        } else {
            commands.entity(recipient).insert(Visibility::Hidden);
        }
        commands.entity(recipient).try_remove::<PrefabMorphFade>();
        commands.entity(actor).remove::<PendingAuthoredMorph>();
        step.complete(actor, clock.tick, &mut commands);
    }
}

pub(super) fn cancel_interrupted_morph_fade(
    removed: On<Remove<RunBehaviorMorphEntity>>,
    pending: Query<&PendingAuthoredMorph>,
    mut commands: Commands,
) {
    if let Ok(morph) = pending.get(removed.entity) {
        commands
            .entity(morph.recipient)
            .try_remove::<PrefabMorphFade>();
        commands
            .entity(removed.entity)
            .try_remove::<PendingAuthoredMorph>();
    }
}

pub(super) fn remove_authored_morph_shells(
    clock: Res<ZooClock>,
    actions: Query<
        (
            Entity,
            &BehaviorTaskExecutionState,
            &RunningBehaviorInstruction,
            &RunBehaviorTermination,
        ),
        (
            With<AuthoredMorphShell>,
            Without<Animal>,
            Without<PendingBehaviorTaskFailure>,
        ),
    >,
    mut commands: Commands,
) {
    for (actor, task, instruction, action) in &actions {
        if action.0.kill_subject
            && instruction.is_current(task)
            && task.next_action_tick <= clock.tick
        {
            commands.entity(actor).despawn();
        }
    }
}
