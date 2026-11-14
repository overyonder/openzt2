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
fn a_turning_straight_is_painted_up_to_both_its_joints() {
    let definition = |appearance| GroundTransportTrackPiecePresentationDefinition {
        piece_type: 1,
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
    let definitions = [definition(0), definition(3), definition(6)];
    // From the south-west, one piece north, then off to the north-west.
    let route = [
        Vec3::new(-3.0, 0.0, 3.0),
        Vec3::ZERO,
        Vec3::new(0.0, 0.0, -4.5),
        Vec3::new(-3.0, 0.0, -7.5),
    ];
    let start = select_authored_ground_track_piece_presentation(&definitions, &route, 1)
        .expect("the start's turn texture");
    assert_eq!(start.appearance, 3);
    assert_eq!(
        authored_ground_track_paint_shift(&route, 1),
        Vec3::new(0.0, 0.0, AUTHORED_GROUND_TRACK_TURN_SHIFT_METRES)
    );
    let (end, shift) = authored_ground_track_second_paint(&definitions, &route, 1)
        .expect("a second paint for the end's turn");
    assert_eq!(end.appearance, 6);
    assert_eq!(
        shift,
        Vec3::new(0.0, 0.0, -AUTHORED_GROUND_TRACK_TURN_SHIFT_METRES)
    );
    // Two pieces north: each also lays the plain road towards the other.
    let route = [
        Vec3::new(-3.0, 0.0, 3.0),
        Vec3::ZERO,
        Vec3::new(0.0, 0.0, -3.0),
        Vec3::new(0.0, 0.0, -6.0),
        Vec3::new(-3.0, 0.0, -9.0),
    ];
    let (plain, shift) = authored_ground_track_second_paint(&definitions, &route, 1)
        .expect("the plain road towards the next straight");
    assert_eq!(plain.appearance, 0);
    assert_eq!(
        shift,
        Vec3::new(0.0, 0.0, -AUTHORED_GROUND_TRACK_TURN_SHIFT_METRES)
    );
    let (plain, shift) = authored_ground_track_second_paint(&definitions, &route, 2)
        .expect("the plain road towards the previous straight");
    assert_eq!(plain.appearance, 0);
    assert_eq!(
        shift,
        Vec3::new(0.0, 0.0, AUTHORED_GROUND_TRACK_TURN_SHIFT_METRES)
    );
    // A road ending one piece after its turn has nothing to reach.
    assert!(authored_ground_track_second_paint(&definitions, &route[..3], 1).is_none());
}
