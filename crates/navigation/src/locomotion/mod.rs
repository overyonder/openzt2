//! Focused route planning, local steering, fixed-step movement, and docking.

mod animal_animation_locomotion;
mod animal_navigation_policy;
#[cfg(test)]
mod animal_terrain_slope_tests;
mod animal_water_contact;
pub mod authored_tank_gate_exit;
mod avian_navigation_agent_driving;
mod behavior_evasion_steering;
mod changed_destination_route_planning;
mod contact_steering_projection;
mod direct_docking_request_execution;
mod direct_locomotion_execution;
mod entity_pursuit_execution;
mod fence_navigation_crossing_types;
mod fence_navigation_policy;
mod kinematic_navigation_agent_motion_integration;
mod local_steering_calculation;
mod locomotion_clip_dependencies;
mod navigation_agent_initialization;
mod navigation_destination_request_execution;
mod navigation_reachability_testing;
mod path_surface_navigation_projection;
mod pathfinding;
pub mod person_walk_animation;
mod sampled_animation_motion_projection;
mod spatial_index_and_docking_operations;
mod staff_tank_gate_navigation;
mod tank_water_navigation_overlay;
mod terrain_edit_navigation_refresh;
mod topology_navigation_edge_invalidation;
mod world_navigation_installation;

use openzt2_zoo_state::locomotion::locomotion_types::Arrived;
use openzt2_zoo_state::locomotion::locomotion_types::DockAt;
use openzt2_zoo_state::locomotion::locomotion_types::NavigateTo;
use openzt2_zoo_state::locomotion::locomotion_types::NavigationFailed;
use openzt2_zoo_state::locomotion::locomotion_types::PursueEntity;
use openzt2_zoo_state::locomotion::locomotion_types::ReachabilityTested;
use openzt2_zoo_state::locomotion::locomotion_types::TestReachability;

use bevy::prelude::*;

use openzt2_game_assets::application_lifecycle::GamePhase;
use openzt2_world_state::application_schedule::FixedGameSet;

/// Projects animal walk clips onto navigation agents. Behaviors that start an
/// animal's own clip run before it, so a stop never replaces that clip.
use openzt2_zoo_state::locomotion::AnimalAnimationNavigationProjection;
use openzt2_zoo_state::locomotion::NavigationRequestAcceptance;

use fence_navigation_crossing_types::FenceNavigationCrossingIndex;

fn fixed_interval_timer(seconds: f32) -> Timer {
    let duration = if seconds <= 0.0 {
        std::time::Duration::ZERO
    } else {
        std::time::Duration::try_from_secs_f32(seconds).unwrap_or(std::time::Duration::MAX)
    };
    Timer::new(duration, TimerMode::Once)
}

pub struct LocomotionPlugin;

