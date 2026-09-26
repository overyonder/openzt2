use crate::assets::source_document::ui::model::{
    SourceUiAnimation, SourceUiColor, SourceUiRect, SourceUiShowHideAnimation,
    SourceUiShowHideColors,
};
use crate::assets::ui_document::source::lower::authored_ui_document_lowering::AuthoredUiDocument;
use crate::assets::ui_document::source::lower::authored_ui_scalar_value_lowering::invalid_at;
use openzt2_game_data::ui_document::node_presentation::{
    UiNodeAnimationDefinition, UiNodeAnimationInterpolation,
};
use std::io;

pub(super) fn lower_animation(
    source: Option<&SourceUiShowHideAnimation>,
    input: &AuthoredUiDocument,
) -> io::Result<Option<UiNodeAnimationDefinition>> {
    let Some(source) = source else {
        return Ok(None);
    };
    lower_animation_values(
        source.seconds,
        source.exit_rate,
        source.initial_seconds,
        source.initial_direction,
        source.bob_seconds,
        source.delay_seconds,
        source.function.as_deref(),
        source.start,
        source.end,
        source.colors.as_ref(),
        input,
    )
}

pub(super) fn lower_widget_animation(
    source: &SourceUiAnimation,
    input: &AuthoredUiDocument,
) -> io::Result<Option<UiNodeAnimationDefinition>> {
    lower_animation_values(
        source.seconds,
        source.exit_rate,
        source.initial_time,
        source.initial_direction,
        source.bob_seconds,
        source.delay_seconds,
        source.function.as_deref(),
        source.start,
        source.end,
        source.colors.as_ref(),
        input,
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn lower_animation_values(
    seconds: Option<f32>,
    exit_rate: Option<f32>,
    initial_seconds: Option<f32>,
    initial_direction: Option<bool>,
    bob_seconds: Option<f32>,
    delay_seconds: Option<f32>,
    source_interpolation_name: Option<&str>,
    start: Option<SourceUiRect>,
    end: Option<SourceUiRect>,
    colors: Option<&SourceUiShowHideColors>,
    input: &AuthoredUiDocument,
) -> io::Result<Option<UiNodeAnimationDefinition>> {
    let seconds = seconds.unwrap_or(0.0);
    let exit_rate = exit_rate.unwrap_or(1.0);
    let initial_seconds = initial_seconds.unwrap_or(0.0);
    let bob_seconds = bob_seconds.unwrap_or(0.0);
    let delay_seconds = delay_seconds.unwrap_or(0.0);
    if !seconds.is_finite()
        || seconds < 0.0
        || !exit_rate.is_finite()
        || exit_rate < 0.0
        || !initial_seconds.is_finite()
        || initial_seconds < 0.0
        || !bob_seconds.is_finite()
        || bob_seconds < 0.0
        || !delay_seconds.is_finite()
        || delay_seconds < 0.0
    {
        return Err(invalid_at(input, "UI animation timing is invalid"));
    }
    let interpolation = match source_interpolation_name {
        None | Some("linear") => UiNodeAnimationInterpolation::Linear,
        Some("sin") => UiNodeAnimationInterpolation::Sinusoidal,
        Some(value) => {
            return Err(invalid_at(
                input,
                format!("unknown UI animation function {value:?}"),
            ));
        }
    };
    let rect = |value: Option<SourceUiRect>| {
        value.map_or([-1.0; 4], |value| {
            [value.x, value.y, value.width, value.height]
        })
    };
    let color = |value: Option<SourceUiColor>| {
        value.map_or([255; 4], |value| {
            [value.red, value.green, value.blue, value.alpha]
        })
    };
    Ok(Some(UiNodeAnimationDefinition {
        duration_ms: (seconds * 1000.0).round() as u32,
        exit_rate,
        initial_ms: (initial_seconds * 1000.0).round() as u32,
        initial_forward: initial_direction.unwrap_or(true),
        delay_ms: (delay_seconds * 1000.0).round() as u32,
        bob_ms: (bob_seconds * 1000.0).round() as u32,
        interpolation,
        start_rect: rect(start),
        end_rect: rect(end),
        animates_color: colors.is_some(),
        affects_text_color: colors.and_then(|value| value.affects_text).unwrap_or(false),
        start_color: color(colors.and_then(|value| value.start)),
        end_color: color(colors.and_then(|value| value.end)),
    }))
}
