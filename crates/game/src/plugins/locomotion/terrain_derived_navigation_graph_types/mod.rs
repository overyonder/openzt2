use super::locomotion_types::NavFlags;

/// Pathfinding graph derived from the selected terrain grid and refreshed
/// from authoritative terrain samples after edits.
#[derive(Debug)]
pub(super) struct TerrainDerivedNavigationGraph {
    pub(super) terrain_cell_size_centimetres: u16,
    pub(super) terrain_cells_per_tile: u16,
    pub(super) terrain_tile_size_centimetres: u32,
    pub(super) terrain_tile_grid_dimensions: [u32; 2],
    pub(super) navigation_nodes: Box<[TerrainDerivedNavigationGraphNode]>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct TerrainDerivedNavigationGraphNode {
    pub(super) height_centimetres: i32,
    pub(super) clearance_centimetres: u16,
    pub(super) water_depth_centimetres: u16,
    pub(super) eligibility_flags: NavFlags,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct TerrainDerivedNavigationGraphEdgeFlags(pub(super) u8);

impl TerrainDerivedNavigationGraphEdgeFlags {
    pub(super) const DISABLED_BY_DEFAULT: Self = Self(1 << 1);
    pub(super) const PREFERRED: Self = Self(1 << 2);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TerrainDerivedNavigationGraphTraversalKind {
    Ground,
    Path,
    Swim,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct TerrainDerivedNavigationGraphEdge {
    pub(super) destination_node: u32,
    pub(super) traversal_cost_millimetres: u32,
    pub(super) width_centimetres: u16,
    pub(super) traversal_kind: TerrainDerivedNavigationGraphTraversalKind,
    pub(super) edge_flags: TerrainDerivedNavigationGraphEdgeFlags,
}
