/// Applies one side's shoreline contour to a row-major five-by-five patch.
/// Horizontal coordinates and selector side order use the source XY convention.
pub(super) fn deform_shoreline_patch(
    positions: &mut [[f32; 3]; 25],
    side: usize,
    selector_code: u8,
) {
    let offsets: &[(usize, f32, f32)] = match selector_code {
        1 => &[
            (24, -0.85, -0.85),
            (18, -0.15, -0.15),
            (19, -0.85, -0.15),
            (23, -0.15, -0.85),
            (14, -0.25, -0.29),
            (22, -0.29, -0.25),
            (9, -0.25, 0.46),
            (21, 0.46, -0.25),
        ],
        3 => &[(23, 0.75, 0.0), (22, 0.29, 0.25), (21, 0.1, 0.85)],
        4 => &[(21, -0.75, 0.0), (22, -0.29, 0.25), (23, -0.1, 0.85)],
        5 => &[
            (12, 0.24, -0.24),
            (16, 0.32, -0.02),
            (21, 0.25, 0.0),
            (20, 1.0, 0.0),
        ],
        6 => &[
            (12, -0.24, -0.24),
            (18, -0.32, -0.02),
            (23, -0.25, 0.0),
            (24, -1.0, 0.0),
        ],
        9 => &[
            (12, -0.73, 0.43),
            (17, -0.73, -0.32),
            (16, -0.75, -0.25),
            (13, -0.51, -0.24),
            (8, -0.51, 0.51),
        ],
        10 => &[
            (12, 0.73, 0.43),
            (17, 0.73, -0.32),
            (18, 0.75, -0.25),
            (11, 0.51, -0.24),
            (6, 0.51, 0.51),
        ],
        11 => &[
            (23, 0.75, 0.0),
            (22, -0.06, -0.15),
            (21, 0.69, -0.15),
            (20, 0.0, -0.6),
        ],
        18 => &[
            (21, -0.75, 0.0),
            (22, 0.06, -0.15),
            (23, -0.69, -0.15),
            (24, 0.0, -0.6),
        ],
        13 => &[(12, 0.55, -0.55), (16, 0.45, -0.25), (20, 0.5, 0.0)],
        20 => &[(12, -0.55, -0.55), (18, -0.45, -0.25), (24, -0.5, 0.0)],
        15 => &[(23, 0.75, 0.0), (22, -0.06, 0.15), (21, -0.75, 0.59)],
        22 => &[(21, -0.75, 0.0), (22, 0.06, 0.15), (23, 0.75, 0.59)],
        16 => &[(12, -0.5, 0.3), (8, -0.2, 0.2), (16, -0.75, 0.25)],
        23 => &[(12, 0.5, 0.3), (6, 0.2, 0.2), (18, 0.75, 0.25)],
        _ => return,
    };
    for &(index, mut offset_x, mut offset_y) in offsets {
        let mut x = index % 5;
        let mut y = index / 5;
        for _ in 0..side {
            (x, y) = (y, 4 - x);
            (offset_x, offset_y) = (offset_y, -offset_x);
        }
        let position = &mut positions[y * 5 + x];
        position[0] += offset_x;
        position[1] += offset_y;
        position[2] = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eastern_corner_moves_boundary_vertices_and_disables_horizontal_chop() {
        let mut positions = [[0.0, 0.0, 1.0]; 25];
        for y in 0_u16..5 {
            for x in 0_u16..5 {
                positions[usize::from(y * 5 + x)] = [f32::from(x), f32::from(y), 1.0];
            }
        }
        deform_shoreline_patch(&mut positions, 1, 1);
        assert_eq!(positions[4], [3.15, 0.85, 0.0]);
        assert_eq!(positions[8], [2.85, 1.15, 0.0]);
        assert_eq!(positions[12], [2.0, 2.0, 1.0]);
    }
}
