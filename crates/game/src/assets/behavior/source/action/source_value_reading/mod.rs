//! Shared conversion of behavior action attributes into canonical scalar and identifier values.

use std::io;

use openzt2_game_data::AssetId;

use super::super::invalid_behavior_source_data;
use crate::assets::source_document::ordered_source_document_types::OrderedSourceDocumentNode as DataNode;

pub(super) fn read_optional_asset_identifier_attribute(
    source_node: &DataNode,
    attribute_name: &str,
) -> Option<AssetId> {
    read_optional_trimmed_string_attribute(source_node, attribute_name)
        .map(|authored_asset_reference| AssetId::from_key(&authored_asset_reference))
}

pub(super) fn read_optional_trimmed_string_attribute(
    source_node: &DataNode,
    attribute_name: &str,
) -> Option<String> {
    source_node
        .attribute(attribute_name)
        .map(str::trim)
        .filter(|attribute_value| !attribute_value.is_empty())
        .map(str::to_owned)
}

pub(super) fn read_optional_float_attribute(
    source_node: &DataNode,
    attribute_name: &str,
) -> io::Result<Option<f32>> {
    source_node
        .attribute(attribute_name)
        .map(|authored_number| {
            authored_number
                .trim_end_matches(['f', 'F'])
                .parse()
                .map_err(|_| {
                    invalid_behavior_source_data(format!(
                        "{} has invalid {attribute_name}",
                        source_node.name
                    ))
                })
        })
        .transpose()
}

pub(super) fn read_nonnegative_centimetres_attribute(
    source_node: &DataNode,
    attribute_name: &str,
) -> io::Result<u32> {
    read_optional_float_attribute(source_node, attribute_name)
        .map(|metres| metres.map_or(0, |metres| (metres.max(0.0) * 100.0).round() as u32))
}

pub(in crate::assets::behavior::source) fn convert_authored_scalar_to_q16(
    authored_scalar: &str,
) -> io::Result<i32> {
    let trimmed_scalar = authored_scalar.trim_end_matches(['f', 'F']);
    if trimmed_scalar.eq_ignore_ascii_case("true") {
        return Ok(1 << 16);
    }
    if trimmed_scalar.eq_ignore_ascii_case("false") {
        return Ok(0);
    }
    let value = trimmed_scalar
        .parse::<f32>()
        .map_err(|_| invalid_behavior_source_data("invalid behavior scalar"))?;
    let scaled = (f64::from(value) * 65_536.0).round();
    if !scaled.is_finite() || scaled < f64::from(i32::MIN) || scaled > f64::from(i32::MAX) {
        return Err(invalid_behavior_source_data(
            "behavior scalar is outside the Q16 range",
        ));
    }
    Ok(scaled as i32)
}

pub(super) fn read_q16_attribute_or_default(
    source_node: &DataNode,
    attribute_name: &str,
    default_q16: i32,
) -> io::Result<i32> {
    source_node
        .attribute(attribute_name)
        .map(convert_authored_scalar_to_q16)
        .transpose()
        .map(|authored_q16| authored_q16.unwrap_or(default_q16))
}

pub(super) fn read_boolean_attribute_or_false(
    source_node: &DataNode,
    attribute_name: &str,
) -> io::Result<bool> {
    source_node
        .attribute(attribute_name)
        .map_or(Ok(false), |authored_value| {
            match authored_value.to_ascii_lowercase().as_str() {
                "true" | "1" | "yes" => Ok(true),
                "false" | "0" | "no" => Ok(false),
                _ => Err(invalid_behavior_source_data(format!(
                    "{} has invalid {attribute_name}",
                    source_node.name
                ))),
            }
        })
}
