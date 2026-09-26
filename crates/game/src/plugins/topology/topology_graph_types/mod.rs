use bevy::{platform::collections::HashMap, prelude::*};
use openzt2_game_data::AssetId;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TopologyNode {
    pub(crate) cell: IVec3,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FenceEdge {
    pub(crate) definition: AssetId,
    pub(crate) a: Entity,
    pub(crate) b: Entity,
}

/// Authored scenario topology that construction tools may inspect but not
/// remove. Protection is live interaction state owned by the edge entity.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TopologyProtected;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PathTile {
    pub(crate) definition: AssetId,
    pub(crate) cell: IVec3,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Portal {
    pub(crate) destination: Entity,
    pub(crate) bidirectional: bool,
}

#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub(crate) struct PathSupport {
    pub(crate) path: Entity,
    pub(crate) ground_height_m: f32,
}

/// Fixed final surface samples for a path tile. The five heights are borrowed
/// from the world-definition catalogue at construction and copied once into
/// the owning ECS entity as its immutable collision and placement facts.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TileSurface {
    pub(crate) height_cm: [i16; 5],
}

#[derive(Resource, Debug, Default)]
pub(crate) struct TopologyIndex {
    pub(crate) nodes: HashMap<IVec3, Entity>,
    pub(crate) paths: HashMap<IVec3, Entity>,
    pub(crate) edges: HashMap<EdgeKey, Entity>,
}

/// Spatial projection of integer topology cells for the active terrain.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub(crate) struct TopologyGrid {
    pub(crate) origin: Vec2,
    pub(crate) spacing_m: f32,
}

impl Default for TopologyGrid {
    fn default() -> Self {
        Self {
            origin: Vec2::ZERO,
            spacing_m: 1.0,
        }
    }
}

impl TopologyGrid {
    pub(crate) fn cell_translation(self, cell: IVec3) -> Vec3 {
        Vec3::new(
            self.origin.x + cell.x as f32 * self.spacing_m,
            cell.z as f32,
            self.origin.y + cell.y as f32 * self.spacing_m,
        )
    }

    pub(crate) fn position_cell(self, position: Vec3) -> IVec3 {
        let horizontal = (position.xz() - self.origin) / self.spacing_m;
        IVec3::new(
            horizontal.x.round() as i32,
            horizontal.y.round() as i32,
            position.y.round() as i32,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct EdgeKey {
    pub(crate) lo: IVec3,
    pub(crate) hi: IVec3,
}

impl EdgeKey {
    pub(crate) fn new(first_cell: IVec3, second_cell: IVec3) -> Option<Self> {
        if first_cell == second_cell {
            return None;
        }
        let (lo, hi) = if cell_key(first_cell) <= cell_key(second_cell) {
            (first_cell, second_cell)
        } else {
            (second_cell, first_cell)
        };
        Some(Self { lo, hi })
    }
}

fn cell_key(cell: IVec3) -> (i32, i32, i32) {
    (cell.x, cell.y, cell.z)
}

pub(crate) fn topology_cell_rectangle(cell: IVec3) -> IRect {
    IRect::from_corners(IVec2::new(cell.x, cell.y), IVec2::new(cell.x, cell.y))
}
