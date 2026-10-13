use bevy::prelude::*;

pub mod locomotion_types;
pub mod terrain_derived_navigation_graph_types;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AnimalAnimationNavigationProjection;

mod terrain_derived_navigation_graph_construction;

mod terrain_derived_navigation_graph_operations;

mod terrain_derived_navigation_graph_edge_queries;
