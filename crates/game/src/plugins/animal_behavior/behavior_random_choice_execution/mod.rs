//! Native weighted choices and play counts shared by random sets and clips.

use crate::plugins::simulation_time::deterministic_random_stream::DeterministicRng;

pub(super) fn select_next_authored_random_choice<T>(
    choices: &[(T, f32)],
    minimum_plays: u32,
    maximum_plays: u32,
    looping: bool,
    repetitions: &mut u32,
    random: &mut DeterministicRng,
) -> Result<Option<usize>, ()> {
    if !looping {
        if *repetitions == 0 {
            let count = if minimum_plays == maximum_plays {
                minimum_plays
            } else {
                minimum_plays
                    + random
                        .range_u32(maximum_plays.checked_sub(minimum_plays).ok_or(())?)
                        .ok_or(())?
            };
            *repetitions = count.checked_add(1).ok_or(())?;
        }
        if *repetitions == 1 {
            return Ok(None);
        }
        *repetitions -= 1;
    }
    let total: f32 = choices.iter().map(|(_, weight)| weight).sum();
    let mut draw = random.unit_f32() * total;
    let index = choices
        .iter()
        .position(|(_, weight)| {
            draw -= *weight;
            draw <= 0.0
        })
        .or_else(|| choices.len().checked_sub(1))
        .ok_or(())?;
    Ok(Some(index))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_count_finishes_without_resampling_or_resetting() {
        let mut random = DeterministicRng::from_raw([42, 7]);
        let mut remaining = 0;
        for _ in 0..3 {
            assert_eq!(
                select_next_authored_random_choice(
                    &[("idle", 1.0)],
                    3,
                    3,
                    false,
                    &mut remaining,
                    &mut random
                ),
                Ok(Some(0))
            );
        }
        let before = random.to_raw();
        assert_eq!(
            select_next_authored_random_choice(
                &[("idle", 1.0)],
                3,
                3,
                false,
                &mut remaining,
                &mut random
            ),
            Ok(None)
        );
        assert_eq!(random.to_raw(), before);
    }
}
