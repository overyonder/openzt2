use super::player_clock_lanes::AnimationPlaybackInterval;
use openzt2_game_assets::animation::animation_set_asset_types::AnimationTimelineKey;

pub(super) fn visit_interval(
    step: &AnimationPlaybackInterval,
    keys: &[AnimationTimelineKey],
    visit: &mut impl FnMut(&AnimationTimelineKey),
) {
    let at_or_after = |seconds: f32| {
        keys.partition_point(|key| {
            f64::from(key.milliseconds) < f64::from(seconds) * 1000.0 - 0.0001
        })
    };
    let after = |seconds: f32| {
        keys.partition_point(|key| {
            f64::from(key.milliseconds) <= f64::from(seconds) * 1000.0 + 0.0001
        })
    };
    let first = at_or_after(step.cycle_start);
    let last = after(step.cycle_end);
    let mut play_range = |mut start: usize, end: usize| {
        let end = end.min(keys.len());
        start = start.min(end);
        if step.forward {
            for index in start..end {
                visit(&keys[index]);
            }
        } else {
            let mut right = end;
            while right > start {
                let time = keys[right - 1].milliseconds;
                let left = start
                    + keys[start..right].partition_point(|candidate| candidate.milliseconds < time);
                for key in &keys[left..right] {
                    visit(key);
                }
                right = left;
            }
        }
    };
    if step.forward {
        let start = if step.clip_entry { 0 } else { after(step.from) };
        if step.cycles <= 0 {
            play_range(start, after(step.to));
        } else {
            play_range(start, last);
            for _ in 1..step.cycles {
                play_range(first, last);
            }
            play_range(first, after(step.to));
        }
    } else if step.cycles >= 0 {
        let end = if step.clip_entry {
            last
        } else {
            at_or_after(step.from)
        };
        play_range(at_or_after(step.to), end);
    } else {
        let end = at_or_after(step.from);
        play_range(first, end);
        for _ in 1..step.cycles.unsigned_abs() {
            play_range(first, last);
        }
        play_range(at_or_after(step.to), last);
    }
}

#[cfg(test)]
mod tests;
