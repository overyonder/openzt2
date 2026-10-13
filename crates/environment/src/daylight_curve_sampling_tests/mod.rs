use super::daylight_curve_sampling::{
    daylight_fraction_unit_is_within_inclusive_window, encode_daylight_fraction_as_u16,
    select_cyclic_daylight_curve_sample,
};

#[test]
fn authored_closing_sky_and_fog_keys_retain_the_final_daylight_interval() {
    let sky_times = [0.0, 0.03, 0.2, 0.3, 0.7, 0.8, 0.97, 1.0];
    let sample = select_cyclic_daylight_curve_sample(0.985, sky_times.len(), |index| {
        encode_daylight_fraction_as_u16(sky_times[index])
    })
    .expect("authored sky curve has keyframes");
    assert_eq!(
        (sample.left_keyframe_index, sample.right_keyframe_index),
        (6, 7)
    );
    assert!((sample.interpolation_fraction - 0.5).abs() < 0.001);
    assert_eq!(encode_daylight_fraction_as_u16(1.0), u16::MAX);
}

#[test]
fn cyclic_keyframes_interpolate_across_midnight_without_allocating() {
    let keyframe_day_fractions = [10_000, 30_000, 60_000];
    let before_midnight =
        select_cyclic_daylight_curve_sample(0.99, keyframe_day_fractions.len(), |index| {
            keyframe_day_fractions[index]
        })
        .unwrap();
    assert_eq!(
        (
            before_midnight.left_keyframe_index,
            before_midnight.right_keyframe_index,
        ),
        (2, 0)
    );
    assert!((0.0..=1.0).contains(&before_midnight.interpolation_fraction));

    let daytime = select_cyclic_daylight_curve_sample(0.4, keyframe_day_fractions.len(), |index| {
        keyframe_day_fractions[index]
    })
    .unwrap();
    assert_eq!(
        (daytime.left_keyframe_index, daytime.right_keyframe_index,),
        (0, 1)
    );
    assert_eq!(select_cyclic_daylight_curve_sample(0.5, 0, |_| 0), None);
}

#[test]
fn wrapped_day_periods_cover_authored_night_windows() {
    assert!(daylight_fraction_unit_is_within_inclusive_window(
        64_000,
        [60_000, 5_000]
    ));
    assert!(daylight_fraction_unit_is_within_inclusive_window(
        1_000,
        [60_000, 5_000]
    ));
    assert!(!daylight_fraction_unit_is_within_inclusive_window(
        30_000,
        [60_000, 5_000]
    ));
    assert!(daylight_fraction_unit_is_within_inclusive_window(
        30_000,
        [20_000, 40_000]
    ));
}
