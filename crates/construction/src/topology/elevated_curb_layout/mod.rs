use std::collections::VecDeque;

use bevy::{ecs::system::SystemParam, platform::collections::HashMap, prelude::*};
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use openzt2_game_data::AssetId;
use openzt2_zoo_state::topology::topology_graph_types::{PathTile, TileSurface, TopologyGrid, TopologyIndex};

pub(super) fn is_elevated_curb(definitions: WorldDefinitionsView<'_>, id: AssetId) -> bool {
    definitions
        .find_fence(id)
        .and_then(|fence| definitions.find_object(fence.object))
        .is_some_and(|object| {
            object
                .behavior_type_identifiers
                .contains(&AssetId::from_key("elevatedcurb"))
        })
}

#[derive(SystemParam)]
pub(super) struct ElevatedCurbSurfaces<'w, 's> {
    tiles: Query<'w, 's, (&'static PathTile, &'static TileSurface)>,
}

impl<'w, 's> ElevatedCurbSurfaces<'w, 's> {
    fn perimeters<'a>(
        &'a self,
        cell: IVec2,
        grid: TopologyGrid,
        index: &'a TopologyIndex,
        definitions: WorldDefinitionsView<'a>,
    ) -> impl Iterator<Item = [(IVec3, f32); 4]> + use<'a, 'w, 's> {
        let width = definitions
            .paths()
            .filter(|path| path.elevated)
            .map(|path| path.width_cm)
            .max()
            .unwrap_or_default();
        let reach = IVec2::splat((f32::from(width) * 0.005 / grid.spacing_m).ceil() as i32);
        index
            .paths_within(cell - reach, cell + reach)
            .filter_map(move |entity| {
                let (tile, surface) = self.tiles.get(entity).ok()?;
                let definition = definitions
                    .find_path(tile.definition)
                    .filter(|path| path.elevated)?;
                let half = (f32::from(definition.width_cm) * 0.005 / grid.spacing_m).round() as i32;
                (half > 0).then(|| {
                    [
                        IVec2::new(-1, -1),
                        IVec2::new(1, -1),
                        IVec2::ONE,
                        IVec2::new(-1, 1),
                    ]
                    .into_iter()
                    .zip(surface.height_cm)
                    .enumerate()
                    .fold(
                        [(IVec3::ZERO, 0.0); 4],
                        |mut corners, (i, (direction, rise))| {
                            let height =
                                grid.cell_translation(tile.cell).y + f32::from(rise) * 0.01;
                            corners[i] = (
                                (tile.cell.xy() + direction * half).extend(height.round() as i32),
                                height,
                            );
                            corners
                        },
                    )
                })
            })
    }

    pub(super) fn nearest(
        &self,
        point: Vec3,
        grid: TopologyGrid,
        index: &TopologyIndex,
        definitions: WorldDefinitionsView<'_>,
    ) -> Option<IVec3> {
        let cell = grid.position_cell(point);
        self.perimeters(cell.xy(), grid, index, definitions)
            .flatten()
            .min_by(|&(a, ah), &(b, bh)| {
                let distance = |cell, height| {
                    let mut position = grid.cell_translation(cell);
                    position.y = height;
                    point.distance_squared(position)
                };
                distance(a, ah)
                    .total_cmp(&distance(b, bh))
                    .then_with(|| a.to_array().cmp(&b.to_array()))
            })
            .map(|(cell, _)| cell)
    }

    pub(super) fn height(
        &self,
        cell: IVec3,
        grid: TopologyGrid,
        index: &TopologyIndex,
        definitions: WorldDefinitionsView<'_>,
    ) -> Option<f32> {
        self.perimeters(cell.xy(), grid, index, definitions)
            .flatten()
            .filter_map(|(corner, height)| (corner == cell).then_some(height))
            .min_by(f32::total_cmp)
    }

    pub(super) fn route(
        &self,
        from: IVec3,
        to: IVec3,
        span: i32,
        grid: TopologyGrid,
        index: &TopologyIndex,
        definitions: WorldDefinitionsView<'_>,
    ) -> Option<Vec<IVec3>> {
        let mut previous = HashMap::<IVec3, IVec3>::from_iter([(from, from)]);
        let mut pending = VecDeque::from([from]);
        while let Some(cell) = pending.pop_front() {
            if cell == to {
                let mut route = vec![to];
                while *route.last()? != from {
                    route.push(previous[route.last()?]);
                }
                route.reverse();
                return Some(route);
            }
            for corners in self.perimeters(cell.xy(), grid, index, definitions) {
                let Some(corner) = corners.iter().position(|&(point, _)| point == cell) else {
                    continue;
                };
                for adjacent in [(corner + 1) % 4, (corner + 3) % 4, (corner + 2) % 4] {
                    let (next, _) = corners[adjacent];
                    if (next - cell).xy().abs().max_element() != span
                        || previous.contains_key(&next)
                    {
                        continue;
                    }
                    previous.insert(next, cell);
                    pending.push_back(next);
                }
            }
        }
        None
    }
}
