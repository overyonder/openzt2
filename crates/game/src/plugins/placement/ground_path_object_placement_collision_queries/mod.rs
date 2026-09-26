//! Ground path tiles that collide with object placement footprint cells.

use bevy::{ecs::system::SystemParam, prelude::*};
use openzt2_game_data::world_definitions::object_placement::PlaceableDefinition;

use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use crate::plugins::topology::topology_graph_types::{PathTile, TopologyGrid, TopologyIndex};

// Path tiles that only share an edge with a footprint cell do not collide.
const PATH_TILE_EDGE_CONTACT_TOLERANCE_M: f32 = 0.001;

#[derive(SystemParam)]
pub(super) struct GroundPathTilesUnderObjectPlacement<'w, 's> {
    topology: Res<'w, TopologyIndex>,
    grid: Res<'w, TopologyGrid>,
    paths: Query<'w, 's, &'static PathTile>,
}

impl<'w, 's> GroundPathTilesUnderObjectPlacement<'w, 's> {
    /// Returns the per-cell collision test for one placed definition. Only
    /// definitions lowered with `ground_paths_block_placement` can collide.
    pub(super) fn object_placement_cell_collides_with_ground_path<'a>(
        &'a self,
        definition: &PlaceableDefinition,
        catalogue: WorldDefinitionsView<'a>,
    ) -> impl Fn(IVec2) -> bool + use<'a, 'w, 's> {
        let spacing_m = self.grid.spacing_m;
        let applies = definition.ground_paths_block_placement
            && spacing_m.is_finite()
            && spacing_m > 0.0;
        let search_reach_m = if applies {
            catalogue
                .paths()
                .filter(|path| !path.elevated)
                .map(|path| f32::from(path.width_cm) * 0.005)
                .fold(0.0, f32::max)
        } else {
            0.0
        };
        move |cell| {
            if !applies {
                return false;
            }
            let cell_minimum = cell.as_vec2();
            let cell_maximum = cell_minimum + Vec2::ONE;
            let first = ((cell_minimum - Vec2::splat(search_reach_m) - self.grid.origin)
                / spacing_m)
                .floor()
                .as_ivec2();
            let last = ((cell_maximum + Vec2::splat(search_reach_m) - self.grid.origin)
                / spacing_m)
                .ceil()
                .as_ivec2();
            (first.x..=last.x)
                .flat_map(|x| (first.y..=last.y).map(move |y| IVec3::new(x, y, 0)))
                .filter_map(|path_cell| self.topology.paths.get(&path_cell))
                .filter_map(|path| self.paths.get(*path).ok())
                .any(|tile| {
                    catalogue.find_path(tile.definition).is_some_and(|path| {
                        let half_width_m = f32::from(path.width_cm)
                            .mul_add(0.005, -PATH_TILE_EDGE_CONTACT_TOLERANCE_M);
                        let centre = self.grid.cell_translation(tile.cell).xz();
                        !path.elevated
                            && (centre - Vec2::splat(half_width_m))
                                .cmplt(cell_maximum)
                                .all()
                            && (centre + Vec2::splat(half_width_m))
                                .cmpgt(cell_minimum)
                                .all()
                    })
                })
        }
    }
}
