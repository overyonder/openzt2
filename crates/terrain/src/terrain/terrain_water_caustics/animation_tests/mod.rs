use super::*;

#[test]
fn camera_world_conversion_inverts_the_canonical_source_basis() {
    let source_point = Vec3::new(2.0, 3.0, 5.0);
    let world_point = Vec3::from_array(convert_source_z_up_vector_to_bevy_y_up_coordinates(
        source_point.to_array(),
    ));
    assert_eq!(world_point, Vec3::new(2.0, 5.0, -3.0));
    assert_eq!(
        source_from_bevy_world().transform_point3(world_point),
        source_point
    );
    let camera = Mat4::from_translation(Vec3::new(7.0, 11.0, 13.0));
    assert_eq!(
        (source_from_bevy_world() * camera).transform_point3(Vec3::new(2.0, 3.0, 5.0)),
        Vec3::new(9.0, -18.0, 14.0),
    );
}

#[test]
fn rocking_discards_overshoot_and_resamples_only_at_the_lower_end() {
    let mut rock = TerrainWaterCausticRock {
        phase: 0.5,
        rate: 0.75,
        distance: 0.02,
    };
    let mut draws = 0;
    let mut random = || {
        draws += 1;
        0.5
    };
    advance_rock(&mut rock, [1.0, 0.5], [-0.02, 0.02], 2.0, &mut random);
    assert_eq!((rock.phase, rock.rate, rock.distance), (1.0, -0.75, 0.02));
    advance_rock(&mut rock, [1.0, 0.5], [-0.02, 0.02], 2.0, &mut random);
    assert_eq!((rock.phase, rock.rate, rock.distance), (0.0, 0.75, 0.0));
    assert_eq!(draws, 2);
}

#[test]
fn projection_swaps_top_axes_and_maps_the_vertical_fade_interval() {
    let configuration = TerrainWaterCausticsConfiguration {
        bottom_map: String::new(),
        top_map: String::new(),
        height_lookup: String::new(),
        bottom_rate: [0.0; 2],
        top_rate: [0.0; 2],
        bottom_tile: 2.0,
        top_tile: 3.0,
        fade_start: 1.0,
        fade_distance: 4.0,
        tint_rgba: [1.0; 4],
        terrain_tint_rgba: [1.0; 4],
        bottom_rock_distance_range: [0.0; 2],
        top_rock_distance_range: [0.0; 2],
        bottom_rock_rate_range: [1.0; 2],
        top_rock_rate_range: [1.0; 2],
    };
    let state = initialize_caustic_animation(&configuration, &mut || 0.5);
    let [bottom, top, height] = caustic_world_projection_matrices(&state, &configuration, 11.0);
    let point = Vec4::new(2.0, 4.0, 10.0, 1.0);
    assert_eq!((bottom * point).truncate().truncate(), Vec2::new(4.0, 8.0));
    assert_eq!((top * point).truncate().truncate(), Vec2::new(12.0, 6.0));
    assert_eq!((height * point).x, 0.0);
    assert_eq!((height * Vec4::new(2.0, 4.0, 6.0, 1.0)).x, 1.0);
}
