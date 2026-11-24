use bevy::{ecs::system::SystemParam, prelude::*};
use openzt2_game_assets::{
    behavior::behavior_asset_types::{BehaviorDocumentAsset, LoadedBehaviorDocumentCollection},
    world_definitions::{
        world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
        world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
    },
};
use openzt2_game_data::AssetId;
use openzt2_world_state::{
    simulation_time::simulation_clock_types::ZooClock,
    world_spawn::world_membership_types::{DefinitionId, WorldMember},
};
use openzt2_zoo_state::{
    animal_behavior::{
        behavior_random_stream_state::BehaviorRandomStream,
        behavior_token_types::BehaviorSelfToken,
        interaction_container_membership::{
            InteractionContainerMember, InteractionContainerMemberships,
        },
    },
    animal_health::types::Dead,
    animal_lifecycle::types::SpeciesHandle,
    behavior_task_execution_types::{
        BehaviorTaskExecutionState, BehaviorTaskReturnFrame, PendingBehaviorTaskFailure,
        running_actions::{
            BehaviorStepFixedSet, BehaviorTaskReplaced, RunBehaviorStaffTrainAnimal,
            RunningBehaviorInstruction, SelectedBehaviorSet, TrainerEnabledTricks,
            activated_behavior_actor, behavior_action_actors, update_waiting_behavior_actors,
        },
    },
    shows::performer_training::PerformerTraining,
    staff::{
        staff_assignment_types::TrainerAssignmentSelection, staff_employment_types::StaffRole,
    },
};

use super::{
    behavior_subject_type_resolution::behavior_subject_type_identifiers,
    behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation,
};

#[derive(Clone, Copy)]
enum TrainingPhase {
    Prepare,
    Summon,
    WaitForAnimal { token: Entity, until: u64 },
    Prompt,
    WaitForTrick { token: Entity, until: u64 },
    Reward,
    Cleanup,
}

#[derive(Component, Clone)]
struct AnimalTrainingSession {
    execution: u64,
    origin: BehaviorTaskReturnFrame,
    animal: Entity,
    platform: Option<Entity>,
    prop: Option<Entity>,
    trick: AssetId,
    phase: TrainingPhase,
}

pub(super) fn register(app: &mut App) {
    app.add_observer(activated_behavior_actor::<RunBehaviorStaffTrainAnimal>.pipe(train_animal))
        .add_observer(clear_training_session)
        .add_observer(clear_replaced_training_session)
        .add_systems(
            FixedUpdate,
            update_waiting_behavior_actors
                .pipe(train_animal)
                .in_set(BehaviorStepFixedSet::Think),
        );
}

