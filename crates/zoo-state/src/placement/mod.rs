use bevy::prelude::*;

pub mod object_placement_definition_queries;
pub mod object_placement_headroom_diagnostic_types;
pub mod object_placement_validation;
pub mod placed_object_types;
pub mod placement_preview_types;
pub mod placement_transaction_types;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ObjectPlacementPreviewUpdateSet;

pub mod placement_occupancy_index;
