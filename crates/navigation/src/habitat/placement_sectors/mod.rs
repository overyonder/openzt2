use bevy::prelude::*;

pub fn sector_for_offset(Vec2 { x, y }: Vec2) -> u8 {
    match (y >= x, y > -x) {
        (true, true) => 0,
        (false, true) => 2,
        (false, false) => 4,
        (true, false) => 6,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PlacementSector {
    pub tile: IVec2,
    pub layer: u8,
    pub side: u8,
}

pub fn blocked_span_contains(spans: &[(i32, i32)], tile_y: i32) -> bool {
    spans
        .iter()
        .any(|&(start, end)| start <= tile_y && tile_y <= end)
}

impl PlacementSector {
    pub fn at_position(
        position: Vec2,
        layer: u8,
        reference_for: impl FnOnce(IVec2, u8) -> Option<Vec2>,
    ) -> Option<Self> {
        if !position.is_finite() {
            return None;
        }
        let tile = position.as_ivec2() / 3;
        let reference = reference_for(tile, layer)?;
        Some(Self {
            tile,
            layer,
            side: sector_for_offset(position - reference),
        })
    }

    pub fn packed(self, size: UVec2) -> u32 {
        if self.tile.cmplt(IVec2::ZERO).any() || self.tile.as_uvec2().cmpge(size).any() {
            return u32::MAX;
        }
        let tile = self.tile.as_uvec2();
        let head = (u32::from(self.side) << 8) | u32::from(self.layer);
        (head << 20) | tile.y.wrapping_mul(size.x).wrapping_add(tile.x)
    }

    pub fn expanded(self, exists: impl Fn(IVec2, u8) -> bool) -> [Option<Self>; 2] {
        if !exists(self.tile, self.layer) {
            return [None; 2];
        }
        let pair = match self.side {
            1 => [0, 2],
            3 => [4, 2],
            5 => [4, 6],
            7 => [0, 6],
            0 | 2 | 4 | 6 => {
                let adjacent = self.tile + cardinal_step(self.side);
                return [
                    Some(self),
                    exists(adjacent, self.layer).then_some(Self {
                        tile: adjacent,
                        side: (self.side + 4) % 8,
                        ..self
                    }),
                ];
            }
            _ => return [None; 2],
        };
        pair.map(|side| Some(Self { side, ..self }))
    }
}

pub(super) fn cardinal_step(side: u8) -> IVec2 {
    match side {
        0 => IVec2::NEG_Y,
        2 => IVec2::X,
        4 => IVec2::Y,
        6 => IVec2::NEG_X,
        _ => IVec2::ZERO,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sector_lookup_preserves_the_diagonal_ties() {
        assert_eq!(sector_for_offset(Vec2::new(1.0, 0.25)), 2);
        assert_eq!(sector_for_offset(Vec2::new(0.25, 1.0)), 0);
        assert_eq!(sector_for_offset(Vec2::new(1.0, 1.0)), 0);
        assert_eq!(sector_for_offset(Vec2::new(-1.0, 1.0)), 6);
    }

    #[test]
    fn span_boundaries_are_inclusive_and_missing_tiles_have_no_side() {
        let spans = [(7, 9)];
        assert!(!blocked_span_contains(&spans, 6));
        assert!(blocked_span_contains(&spans, 7));
        assert!(blocked_span_contains(&spans, 9));
        assert!(!blocked_span_contains(&spans, 10));
        assert!(PlacementSector::at_position(Vec2::splat(21.0), 0, |_, _| None).is_none());
    }

    #[test]
    fn fence_stencil_keeps_sides_and_skips_missing_neighbour() {
        let side = |side| PlacementSector {
            tile: IVec2::ZERO,
            layer: 1,
            side,
        };
        let root_only = |tile, _| tile == IVec2::ZERO;
        let pair = side(1).expanded(root_only).map(Option::unwrap);
        assert_eq!(pair, [side(0), side(2)]);
        assert_ne!(pair[0].packed(UVec2::ONE), pair[1].packed(UVec2::ONE));
        assert_eq!(side(5).expanded(root_only), [Some(side(4)), Some(side(6))]);
        assert_eq!(side(0).expanded(root_only), [Some(side(0)), None]);
    }
}
