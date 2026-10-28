use bevy::prelude::*;

pub mod locomotion_types;
pub mod terrain_derived_navigation_graph_types;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AnimalAnimationNavigationProjection;

/// Locomotion accepts this step's destination, docking and pursuit requests.
/// Requesters ordered after it read the outcome from the agent's destination.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NavigationRequestAcceptance;

mod terrain_derived_navigation_graph_construction;

mod terrain_derived_navigation_graph_operations;

mod navigation_path_surfaces;
mod terrain_derived_navigation_graph_edge_queries;
