use bevy::prelude::Vec3;

const DAY_FRACTION_UNITS: f32 = u16::MAX as f32;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct CyclicDaylightCurveSample {
    pub(super) left_keyframe_index: usize,
    pub(super) right_keyframe_index: usize,
    pub(super) interpolation_fraction: f32,
}

pub(super) fn calculate_daylight_fraction(tick_within_day: u32, ticks_per_day: u32) -> Option<f32> {
    (ticks_per_day > 0 && tick_within_day < ticks_per_day)
        .then(|| tick_within_day as f32 / ticks_per_day as f32)
}

pub(super) fn encode_daylight_fraction_as_u16(daylight_fraction: f32) -> u16 {
    // Authored curves include a distinct closing key at t=1. Wrapping that
    // key to zero breaks their sorted order and skips the final daylight
    // interval. Only the sampled clock position wraps, not keyframe times.
    (daylight_fraction.clamp(0.0, 1.0) * DAY_FRACTION_UNITS).round() as u16
}

pub(super) fn select_cyclic_daylight_curve_sample(
    daylight_fraction: f32,
    keyframe_count: usize,
    keyframe_day_fraction: impl Fn(usize) -> u16,
) -> Option<CyclicDaylightCurveSample> {
    if keyframe_count == 0 || !daylight_fraction.is_finite() {
        return None;
    }
    if keyframe_count == 1 {
        return Some(CyclicDaylightCurveSample {
            left_keyframe_index: 0,
            right_keyframe_index: 0,
            interpolation_fraction: 0.0,
        });
    }

    let sample_position = daylight_fraction.rem_euclid(1.0) * DAY_FRACTION_UNITS;
    let right_keyframe_index = (0..keyframe_count)
        .find(|&index| f32::from(keyframe_day_fraction(index)) > sample_position)
        .unwrap_or(0);
    let left_keyframe_index = if right_keyframe_index == 0 {
        keyframe_count - 1
    } else {
        right_keyframe_index - 1
    };
    let left_position = f32::from(keyframe_day_fraction(left_keyframe_index));
    let mut right_position = f32::from(keyframe_day_fraction(right_keyframe_index));
    let mut wrapped_sample_position = sample_position;
    if right_keyframe_index == 0 {
        right_position += DAY_FRACTION_UNITS;
        if wrapped_sample_position < left_position {
            wrapped_sample_position += DAY_FRACTION_UNITS;
        }
    }
    let keyframe_distance = right_position - left_position;

    Some(CyclicDaylightCurveSample {
        left_keyframe_index,
        right_keyframe_index,
        interpolation_fraction: if keyframe_distance > 0.0 {
            ((wrapped_sample_position - left_position) / keyframe_distance).clamp(0.0, 1.0)
        } else {
            0.0
        },
    })
}

pub(super) fn interpolate_unsigned_normalized_u16(
    left: u16,
    right: u16,
    interpolation_fraction: f32,
) -> f32 {
    (f32::from(left) + (f32::from(right) - f32::from(left)) * interpolation_fraction)
        / f32::from(u16::MAX)
}

pub(super) fn interpolate_signed_normalized_direction(
    left: [i16; 3],
    right: [i16; 3],
    interpolation_fraction: f32,
) -> Vec3 {
    let decode_signed_normalized = |value: i16| f32::from(value) / f32::from(i16::MAX);
    Vec3::new(
        decode_signed_normalized(left[0])
            + (decode_signed_normalized(right[0]) - decode_signed_normalized(left[0]))
                * interpolation_fraction,
        decode_signed_normalized(left[1])
            + (decode_signed_normalized(right[1]) - decode_signed_normalized(left[1]))
                * interpolation_fraction,
        decode_signed_normalized(left[2])
            + (decode_signed_normalized(right[2]) - decode_signed_normalized(left[2]))
                * interpolation_fraction,
    )
    .normalize_or_zero()
}

pub(super) fn daylight_fraction_unit_is_within_inclusive_window(
    daylight_fraction_unit: u16,
    inclusive_window: [u16; 2],
) -> bool {
    if inclusive_window[0] <= inclusive_window[1] {
        daylight_fraction_unit >= inclusive_window[0]
            && daylight_fraction_unit <= inclusive_window[1]
    } else {
        daylight_fraction_unit >= inclusive_window[0]
            || daylight_fraction_unit <= inclusive_window[1]
    }
}
