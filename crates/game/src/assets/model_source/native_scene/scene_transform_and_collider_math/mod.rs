//! Source transform and collider-axis math shared by native scene lowerers.

use openzt2_game_data::scene_prefab::PrefabTransform;

use crate::assets::source_coordinate_conversion::{
    conjugate_source_z_up_rotation_into_bevy_y_up_basis,
    convert_row_major_rotation_matrix_to_xyzw_quaternion,
    convert_source_z_up_vector_to_bevy_y_up_coordinates,
};

pub(super) fn source_vector_length(value: [f32; 3]) -> f32 {
    value
        .into_iter()
        .map(|value| value * value)
        .sum::<f32>()
        .sqrt()
}
pub(super) fn rotation_from_source_positive_y_axis(axis: [f32; 3]) -> [f32; 4] {
    let len = source_vector_length(axis);
    if len <= f32::EPSILON {
        return [0.0, 0.0, 0.0, 1.0];
    }
    let n = axis.map(|v| v / len);
    if n[1] < -0.999_999 {
        [1.0, 0.0, 0.0, 0.0]
    } else {
        let q = [n[2], 0.0, -n[0], 1.0 + n[1]];
        let l = q.into_iter().map(|v| v * v).sum::<f32>().sqrt();
        q.map(|v| v / l)
    }
}
pub(super) fn convert_blue_fang_scene_matrix_to_prefab_transform(
    m: [[f32; 4]; 4],
) -> Option<PrefabTransform> {
    let mut r = [
        m[0][0], m[1][0], m[2][0], m[0][1], m[1][1], m[2][1], m[0][2], m[1][2], m[2][2],
    ];
    let s = [
        source_vector_length([r[0], r[3], r[6]]),
        source_vector_length([r[1], r[4], r[7]]),
        source_vector_length([r[2], r[5], r[8]]),
    ];
    if s.into_iter().any(|v| !v.is_finite() || v <= 0.000_001) {
        return None;
    }
    for row in 0..3 {
        for col in 0..3 {
            r[row * 3 + col] /= s[col];
        }
    }
    Some(PrefabTransform {
        translation_m: convert_source_z_up_vector_to_bevy_y_up_coordinates([
            m[3][0], m[3][1], m[3][2],
        ]),
        rotation_xyzw: convert_row_major_rotation_matrix_to_xyzw_quaternion(
            conjugate_source_z_up_rotation_into_bevy_y_up_basis(r),
        ),
        scale: [s[0], s[2], s[1]],
    })
}
pub(super) fn translated_prefab_transform(translation_m: [f32; 3]) -> PrefabTransform {
    PrefabTransform {
        translation_m,
        rotation_xyzw: [0.0, 0.0, 0.0, 1.0],
        scale: [1.0; 3],
    }
}
pub(super) fn identity_prefab_transform() -> PrefabTransform {
    translated_prefab_transform([0.0; 3])
}
pub(super) fn netimmerse_point_light_range(c: f32, l: f32, q: f32) -> f32 {
    if q > 0.0 {
        q.recip().sqrt().clamp(1.0, 100.0)
    } else if l > 0.0 {
        l.recip().clamp(1.0, 100.0)
    } else {
        let _ = c;
        20.0
    }
}