fn train_animal(
    activation: In<Option<Entity>>,
    clock: Single<&ZooClock>,
    definitions: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    documents: Res<LoadedBehaviorDocumentCollection>,
    behaviors: Res<Assets<BehaviorDocumentAsset>>,
    mut trainers: Query<
        (
            Entity,
            &mut BehaviorTaskExecutionState,
            &RunningBehaviorInstruction,
            &RunBehaviorStaffTrainAnimal,
            &WorldMember,
            &GlobalTransform,
            &StaffRole,
            Option<&TrainerAssignmentSelection>,
            Option<&mut AnimalTrainingSession>,
            &mut BehaviorRandomStream,
        ),
        Without<PendingBehaviorTaskFailure>,
    >,
    animals: Query<(&SpeciesHandle, &WorldMember, &GlobalTransform), Without<Dead>>,
    mut proficiency: Query<&mut PerformerTraining>,
    platforms: Query<
        (Entity, &DefinitionId, &WorldMember, &GlobalTransform),
        Without<SpeciesHandle>,
    >,
    members: InteractionContainerMemberships,
    tokens: Query<&BehaviorSelfToken>,
    mut commands: Commands,
    enabled: Query<&TrainerEnabledTricks>,
    mut prop_placement: TrainingPropPlacement,
) {
    let (Some(view), Some(declarations)) = (
        definitions.get(&assets),
        documents.create_declaration_index_view(&behaviors),
    ) else {
        return;
    };
    for (actor, mut task, step, action, world, pose, staff, assignment, state, mut random) in
        behavior_action_actors(&mut trainers, activation.0)
    {
        if activation.0.is_none() && !step.can_update_after_activation(clock.tick) {
            continue;
        }
        let action = &action.0;
        let current = task.create_return_frame_after_action(task.action);
        let mut state = state.filter(|state| {
            state.execution == task.execution_id && state.origin.same_instruction(&current)
        });
        let animal = state.as_deref().map(|state| state.animal).or(task.target);
        let Some((animal, species, _)) = animal.and_then(|animal| {
            let (species, member, pose) = animals.get(animal).ok()?;
            (member.root == world.root).then_some((animal, species, pose))
        }) else {
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            continue;
        };
        let enabled = enabled.get(actor).ok();
        let eligible_trick = |trick: &openzt2_game_data::world_definitions::animal_shows_and_training::TrickDefinition| {
            trick.species.contains(&species.species) && trick.levels.iter().any(|level| level.staff_can_train)
                && enabled.is_none_or(|enabled| enabled.0.contains(&trick.id))
                && trick.prerequisite.is_none_or(|prerequisite| {
                    proficiency.get(animal).ok().is_some_and(|training| {
                        training.proficiency(prerequisite.trick) >= prerequisite.minimum_score
                    })
                })
        };
        let Some(trick) = state
            .as_deref()
            .and_then(|state| view.find_trick(state.trick))
            .filter(|trick| eligible_trick(trick))
            .or_else(|| {
                let selected = assignment.and_then(|assignment| assignment.0);
                view.tricks()
                    .filter(|trick| {
                        eligible_trick(trick)
                            && (enabled.is_some() || selected == Some(trick.display_order))
                    })
                    .min_by_key(|trick| {
                        (selected != Some(trick.display_order), trick.display_order)
                    })
            })
        else {
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            continue;
        };
        let seconds_to_ticks =
            |seconds: f32| (f64::from(seconds) * f64::from(view.timing().fixed_hz)).ceil() as u64;
        let role = behavior_subject_type_identifiers(None, Some(staff), None, view, &[]);
        let phase = state.as_deref().map(|state| state.phase);
        let mut set_target = None;
        let next = match phase {
            None => {
                if action.go_to_platform.is_some() || action.require_platform {
                    let kind = AssetId::from_key("tankplatform");
                    let slot_tag = AssetId::from_key("trainer");
                    let platform = platforms
                        .iter()
                        .filter(|(platform, definition, member, _)| {
                            member.root == world.root
                                && view.find_object(definition.0).is_some_and(|object| {
                                    object.behavior_type_identifiers.contains(&kind)
                                        && object.interaction_slots.iter().enumerate().any(
                                            |(slot, definition)| {
                                                definition.reservation_tag == slot_tag
                                                    && (members.actor_slot(*platform, actor)
                                                        == Some(slot)
                                                        || members.occupied_places(*platform, slot)
                                                            < usize::from(definition.capacity))
                                            },
                                        )
                                })
                        })
                        .min_by(|a, b| {
                            a.3.translation()
                                .distance_squared(pose.translation())
                                .total_cmp(&b.3.translation().distance_squared(pose.translation()))
                                .then_with(|| a.0.cmp(&b.0))
                        });
                    let Some((platform, _, _, _)) = platform else {
                        mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
                        continue;
                    };
                    set_target = Some(Some(platform));
                }
                (action.go_to_platform, TrainingPhase::Prepare)
            }
            Some(TrainingPhase::Prepare) => {
                let Some(session) = state.as_deref_mut() else {
                    continue;
                };
                if session.prop.is_none() && trick.target != AssetId::default() {
                    match prop_placement.spawn(
                        view,
                        trick.target,
                        actor,
                        animal,
                        world.root,
                        pose,
                        &mut commands,
                    ) {
                        TrainingProp::Ready(prop) => session.prop = Some(prop),
                        TrainingProp::Loading => continue,
                        TrainingProp::Unavailable => {
                            mark_behavior_task_for_failure_and_stop_navigation(
                                actor,
                                &mut commands,
                            );
                            continue;
                        }
                    }
                }
                if task.target != Some(animal) {
                    task.target = Some(animal);
                    task.interaction_slot = None;
                }
                (action.prepare, TrainingPhase::Summon)
            }
            Some(TrainingPhase::Summon) => {
                let until = clock
                    .tick
                    .saturating_add(seconds_to_ticks(action.summon_timeout));
                let token = issue_training_token(
                    animal,
                    actor,
                    AssetId::from_key("t_stsummon"),
                    clock.tick,
                    until,
                    &mut commands,
                );
                (action.summon, TrainingPhase::WaitForAnimal { token, until })
            }
            Some(TrainingPhase::WaitForAnimal { token, until }) => {
                if clock.tick >= until {
                    commands.entity(token).try_despawn();
                    mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
                    continue;
                }
                if !tokens.contains(token) {
                    (action.train, TrainingPhase::Prompt)
                } else {
                    (
                        action.await_animal,
                        TrainingPhase::WaitForAnimal { token, until },
                    )
                }
            }
            Some(TrainingPhase::Prompt) => {
                let Some(outcomes) = view.find_trick_outcome_tokens(trick.id) else {
                    mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
                    continue;
                };
                let until = clock
                    .tick
                    .saturating_add(seconds_to_ticks(action.trick_timeout));
                let token = issue_training_token(
                    animal,
                    state
                        .as_deref()
                        .and_then(|state| state.prop)
                        .unwrap_or(actor),
                    outcomes.success,
                    clock.tick,
                    until,
                    &mut commands,
                );
                (
                    action.perform_trick,
                    TrainingPhase::WaitForTrick { token, until },
                )
            }
            Some(TrainingPhase::WaitForTrick { token, until }) => {
                if clock.tick >= until {
                    commands.entity(token).try_despawn();
                    mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
                    continue;
                }
                if tokens.contains(token) {
                    (
                        action.await_trick,
                        TrainingPhase::WaitForTrick { token, until },
                    )
                } else {
                    let current = proficiency
                        .get(animal)
                        .map_or(0, |proficiency| proficiency.proficiency(trick.id));
                    let chance_percent = 75.0 - f32::from(current) * 0.25;
                    if random.0.unit_f32() * 100.0 <= chance_percent {
                        let amount =
                            action.increment.round().clamp(0.0, f32::from(u16::MAX)) as u16;
                        let cap = trick
                            .levels
                            .iter()
                            .filter(|level| level.staff_can_train)
                            .map(|level| level.minimum_score)
                            .max()
                            .unwrap_or_default();
                        if let Ok(mut proficiency) = proficiency.get_mut(animal) {
                            proficiency.apply_capped_learning(trick.id, amount, cap);
                        } else {
                            let mut proficiency = PerformerTraining::default();
                            proficiency.apply_capped_learning(trick.id, amount, cap);
                            commands.entity(animal).insert(proficiency);
                        }
                    }
                    (action.reward, TrainingPhase::Reward)
                }
            }
            Some(TrainingPhase::Reward) => (action.cleanup, TrainingPhase::Cleanup),
            Some(TrainingPhase::Cleanup) => {
                if let Some(next) = enabled.and_then(|_| {
                    view.tricks()
                        .filter(|candidate| {
                            candidate.display_order > trick.display_order
                                && eligible_trick(candidate)
                        })
                        .min_by_key(|candidate| candidate.display_order)
                }) {
                    if let Some(state) = state.as_deref_mut() {
                        if let Some(prop) = state.prop.take() {
                            commands.entity(prop).try_despawn();
                        }
                        state.trick = next.id;
                        state.phase = TrainingPhase::Prepare;
                        continue;
                    }
                }
                if let Some(state) = state.as_deref() {
                    cancel_training_session(actor, state, &members, &mut commands);
                }
                step.complete(actor, clock.tick, &mut commands);
                continue;
            }
        };
        if let Some(state) = state.as_deref_mut() {
            state.phase = next.1;
        } else {
            commands.entity(actor).insert(AnimalTrainingSession {
                execution: task.execution_id,
                origin: current,
                animal,
                platform: set_target.flatten(),
                prop: None,
                trick: trick.id,
                phase: next.1,
            });
        }
        if let Some(program) = next.0 {
            let Some((document, declaration)) =
                declarations.find_behavior_set_location(program, role)
            else {
                mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
                continue;
            };
            step.enter_set(
                actor,
                SelectedBehaviorSet {
                    program,
                    document,
                    declaration,
                    tick: clock.tick,
                    repeat: Some(task.repetitions),
                    target: set_target,
                },
                &mut commands,
            );
        }
    }
}

