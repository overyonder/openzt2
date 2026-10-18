//! Source transform and collider-axis math shared by native scene lowerers.

use bevy::math::Mat4;
use openzt2_game_data::scene_prefab::PrefabTransform;

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
    let matrix = Mat4::from_cols_array(
        &super::super::native_geometry::source_transform_conversion::convert_blue_fang_source_matrix_to_gltf_matrix(m),
    );
    if !matrix.is_finite() || matrix.determinant() == 0.0 {
        return None;
    }
    let (scale, rotation, translation) = matrix.to_scale_rotation_translation();
    Some(PrefabTransform {
        translation_m: translation.to_array(),
        rotation_xyzw: rotation.to_array(),
        scale: scale.to_array(),
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
