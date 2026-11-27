use bevy::prelude::*;
use openzt2_game_data::world_definitions::staff_management::StaffRoleKind;
use openzt2_zoo_state::animal_behavior::behavior_target_placement_types::BehaviorPlacementTravelTargets;
use openzt2_zoo_state::behavior_task_execution_types::running_actions::behavior_action_actors;
use openzt2_zoo_state::behavior_task_execution_types::running_actions::{
    RunBehaviorMove, RunningBehaviorInstruction,
};

use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionState;
use openzt2_zoo_state::behavior_task_execution_types::PendingBehaviorMoveCompletion;
use openzt2_zoo_state::behavior_task_execution_types::PendingBehaviorTaskFailure;
use openzt2_zoo_state::locomotion::locomotion_types::Arrived;
use openzt2_zoo_state::locomotion::locomotion_types::NavAgent;
use openzt2_zoo_state::locomotion::locomotion_types::NavigateTo;
use openzt2_zoo_state::locomotion::locomotion_types::NavigationFailed;
use openzt2_zoo_state::locomotion::locomotion_types::NavigationRequestGeneration;
use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_zoo_state::staff::staff_employment_types::StaffRole;

use super::behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::habitat::{
    habitat_membership_and_containment::locate_habitat_at_world_position,
    habitat_types::{HabitatCellOf, HabitatIndex, HabitatMember, HabitatRegion},
};
use openzt2_zoo_state::locomotion::locomotion_types::{
    ActiveTerrainDerivedNavigationGraph, NavFlags,
};
use openzt2_zoo_state::topology::topology_graph_types::TopologyGrid;

#[derive(bevy::ecs::system::SystemParam)]
pub(super) struct KeeperLandApproach<'w, 's> {
    world_navigation: Query<
        'w,
        's,
        (
            &'static ActiveTerrainDerivedNavigationGraph,
            &'static HabitatIndex,
            &'static TopologyGrid,
        ),
    >,
    regions: Query<'w, 's, &'static HabitatRegion>,
    cell_owners: Query<'w, 's, &'static HabitatCellOf>,
    animals: Query<
        'w,
        's,
        Option<&'static HabitatMember>,
        With<openzt2_zoo_state::animal_lifecycle::types::Animal>,
    >,
    water: Query<
        'w,
        's,
        &'static openzt2_zoo_state::aquatic::tank_navigation_water::TankNavigationWater,
    >,
    worlds: Query<'w, 's, &'static WorldMember>,
    candidates: Local<'s, Vec<Vec3>>,
}

