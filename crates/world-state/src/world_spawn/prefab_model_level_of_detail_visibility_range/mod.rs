use bevy::prelude::*;

#[derive(Component, Debug, Clone, Copy, PartialEq)]
#[relationship(relationship_target = PrefabLodRanges, allow_self_referential)]
pub struct PrefabModelLevelOfDetailVisibilityRange {
    #[relationship]
    pub group: Entity,
    pub ordinal: u16,
    pub center_m: Vec3,
    pub near_m: f32,
    pub far_m: f32,
    pub active_without_range: bool,
}

#[derive(Component, Debug, Deref)]
#[relationship_target(relationship = PrefabModelLevelOfDetailVisibilityRange)]
pub struct PrefabLodRanges(Vec<Entity>);

/// A presentation whose LOD levels switch as the original's
/// `BFSimpleLODComponent` switches them: in the order of their authored far
/// extent, at the game's own distances rather than the model's.
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct SimpleLodSwitching;

/// A `SimpleLodSwitching` level's distances, in place of its model's.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct SimpleLodRange {
    pub near_m: f32,
    pub far_m: f32,
}

/// The camera distance, in metres, from which each level of a
/// `BFSimpleLODComponent` model shows. `zt.exe` sets these squared in its LOD
/// interface's constructor (`FUN_00744fba`) and only orders a model's levels
/// by their authored far extents (`FUN_007bcd10`).
pub const SIMPLE_LOD_LEVEL_DISTANCES_M: [f32; 6] = [0.0, 15.0, 40.0, 65.0, 90.0, 115.0];

/// Each level's near and far distance, in the order of `authored_far_m`. A
/// level ranks by its authored far extent; one past the known distances never
/// shows.
pub fn simple_lod_ranges(authored_far_m: &[f32]) -> Vec<(f32, f32)> {
    let mut by_far = (0..authored_far_m.len()).collect::<Vec<_>>();
    by_far.sort_by(|a, b| authored_far_m[*a].total_cmp(&authored_far_m[*b]));
    let start = |rank: usize| {
        SIMPLE_LOD_LEVEL_DISTANCES_M
            .get(rank)
            .copied()
            .unwrap_or(f32::INFINITY)
    };
    let mut ranges = vec![(0.0, 0.0); authored_far_m.len()];
    for (rank, level) in by_far.iter().enumerate() {
        let far = if rank + 1 == by_far.len() {
            f32::INFINITY
        } else {
            start(rank + 1)
        };
        ranges[*level] = (start(rank), far);
    }
    ranges
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_take_the_game_distances_in_order_of_their_authored_far_extent() {
        // As the keeper's head authors them: LOD2, LOD1, LOD0, LOD4, LOD3.
        let ranges = simple_lod_ranges(&[3.0, 2.0, 1.0, 999_999.0, 4.0]);
        assert_eq!(
            ranges,
            [
                (40.0, 65.0),
                (15.0, 40.0),
                (0.0, 15.0),
                (90.0, f32::INFINITY),
                (65.0, 90.0)
            ]
        );
    }
}