fn issue_training_token(
    animal: Entity,
    trainer: Entity,
    name: AssetId,
    tick: u64,
    until: u64,
    commands: &mut Commands,
) -> Entity {
    commands
        .spawn((
            BehaviorSelfToken {
                name,
                available_at: tick,
                expires_at: until,
                payload: Some(trainer),
                retain_on_failure: true,
            },
            ChildOf(animal),
        ))
        .id()
}

fn clear_training_session(
    removed: On<Remove<BehaviorTaskExecutionState>>,
    states: Query<&AnimalTrainingSession>,
    memberships: InteractionContainerMemberships,
    mut commands: Commands,
) {
    if let Ok(state) = states.get(removed.entity) {
        cancel_training_session(removed.entity, state, &memberships, &mut commands);
    }
}

fn clear_replaced_training_session(
    replaced: On<BehaviorTaskReplaced>,
    states: Query<&AnimalTrainingSession>,
    memberships: InteractionContainerMemberships,
    mut commands: Commands,
) {
    if let Ok(state) = states.get(replaced.actor) {
        if state.execution == replaced.previous_execution {
            cancel_training_session(replaced.actor, state, &memberships, &mut commands);
        }
    }
}

fn cancel_training_session(
    actor: Entity,
    state: &AnimalTrainingSession,
    memberships: &InteractionContainerMemberships,
    commands: &mut Commands,
) {
    if let Some(prop) = state.prop {
        commands.entity(prop).try_despawn();
    }
    if let TrainingPhase::WaitForTrick { token, .. } | TrainingPhase::WaitForAnimal { token, .. } =
        state.phase
    {
        commands.entity(token).try_despawn();
    }
    let mut actor_commands = commands.entity(actor);
    actor_commands.try_remove::<AnimalTrainingSession>();
    if state
        .platform
        .is_some_and(|platform| memberships.actor_slot(platform, actor).is_some())
    {
        actor_commands.try_remove::<InteractionContainerMember>();
    }
}

