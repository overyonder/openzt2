use super::*;

fn controller(flags: u16) -> TextureCoordinateAnimation {
    TextureCoordinateAnimation {
        uv_set: 1,
        flags,
        frequency: 1.0,
        phase: 0.0,
        interval: 0.0..=2.0,
        tracks: std::array::from_fn(|_| Box::default()),
    }
}

#[test]
fn loop_reverse_and_clamp_follow_native_controller_time() {
    let mut animation = controller(8);
    assert_eq!(animation.sample_time(2.5), 0.5);
    assert_eq!(animation.sample_time(-0.5), 1.5);
    animation.flags = 10;
    assert_eq!(animation.sample_time(2.5), 1.5);
    assert_eq!(animation.sample_time(-0.5), 0.5);
    animation.flags = 12;
    assert_eq!(animation.sample_time(2.5), 2.0);
    assert_eq!(animation.sample_time(-0.5), 0.0);
    animation.flags = 8;
    animation.interval = 1.0..=3.0;
    // Native modulus receives scaled time, not scaled time minus range start.
    assert_eq!(animation.sample_time(0.5), 1.5);
    animation.frequency = 2.0;
    animation.phase = 0.25;
    assert_eq!(animation.sample_time(0.5), 2.25);
}

#[test]
fn texture_transform_preserves_native_center_and_offset_signs() {
    let mut animation = controller(8);
    animation.tracks = [0.25, 0.125, 2.0, 0.5].map(|value| {
        Box::from([(
            0.0,
            CubicSegment {
                coeff: [value, 0.0, 0.0, 0.0],
            },
        )])
    });
    let result = animation
        .transform(1.0, 0.0)
        .transform_point3(Vec3::new(0.5, 0.5, 0.0));
    assert_eq!(result, Vec3::new(0.25, 0.625, 0.0));
    animation.flags = 0;
    assert_eq!(animation.transform(1.0, 0.0), Mat4::IDENTITY);
}

#[test]
fn empty_tracks_and_zero_duration_are_stable() {
    let mut animation = controller(8);
    assert_eq!(animation.transform(100.0, 0.0), Mat4::IDENTITY);
    animation.interval = 0.75..=0.75;
    assert_eq!(animation.sample_time(100.0), 0.75);
}

#[test]
fn application_initialized_animation_uses_its_own_clock_origin() {
    let mut animation = controller(9);
    animation.tracks[0] = Box::from([
        (
            0.0,
            CubicSegment {
                coeff: [0.0, 2.0, 0.0, 0.0],
            },
        ),
        (
            2.0,
            CubicSegment {
                coeff: [2.0, 0.0, 0.0, 0.0],
            },
        ),
    ]);
    assert_eq!(animation.transform(101.0, 100.0).w_axis.x, -1.0);
    assert_eq!(animation.transform(101.0, 101.0).w_axis.x, 0.0);
}
