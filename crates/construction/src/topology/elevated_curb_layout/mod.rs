use bevy::{ecs::system::SystemParam, platform::collections::HashSet, prelude::*};
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
        let candidates = |cell: IVec3| {
            let mut slots = [None; 8];
            for corners in self.perimeters(cell.xy(), grid, index, definitions) {
                if !corners.iter().any(|&(point, _)| point == cell) {
                    continue;
                }
                for (other, _) in corners {
                    let offset = (other - cell).xy();
                    if offset.abs().max_element() != span {
                        continue;
                    }
                    let slot = direction_slot(offset.signum());
                    slots[slot] = Some(other);
                }
            }
            slots
        };
        let mut first_directions = candidates(from).into_iter().rev().flatten();
        let first = first_directions.next()?;
        // The anchor's two posts yield independent ordered walks. Select the
        // first occurrence of the pointed segment; equal indices retain the
        // walk from the anchor's first post.
        let forward = walk_curb_assignments(from, first, to, candidates);
        let reverse = first_directions
            .next()
            .and_then(|next| walk_curb_assignments(from, next, to, candidates));
        match (forward, reverse) {
            (Some(forward), Some(reverse)) if reverse.len() < forward.len() => Some(reverse),
            (Some(forward), _) => Some(forward),
            (None, reverse) => reverse,
        }
    }
}

fn direction_slot(direction: IVec2) -> usize {
    match direction.to_array() {
        [0, -1] => 0,
        [1, -1] => 1,
        [1, 0] => 2,
        [1, 1] => 3,
        [0, 1] => 4,
        [-1, 1] => 5,
        [-1, 0] => 6,
        _ => 7,
    }
}

fn walk_curb_assignments(
    anchor: IVec3,
    first: IVec3,
    target: IVec3,
    candidates: impl Fn(IVec3) -> [Option<IVec3>; 8],
) -> Option<Vec<IVec3>> {
    let mut chain = vec![anchor, first];
    let mut visited: HashSet<_> = [(anchor, first)].into_iter().collect();
    let (mut previous, mut current) = (anchor, first);
    loop {
        if current == target {
            return Some(chain);
        }
        let last_assignment = candidates(current)
            .into_iter()
            .rev()
            .flatten()
            .find(|&end| end != previous)?;
        if !visited.insert((current, last_assignment)) {
            return None;
        }
        chain.push(last_assignment);
        (previous, current) = (current, last_assignment);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curb_walk_takes_the_last_assignment_at_a_branch() {
        let origin = IVec3::ZERO;
        let branch = IVec3::X;
        let earlier = branch - IVec3::Y;
        let later = branch + IVec3::Y;
        let candidates = |cell| {
            let mut slots = [None; 8];
            if cell == branch {
                slots[0] = Some(earlier);
                slots[4] = Some(later);
                slots[6] = Some(origin);
            }
            slots
        };
        assert_eq!(
            walk_curb_assignments(origin, branch, later, candidates),
            Some(vec![origin, branch, later]),
        );
        assert_eq!(
            walk_curb_assignments(origin, branch, earlier, candidates),
            None,
        );
    }
}
