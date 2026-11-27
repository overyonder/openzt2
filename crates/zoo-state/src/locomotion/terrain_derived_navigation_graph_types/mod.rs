use crate::locomotion::locomotion_types::NavFlags;
use bevy::prelude::*;
use std::collections::{BTreeMap, BTreeSet};

/// Pathfinding graph derived from the selected terrain grid and refreshed
/// from authoritative terrain samples after edits.
#[derive(Debug)]
pub struct TerrainDerivedNavigationGraph {
    pub terrain_cell_size_centimetres: u16,
    pub terrain_cells_per_tile: u16,
    pub terrain_tile_size_centimetres: u32,
    pub terrain_tile_grid_dimensions: [u32; 2],
    pub navigation_nodes: Box<[TerrainDerivedNavigationGraphNode]>,
    pub(super) terrain_neighbors: Box<[[u32; 4]]>,
    pub(super) terrain_centres_xz: Box<[[i32; 2]]>,

    pub(super) path_surface_edges: BTreeMap<u32, Vec<TerrainDerivedNavigationGraphEdge>>,
    pub(super) path_surfaces: Vec<Option<NavigationPathSurface>>,
    pub(super) available_path_slots: BTreeSet<usize>,
    pub(super) elevated_path_nodes: BTreeMap<Entity, u32>,
    pub(super) path_surfaces_by_cell: BTreeMap<[u32; 2], BTreeMap<Entity, u32>>,
    pub(super) path_nodes_by_edge: BTreeMap<PathEdgeKey, BTreeSet<u32>>,
    pub(super) ground_cells_by_path: BTreeMap<Entity, Box<[u32]>>,
    pub(super) ground_path_count_by_node: BTreeMap<u32, usize>,
}

/// Geometry projected from a path entity into the navigation graph.
#[derive(Debug, Clone, PartialEq)]
pub struct NavigationPathSurface {
    pub(super) entity: Entity,
    pub(super) centre: Vec3,
    pub(super) corners: [Vec3; 4],
    pub(super) node: TerrainDerivedNavigationGraphNode,
}

pub(super) type PathEdgeKey = ([i32; 3], [i32; 3]);

#[derive(Component, Debug, Clone, PartialEq)]
pub enum NavigationPathGeometry {
    Ground(NavigationPathSurface),
    Elevated(NavigationPathSurface),
}

#[derive(Default)]
pub struct NavigationPathSurfaceChanges {
    pub ground_nodes: BTreeSet<u32>,
    pub elevated_nodes: BTreeSet<u32>,
    pub affected_cells: BTreeSet<[u32; 2]>,
    pub affected_edges: BTreeMap<[u32; 2], NavigationEdgeChange>,
}

#[derive(Clone, Copy, Debug)]
pub struct NavigationTerrainCellEdit {
    pub cell: UVec2,
    pub height_centimetres: i32,
    pub minimum_triangle_normal_y: f32,
    pub contains_water: bool,
    pub water_depth_centimetres: u16,
}

pub type NavigationDirectedEdges = BTreeMap<[u32; 2], TerrainDerivedNavigationGraphEdge>;

#[derive(Default)]
pub struct NavigationTerrainChanges {
    pub affected_nodes: BTreeMap<u32, [TerrainDerivedNavigationGraphNode; 2]>,
    pub affected_edges: BTreeMap<[u32; 2], NavigationEdgeChange>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NavigationEdgeChange {
    pub before: Option<TerrainDerivedNavigationGraphEdge>,
    pub after: Option<TerrainDerivedNavigationGraphEdge>,
}

impl NavigationEdgeChange {
    /// Cost direction before applying an actor's traversal policy.
    pub fn cost_direction(self) -> std::cmp::Ordering {
        use std::cmp::Ordering;

        match (self.before, self.after) {
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            _ => (),
        }
        let cost = |edge: Option<TerrainDerivedNavigationGraphEdge>| {
            edge.map_or((true, u32::MAX, true), |edge| {
                (
                    edge.edge_flags.0
                        & TerrainDerivedNavigationGraphEdgeFlags::DISABLED_BY_DEFAULT.0
                        != 0,
                    edge.traversal_cost_millimetres,
                    edge.edge_flags.0 & TerrainDerivedNavigationGraphEdgeFlags::PREFERRED.0 == 0,
                )
            })
        };
        cost(self.after).cmp(&cost(self.before))
    }
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
