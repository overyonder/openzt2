use crate::assets::source_document::ui::model::{SourceUiColor, SourceUiMetric, SourceUiRegion};
use crate::assets::ui_document::source::lower::authored_ui_document_lowering::AuthoredUiDocument;
use std::io;

pub(super) fn numeric_rect(region: Option<&SourceUiRegion>) -> [f32; 4] {
    region
        .map(|r| {
            [
                number(&r.x).unwrap_or(0.0),
                number(&r.y).unwrap_or(0.0),
                number(&r.width).unwrap_or(0.0),
                number(&r.height).unwrap_or(0.0),
            ]
        })
        .unwrap_or([0.0; 4])
}
pub(super) fn number(metric: &SourceUiMetric) -> Option<f32> {
    if let SourceUiMetric::Number(value) = metric {
        Some(*value)
    } else {
        None
    }
}

pub(super) fn color4(value: SourceUiColor) -> [f32; 4] {
    [
        value.red as f32 / 255.0,
        value.green as f32 / 255.0,
        value.blue as f32 / 255.0,
        value.alpha as f32 / 255.0,
    ]
}
pub(super) fn source_color(value: SourceUiColor) -> [u8; 4] {
    [value.red, value.green, value.blue, value.alpha]
}
pub(super) fn parse_bool(value: &str) -> Option<bool> {
    match value {
        "1" | "true" | "on" => Some(true),
        "0" | "false" | "off" => Some(false),
        _ => None,
    }
}

pub(super) fn invalid_at(input: &AuthoredUiDocument, message: impl Into<String>) -> io::Error {
    invalid(format!("{}: {}", input.virtual_path, message.into()))
}
pub(super) fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}
