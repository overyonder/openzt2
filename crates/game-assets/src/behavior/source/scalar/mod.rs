//! Shared authored scalar lowering for evaluation and execution.

use super::{
    action::source_value_reading::convert_authored_scalar_to_q16, invalid_behavior_source_data,
};
use openzt2_game_data::behavior::scalar::BehaviorScalarQ16;
use std::io;

pub(super) fn lower_behavior_scalar_q16(authored_scalar: &str) -> io::Result<BehaviorScalarQ16> {
    let trimmed_scalar = authored_scalar.trim();
    if let Some(arguments) = trimmed_scalar
        .strip_prefix("price_effect(")
        .and_then(|value| value.strip_suffix(')'))
    {
        let values = arguments
            .split(',')
            .map(str::trim)
            .map(convert_authored_scalar_to_q16)
            .collect::<io::Result<Vec<_>>>()?;
        let values: [i32; 3] = values.try_into().map_err(|_| {
            invalid_behavior_source_data(format!("invalid price-effect scalar {trimmed_scalar}"))
        })?;
        return Ok(BehaviorScalarQ16::PriceEffectQ16(values));
    }
    let Some(random_arguments_text) = trimmed_scalar
        .strip_prefix("rand(")
        .and_then(|value| value.strip_suffix(')'))
    else {
        return convert_authored_scalar_to_q16(trimmed_scalar).map(BehaviorScalarQ16::FixedQ16);
    };
    let mut random_argument_values = random_arguments_text.split(',').map(str::trim);
    let minimum_q16 = random_argument_values
        .next()
        .map(convert_authored_scalar_to_q16)
        .transpose()?;
    let maximum_q16 = random_argument_values
        .next()
        .map(convert_authored_scalar_to_q16)
        .transpose()?;
    let step_q16 = random_argument_values
        .next()
        .map(convert_authored_scalar_to_q16)
        .transpose()?;
    let (Some(minimum_q16), Some(maximum_q16), Some(step_q16)) =
        (minimum_q16, maximum_q16, step_q16)
    else {
        return Err(invalid_random_behavior_scalar(trimmed_scalar));
    };
    if random_argument_values.next().is_some() || maximum_q16 < minimum_q16 || step_q16 <= 0 {
        return Err(invalid_random_behavior_scalar(trimmed_scalar));
    }
    Ok(BehaviorScalarQ16::RandomQ16 {
        minimum: minimum_q16,
        maximum: maximum_q16,
        step: step_q16,
    })
}

fn invalid_random_behavior_scalar(authored_scalar: &str) -> io::Error {
    invalid_behavior_source_data(format!("invalid random behavior scalar {authored_scalar}"))
}
