//! Focused route planning, local steering, fixed-step movement, and docking.

mod animal_animation_locomotion;
mod animal_navigation_policy;
#[cfg(test)]
mod animal_terrain_slope_tests;
mod animal_water_contact;
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
mod navigation_agent_initialization;
mod navigation_destination_request_execution;
mod navigation_reachability_testing;
mod path_surface_navigation_projection;
mod pathfinding;
mod person_walk_animation;
mod sampled_animation_motion_projection;
mod spatial_index_and_docking_operations;
mod staff_tank_gate_navigation;
mod tank_water_navigation_overlay;
mod terrain_edit_navigation_refresh;
mod topology_navigation_edge_invalidation;
mod world_navigation_installation;

use openzt2_zoo_state::locomotion::locomotion_types::ActiveTerrainDerivedNavigationGraph;
use openzt2_zoo_state::locomotion::locomotion_types::Arrived;
use openzt2_zoo_state::locomotion::locomotion_types::DockAt;
use openzt2_zoo_state::locomotion::locomotion_types::LocomotionCapacity;
use openzt2_zoo_state::locomotion::locomotion_types::NavigateTo;
use openzt2_zoo_state::locomotion::locomotion_types::NavigationFailed;
use openzt2_zoo_state::locomotion::locomotion_types::NavigationRequestSequence;
use openzt2_zoo_state::locomotion::locomotion_types::PursueEntity;
use openzt2_zoo_state::locomotion::locomotion_types::ReachabilityTested;
use openzt2_zoo_state::locomotion::locomotion_types::SpatialGrid;
use openzt2_zoo_state::locomotion::locomotion_types::TestReachability;

use bevy::prelude::*;

use openzt2_game_assets::application_lifecycle::GamePhase;
use openzt2_world_state::application_schedule::FixedGameSet;

/// Projects animal walk clips onto navigation agents. Behaviors that start an
/// animal's own clip run before it, so a stop never replaces that clip.
use openzt2_zoo_state::locomotion::AnimalAnimationNavigationProjection;
use openzt2_zoo_state::locomotion::NavigationRequestAcceptance;

use fence_navigation_crossing_types::FenceNavigationCrossingIndex;

pub struct LocomotionPlugin;

