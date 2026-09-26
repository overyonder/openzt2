//! Blue Fang and NetImmerse source-coordinate conversion through Bevy/glam transforms.

use bevy::math::{Mat4, Vec3};

use crate::assets::source_coordinate_conversion::{
    conjugate_source_z_up_rotation_into_bevy_y_up_basis,
    convert_row_major_rotation_matrix_to_quaternion,
    convert_row_major_rotation_matrix_to_xyzw_quaternion,
    convert_source_z_up_vector_to_bevy_y_up_coordinates,
};

use super::super::netimmerse_nif_source::transform_and_skin_source_types::NetImmerseNiTransform;

pub(super) fn convert_blue_fang_source_matrix_to_gltf_matrix(
    blue_fang_source_matrix: [[f32; 4]; 4],
) -> [f32; 16] {
    let source_axis_for_bevy_axis = [0_usize, 2, 1, 3];
    let source_axis_sign_for_bevy_axis = [1.0_f32, 1.0, -1.0, 1.0];
    let mut gltf_column_major_matrix = [0.0; 16];
    for gltf_column in 0..4 {
        for gltf_row in 0..4 {
            let source_matrix_value = blue_fang_source_matrix
                [source_axis_for_bevy_axis[gltf_column]][source_axis_for_bevy_axis[gltf_row]];
            gltf_column_major_matrix[gltf_column * 4 + gltf_row] = source_matrix_value
                * source_axis_sign_for_bevy_axis[gltf_column]
                * source_axis_sign_for_bevy_axis[gltf_row];
        }
    }
    gltf_column_major_matrix
}

pub(in crate::assets::model_source) fn compose_blue_fang_row_major_source_matrices(
    left: [[f32; 4]; 4],
    right: [[f32; 4]; 4],
) -> [[f32; 4]; 4] {
    let row_major_source_matrix_to_glam = |source_matrix: [[f32; 4]; 4]| {
        Mat4::from_cols_array_2d(&std::array::from_fn(|column| {
            std::array::from_fn(|row| source_matrix[row][column])
        }))
    };
    let product = row_major_source_matrix_to_glam(left) * row_major_source_matrix_to_glam(right);
    let product_columns = product.to_cols_array_2d();
    std::array::from_fn(|row| std::array::from_fn(|column| product_columns[column][row]))
}

pub(crate) fn convert_source_transform_to_bevy_components(
    source_translation: [f32; 3],
    source_row_major_rotation: [f32; 9],
    source_uniform_scale: f32,
) -> ([f32; 3], [f32; 4], [f32; 3]) {
    (
        convert_source_z_up_vector_to_bevy_y_up_coordinates(source_translation),
        convert_row_major_rotation_matrix_to_xyzw_quaternion(
            conjugate_source_z_up_rotation_into_bevy_y_up_basis(source_row_major_rotation),
        ),
        [source_uniform_scale; 3],
    )
}

pub(super) fn convert_netimmerse_transform_to_gltf_matrix(
    netimmerse_transform: &NetImmerseNiTransform,
) -> [f32; 16] {
    let bevy_rotation = convert_row_major_rotation_matrix_to_quaternion(
        conjugate_source_z_up_rotation_into_bevy_y_up_basis(netimmerse_transform.rotation),
    );
    let bevy_translation = Vec3::from_array(convert_source_z_up_vector_to_bevy_y_up_coordinates(
        netimmerse_transform.translation,
    ));
    Mat4::from_scale_rotation_translation(
        Vec3::splat(netimmerse_transform.scale),
        bevy_rotation,
        bevy_translation,
    )
    .to_cols_array()
}
