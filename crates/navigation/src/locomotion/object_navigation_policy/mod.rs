use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_world_state::world_spawn::world_membership_types::DefinitionId;
use openzt2_zoo_state::locomotion::locomotion_types::NavFlags;
use openzt2_zoo_state::locomotion::terrain_derived_navigation_graph_types::TerrainDerivedNavigationGraph;
use openzt2_zoo_state::placement::placement_occupancy_index::PlacedObjectFootprintOccupancyIndex;

/// Placed objects whose authored collision keeps walkers off their footprint,
/// such as a trough or a building.
#[derive(SystemParam)]
pub(super) struct ObjectNavigationPolicy<'w, 's> {
    occupancy: Option<Res<'w, PlacedObjectFootprintOccupancyIndex>>,
    objects: Query<'w, 's, &'static DefinitionId>,
    definition_assets: Res<'w, Assets<WorldDefinitionAsset>>,
    definitions: Res<'w, WorldDefinitions>,
}

impl ObjectNavigationPolicy<'_, '_> {
    /// Whether an object on the footprint cell holding a world point blocks a
    /// walker of these capabilities.
    fn blocks_at(&self, point: Vec3, capabilities: NavFlags) -> bool {
        let definitions = self.definitions.get(&self.definition_assets);
        let (Some(occupancy), Some(definitions)) = (self.occupancy.as_deref(), definitions) else {
            return false;
        };
        occupancy
            .entities_occupying_cell(footprint_cell(point))
            .any(|entity| {
                self.objects
                    .get(entity)
                    .ok()
                    .and_then(|definition| definitions.find_object(definition.0))
                    .is_some_and(|object| {
                        object.movement_blocking.raw_flag_bits()
                            & capabilities.0
                            & NavFlags::ELIGIBILITY_BITS
                            != 0
                    })
            })
    }

    /// Whether a graph node lies on a blocking object, other than next to the
    /// walker's start or destination: a trough's dock lies inside its
    /// footprint, and an animal saved standing on a trough must step off it.
    /// A path never does: the original keeps blocking objects off paths, and a
    /// footprint derived from a stand's model bounds may reach over the path
    /// beside it.
    pub(super) fn node_is_blocked(
        &self,
        navigation: &TerrainDerivedNavigationGraph,
        node: u32,
        start: Vec3,
        destination: Vec3,
        capabilities: NavFlags,
    ) -> bool {
        navigation.path_surface_at_node(node).is_none()
            && navigation
                .calculate_navigation_node_world_position_centimetres(node)
                .is_some_and(|[x, y, z]| {
                    let point = Vec3::new(x as f32, y as f32, z as f32) * 0.01;
                    let cell = footprint_cell(point);
                    let near = |end: Vec3| (cell - footprint_cell(end)).abs().max_element() <= 1;
                    !near(start) && !near(destination) && self.blocks_at(point, capabilities)
                })
    }
}

/// A placed object's footprint cell `c` covers world `[c, c + 1)` metres on x and z.
fn footprint_cell(point: Vec3) -> IVec2 {
    IVec2::new(point.x.floor() as i32, point.z.floor() as i32)
}
