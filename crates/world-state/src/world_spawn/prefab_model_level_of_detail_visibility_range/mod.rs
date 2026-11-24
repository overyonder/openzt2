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
/// extent, at the distances its object authors (`switch`, `minSwitch`) rather
/// than the model's.
#[derive(Component, Debug, Default, Clone, PartialEq)]
pub struct SimpleLodSwitching(pub Vec<f32>);

/// A `SimpleLodSwitching` level's distances, in place of its model's.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct SimpleLodRange {
    pub near_m: f32,
    pub far_m: f32,
}

/// Each level's near and far distance, in the order of `authored_far_m`. A
/// level ranks by its authored far extent, and `zt.exe` shows the highest
/// rank whose distance the camera is at or past (`FUN_00745071`), so a level
/// shows from its own distance until a higher rank's. A level past the given
/// distances, or one a higher rank always outranks, never shows.
pub fn simple_lod_ranges(authored_far_m: &[f32], distances_m: &[f32]) -> Vec<(f32, f32)> {
    let mut by_far = (0..authored_far_m.len()).collect::<Vec<_>>();
    by_far.sort_by(|a, b| authored_far_m[*a].total_cmp(&authored_far_m[*b]));
    let start = |rank: usize| distances_m.get(rank).copied().unwrap_or(f32::INFINITY);
    let mut ranges = vec![(0.0, 0.0); authored_far_m.len()];
    for (rank, level) in by_far.iter().enumerate() {
        let far = (rank + 1..by_far.len())
            .map(start)
            .fold(f32::INFINITY, f32::min);
        ranges[*level] = (start(rank), far);
    }
    ranges
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_take_their_distances_in_order_of_their_authored_far_extent() {
        // As the keeper's head authors them: LOD2, LOD1, LOD0, LOD4, LOD3.
        let ranges = simple_lod_ranges(
            &[3.0, 2.0, 1.0, 999_999.0, 4.0],
            &[0.0, 15.0, 40.0, 65.0, 90.0, 115.0],
        );
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

    #[test]
    fn a_level_past_the_authored_switch_outranks_the_ones_before() {
        // `switch="0 36"` on a model with three levels: the third switches
        // at 0, so it always shows.
        let ranges = simple_lod_ranges(&[1.0, 2.0, 3.0], &[0.0, 36.0, 0.0]);
        assert_eq!(ranges, [(0.0, 0.0), (36.0, 0.0), (0.0, f32::INFINITY)]);
    }
}
