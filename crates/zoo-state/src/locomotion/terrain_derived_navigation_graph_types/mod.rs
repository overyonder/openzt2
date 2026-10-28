use crate::locomotion::locomotion_types::NavFlags;
use bevy::prelude::*;
use std::collections::BTreeMap;

/// Pathfinding graph derived from the selected terrain grid and refreshed
/// from authoritative terrain samples after edits.
#[derive(Debug)]
pub struct TerrainDerivedNavigationGraph {
    pub terrain_cell_size_centimetres: u16,
    pub terrain_cells_per_tile: u16,
    pub terrain_tile_size_centimetres: u32,
    pub terrain_tile_grid_dimensions: [u32; 2],
    pub navigation_nodes: Box<[TerrainDerivedNavigationGraphNode]>,
    pub(super) path_surfaces: Vec<NavigationPathSurface>,
    pub(super) path_surface_edges: BTreeMap<u32, Vec<TerrainDerivedNavigationGraphEdge>>,
    pub(super) path_surfaces_by_cell: BTreeMap<[u32; 2], Vec<u32>>,
    pub(super) ground_path_nodes: Vec<u32>,
}

/// Geometry projected from a path entity into the navigation graph.
#[derive(Debug, Clone, PartialEq)]
pub struct NavigationPathSurface {
    pub(super) entity: Entity,
    pub(super) transform: GlobalTransform,
    pub(super) width_m: f32,
    pub(super) height_cm: [i16; 5],
    pub(super) node: TerrainDerivedNavigationGraphNode,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TerrainDerivedNavigationGraphNode {
    pub height_centimetres: i32,
    pub minimum_triangle_normal_y: f32,
    /// Free distance from the cell centre; agents compare it with their radius.
    pub clearance_centimetres: u16,
    pub water_depth_centimetres: u16,
    pub eligibility_flags: NavFlags,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TerrainDerivedNavigationGraphEdgeFlags(pub u8);

impl TerrainDerivedNavigationGraphEdgeFlags {
    pub const DISABLED_BY_DEFAULT: Self = Self(1 << 1);
    pub const PREFERRED: Self = Self(1 << 2);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerrainDerivedNavigationGraphTraversalKind {
    Ground,
    Path,
    Swim,
    Shore,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerrainDerivedNavigationGraphEdge {
    pub destination_node: u32,
    pub traversal_cost_millimetres: u32,
    /// Corridor width between two cells; agents compare it with their diameter.
    pub width_centimetres: u16,
    pub traversal_kind: TerrainDerivedNavigationGraphTraversalKind,
    pub edge_flags: TerrainDerivedNavigationGraphEdgeFlags,
}