enum TrainingProp {
    Ready(Entity),
    Loading,
    Unavailable,
}

#[derive(SystemParam)]
struct TrainingPropPlacement<'w, 's> {
    prefabs: Res<'w, Assets<openzt2_game_assets::scene_prefab::ScenePrefabAsset>>,
    models: Res<'w, Assets<bevy::gltf::Gltf>>,
    ids: Query<
        'w,
        's,
        &'static mut openzt2_world_state::world_spawn::persistent_id_types::PersistentIdAllocator,
    >,
    habitats: Query<'w, 's, &'static openzt2_zoo_state::habitat::habitat_types::HabitatMember>,
    tanks: Query<
        'w,
        's,
        (
            &'static openzt2_zoo_state::habitat::habitat_types::HabitatRegion,
            &'static openzt2_zoo_state::aquatic::aquatic_simulation_types::TankGeometry,
        ),
    >,
    grids: Query<'w, 's, &'static openzt2_zoo_state::topology::topology_graph_types::TopologyGrid>,
    terrain: openzt2_zoo_state::terrain::dry_surface_eligibility::DrySurfaceEligibility<'w, 's>,
    occupied: openzt2_zoo_state::placement::placement_occupancy_index::PlacedObjectFootprintQueries<
        'w,
        's,
    >,
}