pub(super) fn start_current_supported_behavior_move_actions(
    activation: In<Option<Entity>>,
    mut commands: Commands,
    clocks: Query<&ZooClock>,
    mut navigation_generations: Query<&mut NavigationRequestGeneration>,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    mut behavior_tasks: Query<
        (
            Entity,
            &WorldMember,
            (
                &mut BehaviorTaskExecutionState,
                &RunningBehaviorInstruction,
                &RunBehaviorMove,
            ),
            (
                Option<&StaffRole>,
                Option<&openzt2_zoo_state::guests::guest_simulation_types::GuestArchetype>,
            ),
            Option<&NavAgent>,
            Has<super::behavior_set_start_request_types::AmbientBehaviorSubject>,
        ),
        (
            Without<PendingBehaviorMoveCompletion>,
            Without<PendingBehaviorTaskFailure>,
        ),
    >,
    entity_global_transforms: Query<&GlobalTransform>,
    travel_sites: BehaviorPlacementTravelTargets,
    authored_attachment_transforms: super::behavior_named_nodes::NamedNodeTransforms,
    mut navigation_requests: MessageWriter<NavigateTo>,
    mut land_approach: KeeperLandApproach,
    animal_actors: Query<(), With<openzt2_zoo_state::animal_lifecycle::types::Animal>>,
    (water_targets, waters, terrain, mut poses, mut swim_layers): (
        Query<&openzt2_zoo_state::animal_behavior::behavior_natural_water_target_types::SelectedNaturalWaterTarget>,
        super::natural_water_targets::NaturalWaterTargets,
        openzt2_zoo_state::terrain::dry_surface_eligibility::DrySurfaceEligibility,
        Query<&mut Transform>,
        Query<&mut openzt2_zoo_state::locomotion::locomotion_types::SwimLayer>,
    ),
    (programs, continuations, guests, entrances, grids, token_points): (
        Res<Assets<openzt2_game_assets::behavior::behavior_asset_types::BehaviorDocumentAsset>>,
        openzt2_zoo_state::behavior_task_execution_types::continuations::BehaviorContinuationFrames,
        Query<(), With<openzt2_zoo_state::guests::guest_simulation_types::Guest>>,
        Query<&openzt2_world_state::world_spawn::zoo_entrance_anchor_synchronization::ZooEntrance>,
        Query<&openzt2_zoo_state::locomotion::locomotion_types::SpatialGrid>,
        Query<(
            &openzt2_zoo_state::animal_behavior::behavior_token_types::BehaviorTokenContext,
            &openzt2_zoo_state::animal_behavior::behavior_token_types::ClaimedBehaviorSelfToken,
            &ChildOf,
        )>,
    ),
) {
    for (
        actor,
        member,
        (mut behavior_task, instruction, running_action),
        (staff_role, guest_archetype),
        navigation_agent,
        ambient,
    ) in behavior_action_actors(&mut behavior_tasks, activation.0)
    {
        let Ok(simulation_clock) = clocks.get(member.root) else {
            continue;
        };
        if activation.0.is_none() && !instruction.can_update_after_activation(simulation_clock.tick)
        {
            continue;
        }
        if behavior_task.next_action_tick > simulation_clock.tick {
            continue;
        }
        if !instruction.is_current(&behavior_task) {
            continue;
        }
        let move_action = &running_action.0;
        let definitions = active_world_definitions.get(&world_definition_assets);
        if !ambient
            && !animal_actors.contains(actor)
            && !person_has_authored_ground_locomotion(
                move_action.locomotion_speed,
                staff_role,
                guest_archetype,
                definitions,
            )
        {
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            continue;
        }
        let placement = travel_sites.delivery_point(actor, &behavior_task);
        let token_point = behavior_task
            .object
            .and_then(|object| token_points.get(object).ok())
            .filter(|(_, claim, recipient)| {
                claim.execution_id == behavior_task.execution_id && recipient.parent() == actor
            })
            .and_then(|(context, _, _)| context.point);
        let target = placement.or(behavior_task.target());
        let target_transform = target.and_then(|target| entity_global_transforms.get(target).ok());
        let point_from_token = placement.is_none() && token_point.is_some();
        let destination = if let Some(point) = token_point.filter(|_| point_from_token) {
            Some(point)
        } else {
            target
                .zip(target_transform)
                .and_then(|(target, transform)| {
                    let named_node_position = move_action.target_node_name.as_deref().map(|name| {
                        authored_attachment_transforms
                            .pose(target, name)
                            .map(|pose| pose.translation())
                    });
                    resolve_behavior_move_destination(transform.translation(), named_node_position)
                        .ok()
                })
        };
        let Some(mut destination) = destination else {
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            continue;
        };
        if let Ok(water) = water_targets.get(actor) {
            if !point_from_token && Some(water.region) == target
                && matches!(
                    water.kind,
                    openzt2_zoo_state::animal_behavior::behavior_natural_water_target_types::NaturalWaterTargetKind::Surface { .. }
                )
            {
                if !waters.valid(member.root, water) || move_action.target_node_name.is_some() {
                    mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
                    continue;
                }
                destination = water.water;
            }
        }
        if let Some(target) = target.filter(|target| {
            !point_from_token
                && move_action.target_node_name.is_none()
                && staff_role
                    .and_then(|role| definitions.and_then(|view| view.find_staff(role.0)))
                    .is_some_and(|staff| staff.role == StaffRoleKind::Keeper)
                && land_approach.animals.contains(*target)
        }) {
            let approach = &mut land_approach;
            let Ok((graph, habitats, grid)) = approach.world_navigation.get(member.root) else {
                continue;
            };
            let tank_water = approach.water.get(member.root).ok();
            let navigation = graph.terrain_derived_navigation_graph();
            let size = f32::from(navigation.terrain_cell_size_centimetres) * 0.01;
            let target_node = (destination.x >= 0.0 && destination.z <= 0.0 && size > 0.0)
                .then(|| {
                    [
                        (destination.x / size).floor() as u32,
                        (-destination.z / size).floor() as u32,
                    ]
                })
                .and_then(|cell| navigation.calculate_navigation_node_for_terrain_cell(cell))
                .and_then(|index| navigation.effective_navigation_node(index, tank_water));
            let Some(target_node) = target_node else {
                continue;
            };
            if target_node.eligibility_flags.contains(NavFlags::WATER) {
                let same_world = approach
                    .worlds
                    .get(target)
                    .is_ok_and(|target| member.root == target.root);
                let Some(habitat) = approach
                    .animals
                    .get(target)
                    .ok()
                    .flatten()
                    .map(|member| member.habitat_entity)
                else {
                    mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
                    continue;
                };
                let Some(region) = approach.regions.get(habitat).ok().filter(|_| same_world) else {
                    mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
                    continue;
                };
                let Some(agent) = navigation_agent else {
                    continue;
                };
                graph.collect_dry_ground_candidates(
                    tank_water,
                    region,
                    *grid,
                    agent,
                    destination,
                    &mut approach.candidates,
                );
                let Some(shore) = approach.candidates.iter().copied().find(|position| {
                    locate_habitat_at_world_position(
                        habitats,
                        &approach.cell_owners,
                        position.xz(),
                        *grid,
                    ) == Some(habitat)
                }) else {
                    mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
                    continue;
                };
                destination = shore;
            }
        }
        if move_action.depth_below_surface_m.is_some() || move_action.height_above_floor_m.is_some()
        {
            let point = destination.xz();
            if let (Some(surface), Some(floor)) = (
                terrain.water_surface_height(member.root, point),
                terrain.ground_height(member.root, point),
            ) {
                if surface > floor {
                    destination.y = if let Some(depth) = move_action.depth_below_surface_m {
                        (surface - depth).max(floor)
                    } else if let Some(height) = move_action.height_above_floor_m {
                        (floor + height).min(surface)
                    } else {
                        destination.y
                    };
                    if ambient && target == Some(actor) {
                        use openzt2_zoo_state::locomotion::locomotion_types::SwimLayer;
                        let Ok(mut pose) = poses.get_mut(actor) else {
                            mark_behavior_task_for_failure_and_stop_navigation(
                                actor,
                                &mut commands,
                            );
                            continue;
                        };
                        pose.translation.y = destination.y;
                        let layer = if destination.y < surface {
                            SwimLayer::Submerged
                        } else {
                            SwimLayer::Surface
                        };
                        if let Ok(mut current) = swim_layers.get_mut(actor) {
                            current.set_if_neq(layer);
                        } else {
                            commands.entity(actor).insert(layer);
                        }
                        instruction.complete(actor, simulation_clock.tick, &mut commands);
                        continue;
                    }
                }
            }
        }
        if !move_start_has_navigation_agent(navigation_agent) {
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            continue;
        }
        let arrival_radius_m = move_action.move_radius_cm as f32 * 0.01;
        let Ok(mut generation) = navigation_generations.get_mut(actor) else {
            continue;
        };
        let request_id = generation.advance();
        let departure_entrance = target
            .and_then(|target| entrances.get(target).ok())
            .zip(target_transform)
            .filter(|_| {
                !point_from_token
                    && move_action.target_node_name.is_none()
                    && guests.contains(actor)
                    && {
                        let (document, declaration) = if behavior_task.depth == 0 {
                            (&behavior_task.document, behavior_task.declaration)
                        } else if let Some(root) = continuations.frames(actor).next() {
                            (&root.document, root.declaration)
                        } else {
                            (&behavior_task.document, behavior_task.declaration)
                        };
                        programs
                            .get(document)
                            .and_then(|document| document.behavior_task_at_index(declaration))
                            .is_some_and(|task| {
                                task.scores.iter().any(|score| {
                                    matches!(
                                score,
                                openzt2_game_data::behavior::score::BehaviorScore::LeaveZooPressure
                            )
                                })
                            })
                    }
            });
        if let Some((entrance, target_transform)) = departure_entrance {
            destination = definitions
                .and_then(|definitions| definitions.guest_generation())
                .map_or(entrance.exit, |policy| {
                    let [x, y] = policy.departure_offset_m;
                    entrance.exit + target_transform.rotation() * Vec3::new(x, 0.0, -y)
                });
            if grids.get(member.root).is_ok_and(|grid| {
                grid.width != 0 && grid.height != 0 && grid.cell_of(destination.xz()).is_none()
            }) {
                commands.entity(actor).insert(
                    openzt2_zoo_state::locomotion::locomotion_types::EntranceNavigationConnection {
                        request_id,
                        gate: entrance.exit,
                        inbound: false,
                    },
                );
            }
        }
        navigation_requests.write(NavigateTo {
            entity: actor,
            request_id,
            destination,
            arrival_radius_m,
        });
        commands
            .entity(actor)
            .insert(PendingBehaviorMoveCompletion {
                instruction: instruction.id(),
                request_id,
            });
        behavior_task.next_action_tick = simulation_clock.tick.saturating_add(1);
    }
}