impl Plugin for LocomotionPlugin {
    fn build(&self, game_application: &mut App) {
        game_application
            .init_resource::<FenceNavigationCrossingIndex>()
            .add_observer(fence_navigation_crossing_types::index_inserted_fence_crossings)
            .add_observer(fence_navigation_crossing_types::unindex_discarded_fence_crossings)
            .init_resource::<NavigationRequestSequence>()
            .init_resource::<SpatialGrid>()
            .add_message::<NavigateTo>()
            .add_message::<TestReachability>()
            .add_message::<terrain_edit_navigation_refresh::NavigationTerrainGraphChanged>()
            .add_message::<path_surface_navigation_projection::NavigationPathGraphChanged>()
            .add_message::<ReachabilityTested>()
            .add_message::<DockAt>()
            .add_message::<PursueEntity>()
            .add_message::<Arrived>()
            .add_message::<NavigationFailed>()
            .add_message::<world_navigation_installation::NavigationGraphReplaced>()
            .add_message::<openzt2_zoo_state::terrain::terrain_navigation_change_types::TerrainNavigationRestored>()
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
                animal_water_contact::project_animal_water_contact.in_set(FixedGameSet::Clock),
            )
            .add_systems(
                FixedUpdate,
                (
                    crate::locomotion::animal_animation_locomotion::project_animal_animation_motion_to_navigation
                        .in_set(AnimalAnimationNavigationProjection),
                    person_walk_animation::project_person_motion_to_walk_animation
                        .in_set(AnimalAnimationNavigationProjection),
                    terrain_edit_navigation_refresh::refresh_navigation_graph_after_terrain_edits,
                    path_surface_navigation_projection::project_elevated_path_colliders,
                    terrain_edit_navigation_refresh::invalidate_routes_after_navigation_geometry_changes
                        .after_ignore_deferred(path_surface_navigation_projection::project_path_surfaces_into_navigation_graph),
                    path_surface_navigation_projection::project_path_surfaces_into_navigation_graph
                        .after(terrain_edit_navigation_refresh::refresh_navigation_graph_after_terrain_edits),
                    topology_navigation_edge_invalidation::synchronize_fence_navigation_crossings
                        .after_ignore_deferred(path_surface_navigation_projection::project_path_surfaces_into_navigation_graph),
                    tank_water_navigation_overlay::project_tank_water_onto_navigation_cells
                        .after(path_surface_navigation_projection::project_path_surfaces_into_navigation_graph)
                        .run_if(in_state(GamePhase::InGame)),
                    navigation_agent_initialization::initialize_navigation_agents_with_bounded_runtime_state
                        .after(AnimalAnimationNavigationProjection),
                    navigation_reachability_testing::test_requested_navigation_destination_reachability
                        .after(navigation_agent_initialization::initialize_navigation_agents_with_bounded_runtime_state)
                        .after_ignore_deferred(tank_water_navigation_overlay::project_tank_water_onto_navigation_cells)
                        .after(topology_navigation_edge_invalidation::synchronize_fence_navigation_crossings)
                        .run_if(on_message::<TestReachability>),
                    entity_pursuit_execution::accept_entity_pursuit_requests
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
                    staff_tank_gate_navigation::receive_staff_tank_gate_events,
                    staff_tank_gate_navigation::resume_completed_staff_tank_gate_journeys,
                    staff_tank_gate_navigation::begin_staff_tank_gate_journeys
                        .after(staff_tank_gate_navigation::resume_completed_staff_tank_gate_journeys),
                    changed_destination_route_planning::prepare_changed_navigation_destinations
                        .after(animal_navigation_policy::invalidate_animal_routes_after_policy_changes)
                        .after(staff_tank_gate_navigation::begin_staff_tank_gate_journeys),
                    changed_destination_route_planning::admit_pending_route_searches
                        .after(changed_destination_route_planning::prepare_changed_navigation_destinations),
                    changed_destination_route_planning::advance_active_navigation_searches
                        .after_ignore_deferred(changed_destination_route_planning::admit_pending_route_searches),
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
                        integrate_non_physics_planned_navigation_motion,
                    crate::locomotion::direct_locomotion_execution::integrate_non_physics_direct_locomotion,
                    avian_navigation_agent_driving::
                        drive_avian_navigation_agent_velocities_and_complete_routes,
                )
                    .after(openzt2_world_state::animation_playback::locomotion_root_motion::AnimationLocomotionMotionSampling)
                    .after(openzt2_zoo_state::topology::gate_operation_types::GateOperationSet)
                    .in_set(FixedGameSet::Act),
            )
            .add_systems(
                FixedUpdate,
                (
                    topology_navigation_edge_invalidation::invalidate_routes_after_topology_changes,

                    spatial_index_and_docking_operations::project_agent_spatial_membership,
                    spatial_index_and_docking_operations::order_changed_spatial_cell_members
                        .after(spatial_index_and_docking_operations::project_agent_spatial_membership),
                    spatial_index_and_docking_operations::detect_stuck_agents
                        .after_ignore_deferred(topology_navigation_edge_invalidation::invalidate_routes_after_topology_changes),
                    spatial_index_and_docking_operations::complete_docking
                        .after_ignore_deferred(spatial_index_and_docking_operations::detect_stuck_agents)
                        .after_ignore_deferred(spatial_index_and_docking_operations::order_changed_spatial_cell_members),
                )
                    .in_set(FixedGameSet::Cleanup),
            )
            .add_systems(
                OnExit(GamePhase::InGame),
                clear_navigation_resources_after_leaving_gameplay,
            );
    }
}

fn clear_navigation_resources_after_leaving_gameplay(
    mut commands: Commands,
    mut spatial_grid: ResMut<SpatialGrid>,
) {
    *spatial_grid = default();
    commands.remove_resource::<ActiveTerrainDerivedNavigationGraph>();
    commands.remove_resource::<LocomotionCapacity>();
}

#[cfg(test)]
mod tests;