impl TrainingPropPlacement<'_, '_> {
    #[allow(clippy::too_many_arguments)]
    fn spawn(
        &mut self,
        view: openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView<'_>,
        target: AssetId,
        trainer: Entity,
        animal: Entity,
        root: Entity,
        relative_to: &GlobalTransform,
        commands: &mut Commands,
    ) -> TrainingProp {
        use openzt2_world::world_spawn::prefab_world_instance_spawning::spawn_loaded_scene_prefab_as_world_instance;
        use openzt2_zoo_state::animal_behavior::behavior_token_types::BehaviorTrickPropOrigin;
        let Some(object) = view.find_object(target).or_else(|| {
            view.objects()
                .find(|object| object.behavior_type_identifiers.contains(&target))
        }) else {
            return TrainingProp::Unavailable;
        };
        let Some(handle) = view.scene(object.prefab) else {
            return if view.has_pending_assets() {
                TrainingProp::Loading
            } else {
                TrainingProp::Unavailable
            };
        };
        let Some(prefab) = self.prefabs.get(&handle) else {
            return TrainingProp::Loading;
        };
        if openzt2_world_state::world_spawn::prefab_model_readiness::first_missing_prefab_collider_model_asset_id(prefab, &self.models).is_some() {
            return TrainingProp::Loading;
        }
        let Ok(grid) = self.grids.get(root) else {
            return TrainingProp::Unavailable;
        };
        let habitat = self.habitats.get(animal).ok();
        let tank = habitat.and_then(|habitat| self.tanks.get(habitat.habitat_entity).ok());
        let distance = object
            .authored_runtime
            .numeric_behavior_properties
            .iter()
            .find_map(|(id, value)| (*id == AssetId::from_key("f_propdistance")).then_some(*value))
            .unwrap_or(grid.spacing_m);
        let preferred = relative_to.translation() + relative_to.forward().as_vec3() * distance;
        let clear = |point: Vec2| {
            self.occupied
                .entities_occupying_cell(
                    root,
                    grid.position_cell(Vec3::new(point.x, 0.0, point.y))
                        .truncate(),
                )
                .all(|placed| placed == trainer || placed == animal)
        };
        let projected = if let Some((region, tank)) = tank {
            let placement = view.find_placeable(object.id);
            let depth = tank.depth();
            if placement.is_some_and(|placement| {
                let surface = placement.surface_policy;
                (surface.minimum_depth_metres >= 0.0 && depth < surface.minimum_depth_metres)
                    || (surface.maximum_depth_metres >= 0.0 && depth > surface.maximum_depth_metres)
            }) {
                return TrainingProp::Unavailable;
            }
            let height = object.terrain_fit_water_offset_m.map_or(
                tank.floor_height + object.terrain_fit_height_offset_m,
                |height| tank.water_height + height,
            );
            region
                .topology_cells
                .iter()
                .map(|cell| grid.origin + (cell.as_vec2() + Vec2::splat(0.5)) * grid.spacing_m)
                .filter(|point| clear(*point))
                .min_by(|left, right| {
                    left.distance_squared(preferred.xz())
                        .total_cmp(&right.distance_squared(preferred.xz()))
                })
                .map(|point| Vec3::new(point.x, height, point.y))
        } else {
            let point = preferred.xz();
            self.terrain
                .ground_height(root, point)
                .filter(|_| clear(point))
                .map(|height| {
                    Vec3::new(
                        point.x,
                        height + object.terrain_fit_height_offset_m,
                        point.y,
                    )
                })
        };
        let Some(position) = projected else {
            return TrainingProp::Unavailable;
        };
        let Ok(mut ids) = self.ids.get_mut(root) else {
            return TrainingProp::Unavailable;
        };
        let Ok(id) = ids.allocate() else {
            return TrainingProp::Unavailable;
        };
        let yaw = relative_to.rotation().to_euler(EulerRot::YXZ).0;
        let pose = Transform::from_translation(position)
            .with_rotation(Quat::from_rotation_y(
                yaw + object.authored_runtime.prop_rotation_offset_radians,
            ))
            .with_scale(Vec3::splat(object.prefab_scale));
        let prop = spawn_loaded_scene_prefab_as_world_instance(
            commands,
            prefab,
            handle,
            root,
            object.id,
            id,
            pose,
            true,
            None,
            avian3d::prelude::RigidBody::Static,
        );
        commands
            .entity(prop)
            .insert(BehaviorTrickPropOrigin(position));
        if let Some(habitat) = habitat {
            commands.entity(prop).insert(*habitat);
        }
        TrainingProp::Ready(prop)
    }
}