/// Native target role: the destination is the behavior-task target entity's
/// world position; a non-empty authored `targetNode` resolves that named
/// attachment on the target instead, and a missing attachment is an explicit
/// failure rather than a walk toward an unset position.
fn resolve_behavior_move_destination(
    target_position: Vec3,
    named_node_position: Option<Option<Vec3>>,
) -> Result<Vec3, BehaviorMoveDestinationResolutionFailure> {
    match named_node_position {
        None => Ok(target_position),
        Some(Some(node_position)) => Ok(node_position),
        Some(None) => Err(BehaviorMoveDestinationResolutionFailure::MissingTargetNode),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BehaviorMoveDestinationResolutionFailure {
    MissingTargetNode,
}

/// Completes the move only for an arrival the locomotion owner reported for a
/// plain destination (no dock target) and the exact originating action/request.
/// Locomotion owns arrival tolerance; GlobalTransform can still precede its
/// latest fixed-step Transform update when this message is consumed.
pub(super) fn finish_behavior_move_actions_after_matching_arrival(
    mut commands: Commands,
    mut arrivals: MessageReader<Arrived>,
    clocks: Query<&ZooClock>,
    water_targets: Query<&openzt2_zoo_state::animal_behavior::behavior_natural_water_target_types::SelectedNaturalWaterTarget>,
    water_contact: Query<(
        &openzt2_zoo_state::locomotion::locomotion_types::AnimalTerrainWaterContact,
        &openzt2_zoo_state::locomotion::locomotion_types::LocomotionMode,
    )>,
    behavior_tasks: Query<
        (
            Entity,
            &BehaviorTaskExecutionState,
            &RunningBehaviorInstruction,
            &PendingBehaviorMoveCompletion,
            &WorldMember,
        ),
        Without<PendingBehaviorTaskFailure>,
    >,
) {
    for arrival in arrivals.read() {
        let Ok((actor, behavior_task, instruction, pending_move, member)) =
            behavior_tasks.get(arrival.entity)
        else {
            continue;
        };
        let Ok(clock) = clocks.get(member.root) else {
            continue;
        };
        if arrival.target.is_some()
            || arrival.request_id != pending_move.request_id
            || !pending_move.owns(behavior_task, instruction)
        {
            continue;
        }
        if let Ok(water) = water_targets.get(actor) {
            if behavior_task.target() == Some(water.region)
                && matches!(
                    water.kind,
                    openzt2_zoo_state::animal_behavior::behavior_natural_water_target_types::NaturalWaterTargetKind::Surface { .. }
                )
            {
                let arrived_in_water = water_contact.get(actor).is_ok_and(|(contact, mode)| {
                    contact.depth_m.is_some_and(|depth| depth > 0.0)
                        && *mode
                            == openzt2_zoo_state::locomotion::locomotion_types::LocomotionMode::Swim
                });
                if !arrived_in_water {
                    mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
                    continue;
                }
            }
        }
        commands
            .entity(actor)
            .remove::<PendingBehaviorMoveCompletion>();
        instruction.complete(actor, clock.tick, &mut commands);
    }
}

pub(super) fn fail_behavior_move_actions_after_navigation_failure(
    mut commands: Commands,
    mut failures: MessageReader<NavigationFailed>,
    waiting_tasks: Query<(
        &BehaviorTaskExecutionState,
        &RunningBehaviorInstruction,
        &PendingBehaviorMoveCompletion,
    )>,
) {
    for failure in failures.read() {
        let waiting_for_move =
            waiting_tasks
                .get(failure.entity)
                .ok()
                .is_some_and(|(task, instruction, pending)| {
                    failure.request_id == pending.request_id && pending.owns(task, instruction)
                });
        if waiting_for_move {
            mark_behavior_task_for_failure_and_stop_navigation(failure.entity, &mut commands);
        }
    }
}

fn move_start_has_navigation_agent(navigation_agent: Option<&NavAgent>) -> bool {
    navigation_agent.is_some()
}

/// A move at an authored speed needs the person's authored ground gaits: a
/// staff member's role or a guest's archetype supplies them.
fn person_has_authored_ground_locomotion(
    locomotion_speed: Option<
        openzt2_game_data::behavior::action::wander::BehaviorGroundLocomotionSpeed,
    >,
    staff_role: Option<&StaffRole>,
    guest_archetype: Option<&openzt2_zoo_state::guests::guest_simulation_types::GuestArchetype>,
    definitions: Option<WorldDefinitionsView<'_>>,
) -> bool {
    if locomotion_speed.is_none() {
        return true;
    }
    let tables = staff_role
        .and_then(|role| {
            Some(
                definitions?
                    .find_staff(role.0)?
                    .locomotion_tables
                    .as_slice(),
            )
        })
        .or_else(|| definitions?.guest_locomotion_tables(guest_archetype?.0));
    tables.is_some_and(|tables| {
        tables.iter().any(|table| {
            table.binder == openzt2_game_data::AssetId::from_key("ground") && !table.rows.is_empty()
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use openzt2_game_assets::behavior::behavior_asset_types::BehaviorDocumentAsset;
    use openzt2_game_data::behavior::action_record::BehaviorAction;
    use openzt2_zoo_state::locomotion::locomotion_types::NavFlags;

    fn test_move_task(app: &mut App) -> BehaviorTaskExecutionState {
        use openzt2_game_data::{
            behavior::{
                action::movement::BehaviorMoveAction,
                document::{BehaviorDocument, BehaviorTask, BehaviorTaskActionPhase},
            },
            AssetId,
        };
        let program = AssetId::from_key("test:move");
        app.init_resource::<Assets<BehaviorDocumentAsset>>()
            .add_observer(openzt2_zoo_state::behavior_task_execution_types::running_actions::apply_behavior_instruction_completion);
        let document = app
            .world_mut()
            .resource_mut::<Assets<BehaviorDocumentAsset>>()
            .add(BehaviorDocumentAsset::from_test_document(
                BehaviorDocument::Tasks(vec![BehaviorTask {
                    id: program,
                    name: "move".into(),
                    priority: None,
                    task_delay_seconds: None,
                    reservation_tag: None,
                    sensor_tag: None,
                    subjects: Vec::new(),
                    targets: Vec::new(),
                    objects: Vec::new(),
                    candidate_eligibility_requirements: Vec::new(),
                    scores: Vec::new(),
                    execution: BehaviorTaskActionPhase::Supported(vec![BehaviorAction::Move(
                        BehaviorMoveAction {
                            locomotion_speed: None,
                            target_node_name: None,
                            move_radius_cm: 200,
                            depth_below_surface_m: None,
                            height_above_floor_m: None,
                        },
                    )]),
                    completion: BehaviorTaskActionPhase::Supported(Vec::new()),
                    failure: BehaviorTaskActionPhase::Supported(Vec::new()),
                }]),
            ));
        BehaviorTaskExecutionState {
            execution_id: 1,
            program,
            target: None,
            document,
            declaration: 0,
            phase: openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionPhase::Execution,
            action: 0,
            repetitions: 0,
            next_action_tick: 1,
            object: None,
            interaction_slot: None,
            depth: 0,
        }
    }

    fn bind_move_completion(world: &mut World, actor: Entity) {
        let instruction = world.get::<RunningBehaviorInstruction>(actor).unwrap().id();
        world
            .entity_mut(actor)
            .insert(PendingBehaviorMoveCompletion {
                instruction,
                request_id: 1,
            });
    }

    #[test]
    fn same_tick_arrival_completes_and_duplicate_arrival_is_ignored() {
        let mut app = App::new();
        let root = app
            .world_mut()
            .spawn(ZooClock {
                fractional_tick: Default::default(),
                tick: 0,
                absolute_day: 0,
                tick_in_day: 0,
            })
            .id();
        app.add_message::<Arrived>()
            .add_systems(Update, finish_behavior_move_actions_after_matching_arrival);
        let task = test_move_task(&mut app);
        let actor = app
            .world_mut()
            .spawn((
                WorldMember { root },
                task,
                NavAgent {
                    radius_m: 0.5,
                    max_speed_mps: 1.0,
                    acceleration_mps2: 1.0,
                    capabilities: NavFlags::STAFF,
                },
                GlobalTransform::IDENTITY,
            ))
            .id();
        bind_move_completion(app.world_mut(), actor);
        app.world_mut().write_message(Arrived {
            entity: actor,
            request_id: 1,
            target: None,
        });
        app.update();
        assert_eq!(
            app.world()
                .get::<BehaviorTaskExecutionState>(actor)
                .unwrap()
                .action,
            1
        );
        assert!(app
            .world()
            .get::<PendingBehaviorMoveCompletion>(actor)
            .is_none());
        app.world_mut().write_message(Arrived {
            entity: actor,
            request_id: 1,
            target: None,
        });
        app.update();
        assert_eq!(
            app.world()
                .get::<BehaviorTaskExecutionState>(actor)
                .unwrap()
                .action,
            1
        );
    }

    #[test]
    fn nested_set_move_advances_the_current_nested_action() {
        let mut app = App::new();
        let root = app
            .world_mut()
            .spawn(ZooClock {
                fractional_tick: Default::default(),
                tick: 0,
                absolute_day: 0,
                tick_in_day: 0,
            })
            .id();
        app.add_message::<Arrived>()
            .add_systems(Update, finish_behavior_move_actions_after_matching_arrival);
        let mut task = test_move_task(&mut app);
        let parent = task.create_return_frame_after_action(7);
        task.depth = 1;
        let actor = app
            .world_mut()
            .spawn((
                WorldMember { root },
                task,
                NavAgent {
                    radius_m: 0.5,
                    max_speed_mps: 1.0,
                    acceleration_mps2: 1.0,
                    capabilities: NavFlags::STAFF,
                },
                GlobalTransform::IDENTITY,
            ))
            .id();
        app.world_mut().spawn((
            openzt2_zoo_state::behavior_task_execution_types::continuations::BehaviorContinuationOf(
                actor,
            ),
            openzt2_zoo_state::behavior_task_execution_types::continuations::SuspendedBehaviorFrame(
                parent,
            ),
        ));
        bind_move_completion(app.world_mut(), actor);
        app.world_mut().write_message(Arrived {
            entity: actor,
            request_id: 1,
            target: None,
        });
        app.update();
        let task = app
            .world()
            .get::<BehaviorTaskExecutionState>(actor)
            .unwrap();
        assert_eq!(task.action, 1);
        assert_eq!(task.depth, 1);
    }

    #[test]
    fn superseded_move_arrival_is_ignored_and_duplicate_failure_is_consumed_once() {
        let mut app = App::new();
        let root = app
            .world_mut()
            .spawn(ZooClock {
                fractional_tick: Default::default(),
                tick: 1,
                absolute_day: 0,
                tick_in_day: 1,
            })
            .id();
        app.add_message::<Arrived>()
            .add_systems(Update, finish_behavior_move_actions_after_matching_arrival);
        let task = test_move_task(&mut app);
        let actor = app
            .world_mut()
            .spawn((
                WorldMember { root },
                task,
                NavAgent {
                    radius_m: 0.5,
                    max_speed_mps: 1.0,
                    acceleration_mps2: 1.0,
                    capabilities: NavFlags::STAFF,
                },
                GlobalTransform::IDENTITY,
            ))
            .id();
        bind_move_completion(app.world_mut(), actor);
        app.world_mut()
            .get_mut::<BehaviorTaskExecutionState>(actor)
            .unwrap()
            .program = openzt2_game_data::AssetId::from_key("test:superseding");
        app.world_mut().write_message(Arrived {
            entity: actor,
            request_id: 2,
            target: None,
        });
        app.update();
        assert_eq!(
            app.world()
                .get::<BehaviorTaskExecutionState>(actor)
                .unwrap()
                .action,
            0
        );
        assert!(app
            .world()
            .get::<PendingBehaviorMoveCompletion>(actor)
            .is_some());

        let mut failure_app = App::new();
        failure_app
            .add_message::<NavigationFailed>()
            .add_systems(Update, fail_behavior_move_actions_after_navigation_failure);
        let failure_task = test_move_task(&mut failure_app);
        let failure_actor = failure_app.world_mut().spawn((failure_task,)).id();
        bind_move_completion(failure_app.world_mut(), failure_actor);
        failure_app.world_mut().write_message(NavigationFailed {
            entity: failure_actor,
            request_id: 1,
            reason: openzt2_zoo_state::locomotion::locomotion_types::NavigationFailure::NoRoute,
        });
        failure_app.update();
        assert!(failure_app
            .world()
            .get::<PendingBehaviorMoveCompletion>(failure_actor)
            .is_none());
        assert!(failure_app
            .world()
            .get::<openzt2_zoo_state::behavior_task_execution_types::PendingBehaviorTaskFailure>(
                failure_actor
            )
            .is_some());
        failure_app.world_mut().write_message(NavigationFailed {
            entity: failure_actor,
            request_id: 1,
            reason: openzt2_zoo_state::locomotion::locomotion_types::NavigationFailure::NoRoute,
        });
        failure_app.update();
    }

    #[test]
    fn same_request_failure_wins_over_same_frame_arrival() {
        let mut app = App::new();
        let root = app
            .world_mut()
            .spawn(ZooClock {
                fractional_tick: Default::default(),
                tick: 1,
                absolute_day: 0,
                tick_in_day: 1,
            })
            .id();
        app.add_message::<Arrived>()
            .add_message::<NavigationFailed>()
            .add_systems(
                Update,
                (
                    fail_behavior_move_actions_after_navigation_failure,
                    finish_behavior_move_actions_after_matching_arrival,
                )
                    .chain(),
            );
        let task = test_move_task(&mut app);
        let actor = app
            .world_mut()
            .spawn((
                WorldMember { root },
                task,
                NavAgent {
                    radius_m: 0.5,
                    max_speed_mps: 1.0,
                    acceleration_mps2: 1.0,
                    capabilities: NavFlags::STAFF,
                },
                GlobalTransform::IDENTITY,
            ))
            .id();
        bind_move_completion(app.world_mut(), actor);
        app.world_mut().write_message(NavigationFailed {
            entity: actor,
            request_id: 1,
            reason: openzt2_zoo_state::locomotion::locomotion_types::NavigationFailure::NoRoute,
        });
        app.world_mut().write_message(Arrived {
            entity: actor,
            request_id: 1,
            target: None,
        });
        app.update();
        assert_eq!(
            app.world()
                .get::<BehaviorTaskExecutionState>(actor)
                .unwrap()
                .action,
            0
        );
        assert!(app
            .world()
            .get::<PendingBehaviorTaskFailure>(actor)
            .is_some());
        assert!(app
            .world()
            .get::<PendingBehaviorMoveCompletion>(actor)
            .is_none());
    }
}
