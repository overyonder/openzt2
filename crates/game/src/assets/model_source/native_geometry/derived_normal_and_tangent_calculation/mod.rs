//! Derived normal and tangent calculations for validated lowered geometry.

use bevy::math::Vec3;

pub(super) fn derive_normals(positions: &[[f32; 3]], indices: &[u32]) -> Vec<[f32; 3]> {
    let mut output = vec![Vec3::ZERO; positions.len()];
    for triangle in indices.chunks_exact(3) {
        let [a, b, c] = [triangle[0], triangle[1], triangle[2]]
            .map(|index| usize::try_from(index).expect("validated geometry index fits usize"));
        let origin = Vec3::from_array(positions[a]);
        let face = (Vec3::from_array(positions[b]) - origin)
            .cross(Vec3::from_array(positions[c]) - origin);
        for index in [a, b, c] {
            output[index] += face;
        }
    }
    output
        .into_iter()
        .map(|normal| normalized_or(Some(normal.to_array()), Vec3::Y.to_array()))
        .collect()
}

pub(super) fn derive_tangents(
    positions: &[[f32; 3]],
    normals: &[[f32; 3]],
    uvs: &[[f32; 2]],
    indices: &[u32],
) -> Vec<[f32; 4]> {
    let mut accumulated_tangents = vec![Vec3::ZERO; positions.len()];
    let mut accumulated_bitangents = vec![Vec3::ZERO; positions.len()];
    for triangle in indices.chunks_exact(3) {
        let [a, b, c] = [triangle[0], triangle[1], triangle[2]]
            .map(|index| usize::try_from(index).expect("validated geometry index fits usize"));
        let origin = Vec3::from_array(positions[a]);
        let edge_a = Vec3::from_array(positions[b]) - origin;
        let edge_b = Vec3::from_array(positions[c]) - origin;
        let duv_a = [uvs[b][0] - uvs[a][0], uvs[b][1] - uvs[a][1]];
        let duv_b = [uvs[c][0] - uvs[a][0], uvs[c][1] - uvs[a][1]];
        let determinant = duv_a[0] * duv_b[1] - duv_a[1] * duv_b[0];
        if determinant.abs() <= f32::EPSILON {
            continue;
        }
        let inverse = determinant.recip();
        let tangent = (edge_a * duv_b[1] - edge_b * duv_a[1]) * inverse;
        let bitangent = (edge_b * duv_a[0] - edge_a * duv_b[0]) * inverse;
        for index in [a, b, c] {
            accumulated_tangents[index] += tangent;
            accumulated_bitangents[index] += bitangent;
        }
    }
    normals
        .iter()
        .zip(accumulated_tangents)
        .zip(accumulated_bitangents)
        .map(|((normal, tangent), bitangent)| {
            let normal = Vec3::from_array(*normal);
            let projected = tangent - normal * normal.dot(tangent);
            let fallback_axis = if normal[1].abs() < 0.9 {
                Vec3::Y
            } else {
                Vec3::X
            };
            let tangent = normalized_or(
                Some(projected.to_array()),
                fallback_axis.cross(normal).to_array(),
            );
            let tangent = Vec3::from_array(tangent);
            let handedness = if normal.cross(tangent).dot(bitangent) < 0.0 {
                -1.0
            } else {
                1.0
            };
            [tangent.x, tangent.y, tangent.z, handedness]
        })
        .collect()
}

pub(super) fn normalized_or(value: Option<[f32; 3]>, fallback: [f32; 3]) -> [f32; 3] {
    value
        .into_iter()
        .chain(std::iter::once(fallback))
        .map(Vec3::from_array)
        .filter(|value| value.is_finite())
        .find_map(|value| {
            let length = value.length();
            (length > 0.000_001).then(|| value / length)
        })
        .unwrap_or(Vec3::Y)
        .to_array()
}
