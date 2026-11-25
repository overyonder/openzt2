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

#[test]
fn paint_selects_one_authored_appearance_for_each_piece() {
    let definition = |piece_type, appearance| GroundTransportTrackPiecePresentationDefinition {
        piece_type,
        appearance,
        ground_decal_texture: openzt2_game_data::AssetId::default(),
        ground_decal_rotation_radians: 0.0,
        ground_decal_size_metres: [4.7, 4.7],
        ground_decal_offset_metres: [0.0, 0.0],
        paint_texture: openzt2_game_data::AssetId::default(),
        paint_sample_start_texels: [10.0, 10.0],
        paint_copy_dimension_texels: [43.0, 43.0],
        elevated_path_texture: openzt2_game_data::AssetId::default(),
        elevated_path_rotation_radians: 0.0,
    };
    let definitions = [definition(1, 3), definition(3, 14)];
    let cardinal = [
        Vec3::new(-3.0, 0.0, 3.0),
        Vec3::ZERO,
        Vec3::new(0.0, 0.0, -4.5),
        Vec3::new(-3.0, 0.0, -7.5),
    ];
    let selected = select_authored_ground_track_piece_presentation(&definitions, &cardinal, 1)
        .expect("the authored cardinal turn appearance");
    assert_eq!(selected.appearance, 3);

    let diagonal = [
        Vec3::new(0.0, 0.0, 3.0),
        Vec3::ZERO,
        Vec3::new(3.0, 0.0, -3.0),
        Vec3::new(3.0, 0.0, -6.0),
    ];
    let selected = select_authored_ground_track_piece_presentation(&definitions, &diagonal, 1)
        .expect("the authored diagonal appearance with two turns");
    assert_eq!(selected.appearance, 14);
}
