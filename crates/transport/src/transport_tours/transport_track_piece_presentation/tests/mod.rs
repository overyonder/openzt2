use super::*;

#[test]
fn elevated_track_strip_carries_both_texture_coordinate_sets_for_the_fixed_function_shader() {
    let mesh = construct_elevated_ground_track_triangle_strip(
        &[Vec3::ZERO, Vec3::new(0.0, 0.0, 4.0)],
        2.0,
        0.0,
        Vec3::ZERO,
    )
    .expect("two route points build a strip");
    assert!(mesh.attribute(Mesh::ATTRIBUTE_UV_0).is_some());
    assert!(mesh.attribute(Mesh::ATTRIBUTE_UV_1).is_some());
}
