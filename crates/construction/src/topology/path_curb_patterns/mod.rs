use bevy::prelude::*;
use openzt2_game_assets::world_definitions::path_placement_highlight_asset::PathPlacementHighlightAsset;

type CurbEdge = [IVec3; 2];

pub(super) fn curb_pair(
    rules: &PathPlacementHighlightAsset,
    centre: IVec3,
    direction: u8,
    half_width: i32,
    occupied: impl Fn(IVec3) -> bool,
) -> Option<[CurbEdge; 2]> {
    let turns = ((direction + 2) % 8) / 2;
    let resolve = |[x, y, side]: [i32; 3]| {
        let mut tile = IVec2::new(x, y);
        for _ in 0..turns {
            tile = IVec2::new(tile.y, -tile.x);
        }
        (
            centre + IVec3::new(tile.x, -tile.y, 0) * (half_width * 4),
            (side as u8 + turns * 2) % 8,
        )
    };
    let present = |triangle| {
        let (tile, side) = resolve(triangle);
        occupied(tile + (compass(side) * half_width).extend(0))
    };
    let curve = rules.curves().iter().find(|curve| {
        curve.on.iter().copied().all(present) && !curve.off.iter().copied().any(present)
    })?;
    let [a, b] = curve.curb_endpoints;
    let edge = |triangle| {
        let (tile, side) = resolve(triangle);
        let (start, end) = match side {
            0 => (IVec2::new(-1, -1), IVec2::new(1, -1)),
            1 => (IVec2::new(1, -1), IVec2::new(-1, 1)),
            2 => (IVec2::new(1, -1), IVec2::new(1, 1)),
            3 => (IVec2::ONE, IVec2::new(-1, -1)),
            4 => (IVec2::new(-1, 1), IVec2::ONE),
            5 => (IVec2::new(-1, 1), IVec2::new(1, -1)),
            6 => (IVec2::new(-1, -1), IVec2::new(-1, 1)),
            _ => (IVec2::new(-1, -1), IVec2::ONE),
        };
        [start, end].map(|offset| tile + (offset * (half_width * 2)).extend(0))
    };
    Some([edge(a?), edge(b?)])
}

fn compass(direction: u8) -> IVec2 {
    [
        IVec2::NEG_Y,
        IVec2::new(1, -1),
        IVec2::X,
        IVec2::ONE,
        IVec2::Y,
        IVec2::new(-1, 1),
        IVec2::NEG_X,
        IVec2::new(-1, -1),
    ][usize::from(direction)]
}
