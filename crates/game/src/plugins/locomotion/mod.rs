//! Focused route planning, local steering, fixed-step movement, and docking.

mod animal_animation_locomotion;
mod animal_navigation_policy;
mod avian_navigation_agent_driving;
mod changed_destination_route_planning;
mod contact_steering_projection;
mod direct_docking_request_execution;
mod direct_locomotion_execution;
mod kinematic_navigation_agent_motion_integration;
mod local_steering_calculation;
pub mod locomotion_types;
mod navigation_agent_initialization;
mod navigation_destination_request_execution;
mod navigation_reachability_testing;
mod pathfinding;
mod spatial_index_and_docking_operations;
mod terrain_derived_navigation_graph_construction;
mod terrain_derived_navigation_graph_edge_queries;
mod terrain_derived_navigation_graph_operations;
mod terrain_derived_navigation_graph_types;
mod terrain_edit_navigation_refresh;
mod topology_navigation_edge_invalidation;
mod world_navigation_installation;

use locomotion_types::{
    ActiveTerrainDerivedNavigationGraph, Arrived, DockAt, LocomotionCapacity, NavigateTo,
    NavigationFailed, NavigationOverlay, NavigationRequestSequence, ReachabilityTested,
    SpatialGrid, TestReachability,
};

use bevy::prelude::*;

use crate::application_lifecycle::GamePhase;
use crate::application_schedule::FixedGameSet;

pub struct LocomotionPlugin;

impl Plugin for LocomotionPlugin {
    fn build(&self, game_application: &mut App) {
        game_application
            .init_resource::<NavigationOverlay>()
            .init_resource::<NavigationRequestSequence>()
            .init_resource::<SpatialGrid>()
            .add_message::<NavigateTo>()
            .add_message::<TestReachability>()
            .add_message::<ReachabilityTested>()
            .add_message::<DockAt>()
            .add_message::<Arrived>()
            .add_message::<NavigationFailed>()
            .add_systems(
                Update,
                world_navigation_installation::
                    install_terrain_derived_navigation_graph_for_loaded_world,
            )
            .add_systems(
                FixedUpdate,
                (
                    animal_animation_locomotion::project_animal_animation_motion_to_navigation,
                    terrain_edit_navigation_refresh::
                        refresh_navigation_graph_and_invalidate_routes_after_terrain_edits,
                    navigation_agent_initialization::
                        initialize_navigation_agents_with_bounded_runtime_state,
                    navigation_reachability_testing::
                        test_requested_navigation_destination_reachability,
                    navigation_destination_request_execution::
                        accept_navigation_destination_requests,
                    direct_docking_request_execution::accept_direct_docking_requests,
                    direct_locomotion_execution::
                        clear_planned_navigation_state_for_direct_locomotion,
                )
                    .chain()
                    .in_set(FixedGameSet::Think),
            )
            .add_systems(
                FixedUpdate,
                (
                    animal_navigation_policy::invalidate_animal_routes_after_policy_changes,
                    changed_destination_route_planning::
                        plan_routes_for_changed_navigation_destinations,
                    contact_steering_projection::
                        project_avian_contacts_into_navigation_steering,
                    local_steering_calculation::
                        calculate_route_following_and_separation_steering,
                    direct_locomotion_execution::calculate_direct_locomotion_steering,
                )
                    .chain()
                    .in_set(FixedGameSet::Navigate),
            )
            .add_systems(
                FixedUpdate,
                (
                    kinematic_navigation_agent_motion_integration::
                        integrate_non_physics_planned_navigation_motion,
                    direct_locomotion_execution::integrate_non_physics_direct_locomotion,
                    avian_navigation_agent_driving::
                        drive_avian_navigation_agent_velocities_and_complete_routes,
                )
                    .in_set(FixedGameSet::Act),
            )
            .add_systems(
                FixedUpdate,
                (
                    topology_navigation_edge_invalidation::
                        synchronize_navigation_edge_blocking_and_invalidate_routes_after_topology_changes,
                    spatial_index_and_docking_operations::rebuild_spatial_grid,
                    spatial_index_and_docking_operations::detect_stuck_agents,
                    spatial_index_and_docking_operations::complete_docking,
                )
                    .chain()
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
    mut navigation_overlay: ResMut<NavigationOverlay>,
    mut spatial_grid: ResMut<SpatialGrid>,
) {
    *navigation_overlay = default();
    *spatial_grid = default();
    commands.remove_resource::<ActiveTerrainDerivedNavigationGraph>();
    commands.remove_resource::<LocomotionCapacity>();
}

#[cfg(test)]
mod tests;
