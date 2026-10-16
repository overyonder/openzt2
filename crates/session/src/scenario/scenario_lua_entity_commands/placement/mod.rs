//! Scripted arrivals use the live construction footprint, fence topology and
//! physics owners, including geometry not represented by an animal's radius.
use avian3d::prelude::{Collider, Sensor, SpatialQuery, SpatialQueryFilter};
use bevy::{ecs::system::SystemParam, prelude::*};
use openzt2_zoo_state::{
    placement::placement_occupancy_index::PlacedObjectFootprintOccupancyIndex,
    terrain::terrain_collision_rebuilding_types::TerrainChunkCollisionSurface,
    topology::{
        gate_operation_types::Gate,
        topology_graph_types::{FenceEdge, TopologyGrid, TopologyIndex},
        topology_grid_geometry::horizontal_world_segments_intersect,
    },
};

#[derive(SystemParam)]
pub(super) struct ScriptedAnimalPlacementObstacles<'w, 's> {
    occupancy: Res<'w, PlacedObjectFootprintOccupancyIndex>,
    topology: Res<'w, TopologyIndex>,
    grid: Res<'w, TopologyGrid>,
    fences: Query<'w, 's, (&'static FenceEdge, Option<&'static Gate>)>,
    spatial: SpatialQuery<'w, 's>,
    nonblocking: Query<'w, 's, (), Or<(With<TerrainChunkCollisionSurface>, With<Sensor>)>>,
}

impl ScriptedAnimalPlacementObstacles<'_, '_> {
    pub(super) fn overlaps(
        &self,
        position: Vec3,
        radius: f32,
        height: f32,
        definitions: openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView<'_>,
        actor_types: &[openzt2_game_data::AssetId],
    ) -> bool {
        let center = position.xz();
        let min = center - Vec2::splat(radius);
        let max = center + Vec2::splat(radius);
        // Footprint cells are metre-spaced, centred on integer coordinates.
        // Cover the whole animal envelope, conservatively including boundary cells.
        let first = (min - Vec2::splat(0.5)).ceil().as_ivec2();
        let last = (max + Vec2::splat(0.5)).floor().as_ivec2();
        for y in first.y..=last.y {
            for x in first.x..=last.x {
                if self
                    .occupancy
                    .entities_occupying_cell(IVec2::new(x, y))
                    .next()
                    .is_some()
                {
                    return true;
                }
            }
        }
        let corners = [min, Vec2::new(max.x, min.y), max, Vec2::new(min.x, max.y)];
        for (edge, entity) in &self.topology.edges {
            let Ok((fence, gate)) = self.fences.get(*entity) else {
                return true;
            };
            let Some(definition) = definitions.find_fence(fence.definition) else {
                return true;
            };
            if !definition.blocks.contains_all(openzt2_game_data::world_definitions::fences_and_gates::FenceTraversalBlockingFlags::ANIMAL) {
                continue;
            }
            if gate.is_some_and(|gate| {
                gate.open
                    && definition
                        .gate_policy
                        .open_passage_types
                        .as_ref()
                        .is_none_or(|types| {
                            types
                                .iter()
                                .any(|exception| actor_types.contains(&exception.entity_type))
                        })
            }) {
                continue;
            }
            let a = self.grid.cell_translation(edge.lo).xz();
            let b = self.grid.cell_translation(edge.hi).xz();
            let inside = |point: Vec2| point.cmpge(min).all() && point.cmple(max).all();
            if inside(a)
                || inside(b)
                || (0..4).any(|index| {
                    horizontal_world_segments_intersect(
                        a,
                        b,
                        corners[index],
                        corners[(index + 1) % 4],
                    )
                })
            {
                return true;
            }
        }
        let shape = Collider::cylinder(radius, height);
        let mut blocked = false;
        self.spatial.shape_intersections_callback(
            &shape,
            position + Vec3::Y * (height * 0.5),
            Quat::IDENTITY,
            &SpatialQueryFilter::DEFAULT,
            |entity| {
                if self.nonblocking.contains(entity) {
                    return true;
                }
                blocked = true;
                false
            },
        );
        blocked
    }
}