impl Plugin for LocomotionPlugin {
    fn build(&self, game_application: &mut App) {
        game_application
            .register_required_components::<openzt2_zoo_state::locomotion::locomotion_types::ContactSteering, contact_steering_projection::ContactRecovery>()
            .register_required_components::<openzt2_zoo_state::locomotion::locomotion_types::MotionProgress, spatial_index_and_docking_operations::RouteStallTimer>()
            .register_required_components::<openzt2_zoo_state::animal_lifecycle::types::AnimalVariant, animal_navigation_policy::AnimalCollisionProfile>()
            .init_resource::<changed_destination_route_planning::ReusableRouteSearchScratch>()
            .register_required_components::<openzt2_world_state::world_spawn::world_membership_types::WorldRoot, FenceNavigationCrossingIndex>()
            .add_observer(fence_navigation_crossing_types::mark_crossing_inserted)
            .add_observer(fence_navigation_crossing_types::mark_crossing_discarded)
            .add_observer(topology_navigation_edge_invalidation::remove_crossings_after_fence_is_removed)
            .add_message::<NavigateTo>()
            .add_message::<TestReachability>()
            .add_message::<terrain_edit_navigation_refresh::NavigationTerrainGraphChanged>()
            .add_message::<path_surface_navigation_projection::NavigationPathGraphChanged>()
            .add_message::<tank_water_navigation_overlay::TankNavigationWaterChanged>()
            .add_message::<openzt2_zoo_state::aquatic::aquatic_simulation_types::TankWaterCellsChanged>()
            .add_message::<openzt2_zoo_state::terrain::terrain_water_presentation_types::TerrainWaterCellsChanged>()
            .add_message::<openzt2_zoo_state::habitat::habitat_cell_ownership_changes::HabitatCellOwnershipChanged>()
            .add_message::<ReachabilityTested>()
            .add_message::<DockAt>()
            .add_message::<PursueEntity>()
            .add_message::<Arrived>()
            .add_message::<NavigationFailed>()
            .add_message::<world_navigation_installation::NavigationGraphReplaced>()
            .add_message::<openzt2_zoo_state::terrain::terrain_navigation_change_types::TerrainNavigationRestored>()
            .add_message::<openzt2_zoo_state::terrain::terrain_navigation_change_types::SnapshotTerrainNavigationPending>()
            .add_message::<openzt2_zoo_state::terrain::terrain_navigation_change_types::SnapshotTerrainNavigationReady>()
            .add_observer(
                openzt2_zoo_state::behavior_task_execution_types::running_actions::activated_behavior_actor::<openzt2_zoo_state::behavior_task_execution_types::running_actions::RunBehaviorExitTankGate>
                    .pipe(authored_tank_gate_exit::start_authored_tank_gate_exit),
            )
            .add_systems(
                FixedUpdate,
                openzt2_zoo_state::behavior_task_execution_types::running_actions::update_waiting_behavior_actors
                    .pipe(authored_tank_gate_exit::start_authored_tank_gate_exit)
                    .in_set(openzt2_zoo_state::behavior_task_execution_types::running_actions::BehaviorStepFixedSet::Think),
            )
            .add_systems(
                Update,
                terrain_edit_navigation_refresh::acknowledge_restored_terrain_navigation
                    .after(path_surface_navigation_projection::project_path_surfaces_into_navigation_graph)
                    .after(tank_water_navigation_overlay::project_tank_water_onto_navigation_cells)
                    .in_set(openzt2_world_state::application_schedule::GameSet::Intent)
                    .run_if(in_state(GamePhase::Loading)),
            )
            .add_systems(
                Update,
                world_navigation_installation::
                    install_terrain_derived_navigation_graph_for_loaded_world,
            )
            .add_systems(
                Update,
                terrain_edit_navigation_refresh::refresh_navigation_graph_after_terrain_restoration
                    .after(world_navigation_installation::install_terrain_derived_navigation_graph_for_loaded_world)
                    .before(path_surface_navigation_projection::project_path_surfaces_into_navigation_graph)
                    .in_set(openzt2_world_state::application_schedule::GameSet::Intent)
                    .run_if(in_state(GamePhase::Loading)),
            )
            .add_systems(
                Update,
                tank_water_navigation_overlay::project_tank_water_onto_navigation_cells
                    .after(terrain_edit_navigation_refresh::refresh_navigation_graph_after_terrain_restoration)
                    .in_set(openzt2_world_state::application_schedule::GameSet::Intent)
                    .run_if(in_state(GamePhase::Loading)),
            )
            .add_systems(
                Update,
                openzt2_zoo_state::locomotion::navigation_path_geometry_projection::project_navigation_path_geometry
                    .before(path_surface_navigation_projection::project_path_surfaces_into_navigation_graph)
                    .in_set(openzt2_world_state::application_schedule::GameSet::Intent)
                    .run_if(in_state(GamePhase::Loading)),
            )
            .add_systems(
                Update,
                path_surface_navigation_projection::project_path_surfaces_into_navigation_graph
                    .after(world_navigation_installation::install_terrain_derived_navigation_graph_for_loaded_world)
                    .in_set(openzt2_world_state::application_schedule::GameSet::Intent)
                    .run_if(in_state(GamePhase::Loading)),
            )
            .add_systems(
                Update,
                path_surface_navigation_projection::project_elevated_path_colliders
                    .in_set(openzt2_world_state::application_schedule::GameSet::Intent)
                    .run_if(in_state(GamePhase::Loading)),
            )
            .add_systems(
                FixedUpdate,
                (
                    animal_water_contact::project_animal_water_contact.in_set(FixedGameSet::Clock),
                    animal_navigation_policy::project_obstacle_priorities.in_set(FixedGameSet::Clock),
                    animal_navigation_policy::project_animal_floating_fact
                        .after(animal_water_contact::project_animal_water_contact)
                        .in_set(FixedGameSet::Clock),
                    spatial_index_and_docking_operations::project_agent_spatial_membership
                        .in_set(openzt2_zoo_state::locomotion::AgentSpatialMembershipProjection)
                        .in_set(FixedGameSet::Clock),
                    spatial_index_and_docking_operations::order_changed_spatial_cell_members
                        .after(spatial_index_and_docking_operations::project_agent_spatial_membership)
                        .before(FixedGameSet::Think),
                ),
            )
            .add_systems(
                FixedUpdate,
                (
                    locomotion_clip_dependencies::project_gait_rig_joint_lookup,
                    crate::locomotion::animal_animation_locomotion::project_animal_animation_motion_to_navigation
                        .after(locomotion_clip_dependencies::project_gait_rig_joint_lookup)
                        .in_set(AnimalAnimationNavigationProjection),
                    person_walk_animation::project_person_motion_to_walk_animation
                        .after(locomotion_clip_dependencies::project_gait_rig_joint_lookup)
                        .in_set(AnimalAnimationNavigationProjection),
                    terrain_edit_navigation_refresh::refresh_navigation_graph_after_terrain_edits,
                    path_surface_navigation_projection::project_elevated_path_colliders,
                    terrain_edit_navigation_refresh::invalidate_routes_after_navigation_geometry_changes
                        .after_ignore_deferred(tank_water_navigation_overlay::project_tank_water_onto_navigation_cells),
                    openzt2_zoo_state::locomotion::navigation_path_geometry_projection::project_navigation_path_geometry
                        .before(path_surface_navigation_projection::project_path_surfaces_into_navigation_graph),
                    path_surface_navigation_projection::project_path_surfaces_into_navigation_graph
                        .after(terrain_edit_navigation_refresh::refresh_navigation_graph_after_terrain_edits),
                    fence_navigation_crossing_types::prune_empty_route_edges.before(topology_navigation_edge_invalidation::synchronize_fence_navigation_crossings),
                    topology_navigation_edge_invalidation::synchronize_fence_navigation_crossings
                        .after_ignore_deferred(path_surface_navigation_projection::project_path_surfaces_into_navigation_graph),
                    tank_water_navigation_overlay::project_tank_water_onto_navigation_cells
                        .after(path_surface_navigation_projection::project_path_surfaces_into_navigation_graph)
                        .run_if(in_state(GamePhase::InGame)),
                    navigation_agent_initialization::initialize_navigation_agents_with_bounded_runtime_state
                        .after(AnimalAnimationNavigationProjection),
                    animal_navigation_policy::project_animal_collision_profiles,
                    navigation_reachability_testing::test_requested_navigation_destination_reachability
                        .after_ignore_deferred(animal_navigation_policy::project_animal_collision_profiles)
                        .after(navigation_agent_initialization::initialize_navigation_agents_with_bounded_runtime_state)
                        .after_ignore_deferred(tank_water_navigation_overlay::project_tank_water_onto_navigation_cells)
                        .after(topology_navigation_edge_invalidation::synchronize_fence_navigation_crossings)
                        .run_if(on_message::<TestReachability>),
                    entity_pursuit_execution::accept_entity_pursuit_requests
                        .after_ignore_deferred(animal_navigation_policy::project_animal_collision_profiles)
                        .after(openzt2_zoo_state::behavior_task_execution_types::running_actions::BehaviorStepFixedSet::Think)
                        .after(navigation_agent_initialization::initialize_navigation_agents_with_bounded_runtime_state)
                        .after_ignore_deferred(tank_water_navigation_overlay::project_tank_water_onto_navigation_cells)
                        .in_set(NavigationRequestAcceptance)
                        .run_if(on_message::<PursueEntity>),
                    navigation_destination_request_execution::accept_navigation_destination_requests
                        .after(entity_pursuit_execution::accept_entity_pursuit_requests)
                        .in_set(NavigationRequestAcceptance)
                        .run_if(on_message::<NavigateTo>),
                    direct_docking_request_execution::accept_direct_docking_requests
                        .after(navigation_destination_request_execution::accept_navigation_destination_requests)
                        .in_set(NavigationRequestAcceptance)
                        .run_if(on_message::<DockAt>),
                    direct_locomotion_execution::clear_planned_navigation_state_for_direct_locomotion
                        .after(NavigationRequestAcceptance),
                )
                    .in_set(FixedGameSet::Think),
            )
            .add_systems(
                FixedUpdate,
                (
                    contact_steering_projection::project_avian_contacts_into_navigation_steering
                        .before_ignore_deferred(local_steering_calculation::calculate_route_following_and_separation_steering),
                    direct_locomotion_execution::calculate_direct_locomotion_steering,
                )
                    .in_set(FixedGameSet::Navigate),
            )
            .add_systems(
                FixedUpdate,
                (
                    entity_pursuit_execution::steer_entity_pursuits_toward_live_targets,
                    animal_navigation_policy::invalidate_animal_routes_after_policy_changes
                        .after(entity_pursuit_execution::steer_entity_pursuits_toward_live_targets),
                    authored_tank_gate_exit::settle_authored_tank_gate_exit,
                    staff_tank_gate_navigation::receive_staff_tank_gate_events,
                    staff_tank_gate_navigation::resume_completed_staff_tank_gate_journeys,
                    staff_tank_gate_navigation::begin_staff_tank_gate_journeys
                        .after(staff_tank_gate_navigation::resume_completed_staff_tank_gate_journeys),
                    changed_destination_route_planning::queue_changed_navigation_destinations
                        .after(NavigationRequestAcceptance)
                        .after(animal_navigation_policy::invalidate_animal_routes_after_policy_changes)
                        .after(staff_tank_gate_navigation::begin_staff_tank_gate_journeys),
                    changed_destination_route_planning::admit_waiting_route_searches
                        .after(changed_destination_route_planning::queue_changed_navigation_destinations)
                        .run_if(any_with_component::<changed_destination_route_planning::WaitingRouteSearch>),
                    changed_destination_route_planning::advance_active_navigation_searches
                        .after(changed_destination_route_planning::admit_waiting_route_searches)
                        .run_if(any_with_component::<changed_destination_route_planning::ActiveRouteSearch>),
                    behavior_evasion_steering::project_behavior_evasion_steering
                        .after(entity_pursuit_execution::steer_entity_pursuits_toward_live_targets),
                    local_steering_calculation::calculate_route_following_and_separation_steering
                        .after(changed_destination_route_planning::advance_active_navigation_searches)
                        .after_ignore_deferred(behavior_evasion_steering::project_behavior_evasion_steering),
                )
                    .in_set(FixedGameSet::Navigate),
            )
            .add_systems(
                FixedUpdate,
                (
                    staff_tank_gate_navigation::execute_staff_tank_gate_journeys
                        .before(kinematic_navigation_agent_motion_integration::integrate_non_physics_planned_navigation_motion)
                        .before(direct_locomotion_execution::integrate_non_physics_direct_locomotion),
                    kinematic_navigation_agent_motion_integration::
                        integrate_non_physics_planned_navigation_motion
                        .after_ignore_deferred(local_steering_calculation::calculate_route_following_and_separation_steering),
                    direct_locomotion_execution::integrate_non_physics_direct_locomotion
                        .after_ignore_deferred(direct_locomotion_execution::calculate_direct_locomotion_steering),
                    avian_navigation_agent_driving::
                        drive_avian_navigation_agent_velocities_and_complete_routes
                        .after_ignore_deferred(local_steering_calculation::calculate_route_following_and_separation_steering)
                        .after_ignore_deferred(direct_locomotion_execution::calculate_direct_locomotion_steering),
                )
                    .after(openzt2_world_state::animation_playback::locomotion_root_motion::AnimationLocomotionMotionSampling)
                    .after(openzt2_zoo_state::topology::gate_operation_types::GateOperationSet)
                    .in_set(FixedGameSet::Act),
            )
            .add_systems(
                FixedUpdate,
                (
                    topology_navigation_edge_invalidation::invalidate_routes_after_topology_changes,

                    spatial_index_and_docking_operations::detect_stuck_agents
                        .after_ignore_deferred(topology_navigation_edge_invalidation::invalidate_routes_after_topology_changes),
                    spatial_index_and_docking_operations::complete_docking
                        .after_ignore_deferred(spatial_index_and_docking_operations::detect_stuck_agents),
                )
                    .in_set(FixedGameSet::Cleanup),
            );
    }
}

#[cfg(test)]
mod tests;
