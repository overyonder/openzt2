use bevy::{ecs::entity::EntityHashMap, platform::collections::HashMap};
use bevy::{ecs::query::QueryData, prelude::*};
use openzt2_game_data::AssetId;

use super::topology_spatial_index_lifecycle::{
    discard_indexed_fence_edge, discard_indexed_path_tile, discard_indexed_topology_node,
    index_inserted_fence_edge, index_inserted_path_tile, index_inserted_topology_node,
};

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
#[component(immutable, on_insert = index_inserted_topology_node, on_discard = discard_indexed_topology_node)]
pub struct TopologyNode {
    pub cell: IVec3,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
#[component(on_insert = index_inserted_fence_edge, on_discard = discard_indexed_fence_edge)]
pub struct FenceEdge {
    pub definition: AssetId,
}

impl FenceEdge {
    pub fn between(definition: AssetId, a: Entity, b: Entity) -> (FenceStart, FenceEnd, Self) {
        (FenceStart(a), FenceEnd(b), Self { definition })
    }
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
#[relationship(relationship_target = FencesStartingHere)]
pub struct FenceStart(pub Entity);

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
#[relationship(relationship_target = FencesEndingHere)]
pub struct FenceEnd(pub Entity);

#[derive(Component, Debug)]
#[relationship_target(relationship = FenceStart, linked_spawn)]
pub struct FencesStartingHere(Vec<Entity>);

#[derive(Component, Debug)]
#[relationship_target(relationship = FenceEnd, linked_spawn)]
pub struct FencesEndingHere(Vec<Entity>);

#[derive(QueryData)]
pub struct FenceSegment {
    pub edge: &'static FenceEdge,
    pub start: &'static FenceStart,
    pub end: &'static FenceEnd,
}

impl FenceSegmentItem<'_, '_> {
    pub fn a(&self) -> Entity {
        self.start.0
    }
    pub fn b(&self) -> Entity {
        self.end.0
    }
}

impl std::ops::Deref for FenceSegmentItem<'_, '_> {
    type Target = FenceEdge;

    fn deref(&self) -> &Self::Target {
        self.edge
    }
}

/// Authored scenario topology that construction tools may inspect but not
/// remove. Protection is live interaction state owned by the edge entity.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TopologyProtected;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
#[component(immutable, on_insert = index_inserted_path_tile, on_discard = discard_indexed_path_tile)]
pub struct PathTile {
    pub definition: AssetId,
    pub cell: IVec3,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
#[relationship(relationship_target = IncomingPortals)]
pub struct Portal {
    #[relationship]
    pub destination: Entity,
    pub bidirectional: bool,
}

#[derive(Component, Debug)]
#[relationship_target(relationship = Portal, linked_spawn)]
pub struct IncomingPortals(Vec<Entity>);

#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct PathSupport {
    pub ground_height_m: f32,
}

/// Fixed final surface samples for a path tile. The five heights are borrowed
/// from the world-definition catalogue at construction and copied once into
/// the owning ECS entity as its immutable collision and placement facts.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TileSurface {
    pub height_cm: [i16; 5],
}

#[derive(Component, Debug, Default)]
pub struct TopologyIndex {
    pub nodes: HashMap<IVec3, Entity>,
    pub paths: HashMap<IVec3, Entity>,
    pub edges: HashMap<EdgeKey, Entity>,
    pub(super) path_columns: HashMap<IVec2, Vec<Entity>>,
    pub(super) fine_fence_steps: HashMap<EdgeKey, Vec<Entity>>,
    pub(super) fence_columns: HashMap<IVec2, Vec<Entity>>,
    pub(super) edge_keys: EntityHashMap<EdgeKey>,
}

impl TopologyIndex {
    pub fn paths_within(&self, min: IVec2, max: IVec2) -> impl Iterator<Item = Entity> + '_ {
        (min.y..=max.y).flat_map(move |y| {
            (min.x..=max.x).flat_map(move |x| {
                self.path_columns
                    .get(&IVec2::new(x, y))
                    .into_iter()
                    .flatten()
                    .copied()
            })
        })
    }

    pub fn fences_within(&self, min: IVec2, max: IVec2) -> impl Iterator<Item = Entity> + '_ {
        (min.y..=max.y).flat_map(move |y| {
            (min.x..=max.x).flat_map(move |x| {
                self.fence_columns
                    .get(&IVec2::new(x, y))
                    .into_iter()
                    .flatten()
                    .copied()
            })
        })
    }

    pub fn fences_covering_step(&self, step: EdgeKey) -> impl Iterator<Item = Entity> + '_ {
        self.fine_fence_steps
            .get(&step)
            .into_iter()
            .flatten()
            .copied()
    }

    pub fn fence_key(&self, entity: Entity) -> Option<EdgeKey> {
        self.edge_keys.get(&entity).copied()
    }
}

/// Spatial projection of integer topology cells for the active terrain.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct TopologyGrid {
    pub origin: Vec2,
    pub spacing_m: f32,
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
    pub fn cell_translation(self, cell: IVec3) -> Vec3 {
        Vec3::new(
            self.origin.x + cell.x as f32 * self.spacing_m,
            cell.z as f32,
            self.origin.y + cell.y as f32 * self.spacing_m,
        )
    }

    pub fn position_cell(self, position: Vec3) -> IVec3 {
        let horizontal = (position.xz() - self.origin) / self.spacing_m;
        IVec3::new(
            horizontal.x.round() as i32,
            horizontal.y.round() as i32,
            position.y.round() as i32,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EdgeKey {
    pub lo: IVec3,
    pub hi: IVec3,
}

impl EdgeKey {
    pub fn new(first_cell: IVec3, second_cell: IVec3) -> Option<Self> {
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

pub fn topology_cell_rectangle(cell: IVec3) -> IRect {
    IRect::from_corners(IVec2::new(cell.x, cell.y), IVec2::new(cell.x, cell.y))
}
