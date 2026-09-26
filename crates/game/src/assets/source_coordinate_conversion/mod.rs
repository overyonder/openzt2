//! Canonical conversion from Blue Fang's right-handed Z-up space to Bevy's
//! right-handed Y-up space.

use bevy::math::{Mat3, Quat, Vec3};

/// Blue Fang and Gamebryo source data uses X/Y as its horizontal plane and Z
/// as up. Preserve the source handedness by mapping source positive Y to Bevy
/// negative Z: `[X, Y, Z] -> [X, Z, -Y]`.
pub(crate) const fn convert_source_z_up_vector_to_bevy_y_up_coordinates(
    source_vector: [f32; 3],
) -> [f32; 3] {
    [source_vector[0], source_vector[2], -source_vector[1]]
}

/// Conjugates a source rotation matrix through the proper source-to-Bevy basis.
pub(in crate::assets) fn conjugate_source_z_up_rotation_into_bevy_y_up_basis(
    source_row_major_rotation: [f32; 9],
) -> [f32; 9] {
    let source_axis_for_bevy_axis = [0_usize, 2, 1];
    let source_axis_sign_for_bevy_axis = [1.0_f32, 1.0, -1.0];
    std::array::from_fn(|bevy_row_major_index| {
        let bevy_row = bevy_row_major_index / 3;
        let bevy_column = bevy_row_major_index % 3;
        source_row_major_rotation
            [source_axis_for_bevy_axis[bevy_row] * 3 + source_axis_for_bevy_axis[bevy_column]]
            * source_axis_sign_for_bevy_axis[bevy_row]
            * source_axis_sign_for_bevy_axis[bevy_column]
    })
}

/// Converts an ordinary source column-vector quaternion through the proper
/// source-to-Bevy basis.
pub(in crate::assets) fn convert_source_z_up_column_vector_xyzw_quaternion_to_bevy_y_up_coordinates(
    source_quaternion: [f32; 4],
) -> [f32; 4] {
    [
        source_quaternion[0],
        source_quaternion[2],
        -source_quaternion[1],
        source_quaternion[3],
    ]
}

pub(in crate::assets) fn convert_row_major_rotation_matrix_to_xyzw_quaternion(
    source_row_major_rotation: [f32; 9],
) -> [f32; 4] {
    convert_row_major_rotation_matrix_to_quaternion(source_row_major_rotation).to_array()
}

pub(in crate::assets) fn convert_row_major_rotation_matrix_to_quaternion(
    source_row_major_rotation: [f32; 9],
) -> Quat {
    let source_rotation_quaternion = Quat::from_mat3(&row_major_rotation_array_to_glam_matrix(
        source_row_major_rotation,
    ));
    if source_rotation_quaternion.is_finite()
        && source_rotation_quaternion.length_squared() > 1.0e-12
    {
        source_rotation_quaternion.normalize()
    } else {
        Quat::IDENTITY
    }
}

fn row_major_rotation_array_to_glam_matrix(source_row_major_rotation: [f32; 9]) -> Mat3 {
    Mat3::from_cols(
        Vec3::new(
            source_row_major_rotation[0],
            source_row_major_rotation[3],
            source_row_major_rotation[6],
        ),
        Vec3::new(
            source_row_major_rotation[1],
            source_row_major_rotation[4],
            source_row_major_rotation[7],
        ),
        Vec3::new(
            source_row_major_rotation[2],
            source_row_major_rotation[5],
            source_row_major_rotation[8],
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proper_vector_basis_preserves_triangle_winding_relative_to_transformed_normal() {
        let source_positions = [Vec3::ZERO, Vec3::X, Vec3::Y];
        let converted_positions = source_positions.map(|position| {
            Vec3::from_array(convert_source_z_up_vector_to_bevy_y_up_coordinates(
                position.to_array(),
            ))
        });
        let unchanged_winding_normal = (converted_positions[1] - converted_positions[0])
            .cross(converted_positions[2] - converted_positions[0])
            .normalize();
        let converted_authored_normal = Vec3::from_array(
            convert_source_z_up_vector_to_bevy_y_up_coordinates(Vec3::Z.to_array()),
        );

        assert!(unchanged_winding_normal.abs_diff_eq(converted_authored_normal, 1.0e-6));
    }

    #[test]
    fn proper_column_vector_quaternion_preserves_source_rotation_result() {
        let source_rotation = Quat::from_rotation_z(std::f32::consts::FRAC_PI_2);
        let converted_rotation = Quat::from_array(
            convert_source_z_up_column_vector_xyzw_quaternion_to_bevy_y_up_coordinates(
                source_rotation.to_array(),
            ),
        );
        let converted_source_result =
            Vec3::from_array(convert_source_z_up_vector_to_bevy_y_up_coordinates(
                (source_rotation * Vec3::X).to_array(),
            ));

        assert!((converted_rotation * Vec3::X).abs_diff_eq(converted_source_result, 1.0e-6));
    }

    #[test]
    fn blue_fang_quaternion_matches_native_indexed_matrix_writes() {
        // BF samples are xyzw; the sampler constructs a middleware wxyz record.
        // Compare transformed vectors so matrix storage order cannot mask a
        // rotation or handedness mismatch.
        for rotation in [
            Quat::from_rotation_x(0.8),
            Quat::from_rotation_y(-1.2),
            Quat::from_rotation_z(1.1),
            Quat::from_xyzw(-0.5, -0.5, -0.5, 0.5),
        ] {
            let [x, y, z, w] = rotation.to_array();
            let native_rows = [
                1.0 - 2.0 * (z * z + y * y),
                2.0 * (x * y - w * z),
                2.0 * (x * z + w * y),
                2.0 * (x * y + w * z),
                1.0 - 2.0 * (z * z + x * x),
                2.0 * (y * z - w * x),
                2.0 * (x * z - w * y),
                2.0 * (y * z + w * x),
                1.0 - 2.0 * (y * y + x * x),
            ];
            let native_bevy = row_major_rotation_array_to_glam_matrix(
                conjugate_source_z_up_rotation_into_bevy_y_up_basis(native_rows),
            );
            let converted = Quat::from_array(
                convert_source_z_up_column_vector_xyzw_quaternion_to_bevy_y_up_coordinates(
                    rotation.to_array(),
                ),
            );
            for axis in [Vec3::X, Vec3::Y, Vec3::Z] {
                assert!((converted * axis).abs_diff_eq(native_bevy * axis, 1.0e-6));
            }
        }
    }
}
