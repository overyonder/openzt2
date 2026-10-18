use bevy::prelude::Vec2;
use openzt2_zoo_state::terrain::terrain_water_shoreline_geometry_types::TerrainWaterStitchGeometry;

const QUAD: [u32; 6] = [0, 1, 2, 0, 2, 3];
const FAN: [u32; 9] = [0, 1, 2, 0, 2, 3, 0, 3, 4];

pub(super) fn append_selector_stitch_side(
    geometry: &mut TerrainWaterStitchGeometry,
    center: [f32; 3],
    half_span: f32,
    side: usize,
    code: u8,
) -> Option<()> {
    let left = Vec2::new(-half_span, half_span);
    let right = Vec2::splat(half_span);
    let bottom_left = -right;
    let bottom_right = -left;
    if matches!(code, 0 | 3 | 4 | 7 | 8 | 9 | 10 | 15 | 17 | 22 | 24) {
        let offset = match code {
            7 => Vec2::splat(0.24),
            8 => Vec2::new(-0.24, 0.24),
            17 => Vec2::splat(0.55),
            24 => Vec2::new(-0.55, 0.55),
            _ => Vec2::ZERO,
        };
        emit(geometry, center, side, &[offset, right, left], &[0, 1, 2])?;
    }
    match code {
        0 | 7 | 8 | 12 | 14 | 17 | 19 | 21 | 24 => {}
        1 => emit(
            geometry,
            center,
            side,
            &[
                Vec2::ZERO,
                left,
                left + Vec2::new(1.21, -0.25),
                left + Vec2::new(2.15, -0.85),
            ],
            &QUAD,
        )?,
        2 => emit(
            geometry,
            center,
            side,
            &[
                Vec2::ZERO,
                right,
                right + Vec2::new(-1.21, -0.25),
                right + Vec2::new(-2.15, -0.85),
            ],
            &QUAD,
        )?,
        3 => emit(
            geometry,
            center,
            side,
            &[
                left,
                right,
                right + Vec2::new(-1.21, 0.25),
                right + Vec2::new(-2.15, 0.85),
            ],
            &QUAD,
        )?,
        4 => emit(
            geometry,
            center,
            side,
            &[
                right,
                left,
                left + Vec2::new(1.21, 0.25),
                left + Vec2::new(2.15, 0.85),
            ],
            &QUAD,
        )?,
        5 | 6 | 9 | 10 => {
            let (corner, shift, first, second) = match code {
                5 => (
                    right,
                    Vec2::new(0.24, -0.24),
                    Vec2::new(-0.82, 0.82),
                    Vec2::new(-1.24, 1.24),
                ),
                6 => (
                    left,
                    Vec2::splat(-0.24),
                    Vec2::splat(0.82),
                    Vec2::splat(1.24),
                ),
                9 => (
                    left,
                    Vec2::new(0.24, -0.24),
                    Vec2::new(-0.82, 0.82),
                    Vec2::new(-1.24, 1.24),
                ),
                10 => (
                    right,
                    Vec2::splat(-0.24),
                    Vec2::splat(0.82),
                    Vec2::splat(1.24),
                ),
                _ => return None,
            };
            let signs = match code {
                5 => Vec2::ONE,
                6 => Vec2::new(-1.0, 1.0),
                9 => Vec2::NEG_ONE,
                10 => Vec2::new(1.0, -1.0),
                _ => return None,
            };
            emit(
                geometry,
                center,
                side,
                &[
                    corner,
                    shift,
                    shift + first + signs * 0.15,
                    shift + second + signs * 0.5,
                ],
                &QUAD,
            )?;
        }
        11 => emit(
            geometry,
            center,
            side,
            &[
                bottom_left,
                right,
                right + Vec2::new(-1.56, -0.15),
                right + Vec2::new(-3.0, -0.59),
            ],
            &QUAD,
        )?,
        18 => emit(
            geometry,
            center,
            side,
            &[
                bottom_right,
                left,
                left + Vec2::new(1.56, -0.15),
                left + Vec2::new(3.0, -0.59),
            ],
            &QUAD,
        )?,
        13 => emit(
            geometry,
            center,
            side,
            &[
                right,
                bottom_right,
                bottom_right + Vec2::new(-0.95, 0.95),
                bottom_right + Vec2::new(-1.9, 1.9) + Vec2::splat(0.1),
                bottom_right + Vec2::new(-2.75, 2.75) + Vec2::splat(0.25),
            ],
            &FAN,
        )?,
        20 => emit(
            geometry,
            center,
            side,
            &[
                left,
                bottom_left,
                bottom_left + Vec2::splat(0.95),
                bottom_left + Vec2::splat(1.9) + Vec2::new(-0.1, 0.1),
                bottom_left + Vec2::splat(2.75) + Vec2::new(-0.25, 0.25),
            ],
            &FAN,
        )?,
        15 => emit(
            geometry,
            center,
            side,
            &[
                left,
                right,
                right + Vec2::new(-1.56, 0.15),
                right + Vec2::new(-3.0, 0.59),
            ],
            &QUAD,
        )?,
        22 => emit(
            geometry,
            center,
            side,
            &[
                right,
                left,
                left + Vec2::new(1.56, 0.15),
                left + Vec2::new(3.0, 0.59),
            ],
            &QUAD,
        )?,
        16 | 23 => {
            let (first, last, shift, sign) = if code == 16 {
                (right, left, Vec2::new(0.55, -0.55), -1.0)
            } else {
                (left, right, Vec2::splat(-0.55), 1.0)
            };
            emit(
                geometry,
                center,
                side,
                &[
                    first,
                    shift,
                    shift + Vec2::new(sign * 0.95, 0.95) + Vec2::new(sign * 0.1, -0.1),
                    shift + Vec2::new(sign * 1.8, 1.8) + Vec2::new(sign * 0.25, -0.25),
                    last,
                ],
                &FAN,
            )?;
        }
        _ => return None,
    }
    Some(())
}

fn emit(
    geometry: &mut TerrainWaterStitchGeometry,
    center: [f32; 3],
    side: usize,
    points: &[Vec2],
    indices: &[u32],
) -> Option<()> {
    let base = u32::try_from(geometry.positions.len()).ok()?;
    for &point in points {
        let mut point = point;
        for _ in 0..side {
            point = Vec2::new(point.y, -point.x);
        }
        geometry
            .positions
            .push([center[0] + point.x, center[1] + point.y, center[2]]);
    }
    geometry
        .indices
        .extend(indices.iter().map(|index| base + *index));
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eastern_selector_quad_rotates_about_tile_center() -> Result<(), &'static str> {
        let mut geometry = TerrainWaterStitchGeometry {
            positions: Vec::new(),
            indices: Vec::new(),
        };
        append_selector_stitch_side(&mut geometry, [10.0, 20.0, 3.0], 2.0, 1, 1)
            .ok_or("selector rejected")?;
        let expected = [
            [10.0, 20.0, 3.0],
            [12.0, 22.0, 3.0],
            [11.75, 20.79, 3.0],
            [11.15, 19.85, 3.0],
        ];
        for (actual, expected) in geometry.positions.iter().zip(expected) {
            for (actual, expected) in actual.iter().zip(expected) {
                assert!((actual - expected).abs() < 0.00001);
            }
        }
        assert_eq!(geometry.indices, [0, 1, 2, 0, 2, 3]);
        Ok(())
    }
}
