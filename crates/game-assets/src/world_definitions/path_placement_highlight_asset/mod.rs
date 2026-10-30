use bevy::prelude::*;

#[derive(Asset, TypePath)]
pub struct PathPlacementHighlightAsset {
    pub(super) fallback: Handle<Image>,
    pub(super) curves: Vec<PathPlacementHighlightCurve>,
}

pub(super) struct PathPlacementHighlightCurve {
    pub(super) texture: Handle<Image>,
    pub(super) on: Vec<[i32; 3]>,
    pub(super) off: Vec<[i32; 3]>,
}

impl PathPlacementHighlightAsset {
    pub fn texture_for_direction_cell(
        &self,
        centre: IVec3,
        direction: u8,
        half_width_cells: i32,
        occupied: impl Fn(IVec3) -> bool,
    ) -> Handle<Image> {
        let turns = ((direction + 2) % 8) / 2;
        let present = |requirement: &[i32; 3]| {
            let side = match requirement[2] {
                0 => IVec2::Y,
                2 => IVec2::X,
                4 => -IVec2::Y,
                6 => -IVec2::X,
                _ => return false,
            };
            let mut offset = IVec2::new(requirement[0], requirement[1]) * half_width_cells * 4
                + side * half_width_cells;
            for _ in 0..turns {
                offset = IVec2::new(offset.y, -offset.x);
            }
            occupied(centre + IVec3::new(offset.x, -offset.y, 0))
        };
        self.curves
            .iter()
            .find(|curve| curve.on.iter().all(present) && !curve.off.iter().any(present))
            .map_or_else(|| self.fallback.clone(), |curve| curve.texture.clone())
    }
}
