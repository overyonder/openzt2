use crate::assets::source_document::blue_fang_source_numeric_lexeme::parse_blue_fang_source_numeric_lexeme;
use crate::assets::source_document::ordered_source_document_types::{
    OrderedSourceDocumentNode, OrderedSourceDocumentSpan,
};
use crate::assets::source_document::resolved_source_record_index::{BindError, RecordView};
use crate::assets::source_document::source_document_semantic_name::{
    canonicalize_source_document_record_key, source_document_names_are_semantically_equal,
};
use openzt2_game_data::AssetId;
use std::str::FromStr;

pub(super) fn simple_error(message: impl Into<String>) -> BindError {
    BindError {
        virtual_path: "<world-definition-document>".to_owned(),
        span: OrderedSourceDocumentSpan::default(),
        message: message.into(),
    }
}
pub(super) fn required<'document>(
    record: &RecordView<'_, 'document>,
    names: &[&str],
) -> Result<&'document str, BindError> {
    record.value(names).ok_or_else(|| {
        BindError::record(record, format!("missing required typed field {}", names[0]))
    })
}
pub(super) fn required_element<'a>(
    element: &'a OrderedSourceDocumentNode,
    names: &[&str],
) -> Result<&'a str, BindError> {
    element
        .attribute_named_any(names)
        .or_else(|| element.first_text())
        .ok_or_else(|| simple_error(format!("missing required child field {}", names[0])))
}
pub(super) fn parse<T: FromStr>(
    value: &str,
    record: &RecordView<'_, '_>,
    field: &str,
) -> Result<T, BindError> {
    parse_blue_fang_source_numeric_lexeme(value)
        .ok_or_else(|| BindError::record(record, format!("invalid {field}: {value}")))
}
pub(super) fn required_number<T: FromStr>(
    record: &RecordView<'_, '_>,
    names: &[&str],
) -> Result<T, BindError> {
    parse(required(record, names)?, record, names[0])
}
pub(super) fn number_or<T: FromStr>(
    record: &RecordView<'_, '_>,
    names: &[&str],
    default: T,
) -> Result<T, BindError> {
    record
        .value(names)
        .map(|value| parse(value, record, names[0]))
        .transpose()
        .map(|value| value.unwrap_or(default))
}
pub(super) fn bool_or(
    record: &RecordView<'_, '_>,
    names: &[&str],
    default: bool,
) -> Result<bool, BindError> {
    record
        .value(names)
        .map(
            |value| match canonicalize_source_document_record_key(value).as_str() {
                "true" | "yes" | "1" => Ok(true),
                "false" | "no" | "0" => Ok(false),
                _ => Err(BindError::record(
                    record,
                    format!("invalid boolean {value}"),
                )),
            },
        )
        .transpose()
        .map(|value| value.unwrap_or(default))
}
pub(super) fn required_bool(
    record: &RecordView<'_, '_>,
    names: &[&str],
) -> Result<bool, BindError> {
    match canonicalize_source_document_record_key(required(record, names)?).as_str() {
        "true" | "yes" | "1" => Ok(true),
        "false" | "no" | "0" => Ok(false),
        value => Err(BindError::record(
            record,
            format!("invalid boolean {value}"),
        )),
    }
}
pub(super) fn element_bool(
    element: &'_ OrderedSourceDocumentNode,
    names: &[&str],
    default: bool,
) -> Result<bool, BindError> {
    element
        .attribute_named_any(names)
        .map_or(
            Ok(default),
            |value| match canonicalize_source_document_record_key(value).as_str() {
                "true" | "yes" | "1" => Ok(true),
                "false" | "no" | "0" => Ok(false),
                value => Err(simple_error(format!("invalid child boolean {value}"))),
            },
        )
}
pub(super) fn array_required<T: FromStr + Copy, const N: usize>(
    record: &RecordView<'_, '_>,
    names: &[&str; N],
) -> Result<[T; N], BindError> {
    let mut values = Vec::with_capacity(N);
    for name in names {
        values.push(parse(required(record, &[*name])?, record, name)?);
    }
    values
        .try_into()
        .map_err(|_| BindError::record(record, "invalid fixed field array"))
}
pub(super) fn seconds_ns(seconds: f64, record: &RecordView<'_, '_>) -> Result<u64, BindError> {
    let value = seconds * 1_000_000_000.0;
    if !value.is_finite() || value < 0.0 || value > u64::MAX as f64 {
        Err(BindError::record(
            record,
            "seconds value exceeds stored duration",
        ))
    } else {
        Ok(value.round() as u64)
    }
}
pub(super) fn optional_seconds_ns(
    record: &RecordView<'_, '_>,
    names: &[&str],
) -> Result<u64, BindError> {
    record
        .value(names)
        .map(|value| parse::<f64>(value, record, names[0]))
        .transpose()?
        .map_or(Ok(0), |value| seconds_ns(value, record))
}
pub(super) fn money_cents(value: f64, record: &RecordView<'_, '_>) -> Result<i32, BindError> {
    let cents = value * 100.0;
    if !cents.is_finite() || cents < f64::from(i32::MIN) || cents > f64::from(i32::MAX) {
        Err(BindError::record(
            record,
            "money value exceeds stored cents",
        ))
    } else {
        Ok(cents.round() as i32)
    }
}
pub(super) fn money_cents_i64_or(
    record: &RecordView<'_, '_>,
    cents_names: &[&str],
    source_names: &[&str],
) -> Result<i64, BindError> {
    if let Some(value) = record.value(cents_names) {
        parse(value, record, cents_names[0])
    } else {
        record
            .value(source_names)
            .map(|value| parse::<f64>(value, record, source_names[0]))
            .transpose()?
            .map_or(Ok(0), |value| money_cents(value, record).map(i64::from))
    }
}
pub(super) fn money_cents_i32_or(
    record: &RecordView<'_, '_>,
    cents_names: &[&str],
    source_names: &[&str],
) -> Result<i32, BindError> {
    money_cents_i64_or(record, cents_names, source_names).and_then(|value| {
        i32::try_from(value)
            .map_err(|_| BindError::record(record, "money value exceeds stored cents"))
    })
}
pub(super) fn percent_permille(value: f64, record: &RecordView<'_, '_>) -> Result<u16, BindError> {
    let permille = value * 10.0;
    if !permille.is_finite() || !(0.0..=f64::from(u16::MAX)).contains(&permille) {
        Err(BindError::record(
            record,
            "percentage exceeds stored permille",
        ))
    } else {
        Ok(permille.round() as u16)
    }
}
pub(super) fn source_distance_cm(
    value: f64,
    record: &RecordView<'_, '_>,
) -> Result<i32, BindError> {
    let centimetres = value * 100.0;
    if !centimetres.is_finite()
        || centimetres < f64::from(i32::MIN)
        || centimetres > f64::from(i32::MAX)
    {
        Err(BindError::record(
            record,
            "source distance exceeds stored centimetres",
        ))
    } else {
        Ok(centimetres.round() as i32)
    }
}
pub(super) fn parse_rgb_u16(value: &str) -> Result<[u16; 3], BindError> {
    let values = value
        .split_ascii_whitespace()
        .map(parse_blue_fang_source_numeric_lexeme::<u16>)
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| simple_error(format!("invalid RGB color {value}")))?;
    let [r, g, b]: [u16; 3] = values
        .try_into()
        .map_err(|_| simple_error(format!("invalid RGB color {value}")))?;
    if r > 255 || g > 255 || b > 255 {
        return Err(simple_error(format!("RGB channel exceeds 255: {value}")));
    }
    Ok([r * 257, g * 257, b * 257])
}
pub(super) fn parse_f32_triplet(value: &str) -> Result<[f32; 3], BindError> {
    value
        .split_ascii_whitespace()
        .map(parse_blue_fang_source_numeric_lexeme::<f32>)
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| simple_error(format!("invalid float triplet {value}")))?
        .try_into()
        .map_err(|_| simple_error(format!("invalid float triplet {value}")))
}
pub(super) fn parse_f32_pair(value: &str) -> Result<[f32; 2], BindError> {
    value
        .split_ascii_whitespace()
        .map(parse_blue_fang_source_numeric_lexeme::<f32>)
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| simple_error(format!("invalid float pair {value}")))?
        .try_into()
        .map_err(|_| simple_error(format!("invalid float pair {value}")))
}
pub(super) fn indexed_attribute(name: &str, prefix: char) -> Option<u16> {
    let mut chars = name.chars();
    (chars.next()?.eq_ignore_ascii_case(&prefix))
        .then(|| chars.as_str().parse().ok())
        .flatten()
}
pub(super) fn child_named<'a>(
    element: &'a OrderedSourceDocumentNode,
    name: &str,
) -> Option<&'a OrderedSourceDocumentNode> {
    element
        .element_children()
        .find(|child| source_document_names_are_semantically_equal(child.name.as_str(), name))
}
pub(super) fn indexed_values<'a>(
    element: &'a OrderedSourceDocumentNode,
    prefixes: &[char],
) -> impl Iterator<Item = (u16, &'a str)> + use<'a> {
    let prefixes = prefixes.to_vec();
    element.attributes().filter_map(move |(name, value)| {
        let normalized = name.to_ascii_lowercase();
        prefixes
            .iter()
            .any(|prefix| normalized.starts_with(*prefix))
            .then(|| {
                let digits = normalized.trim_start_matches(|ch: char| ch.is_ascii_alphabetic());
                digits.parse::<u16>().ok().map(|index| (index, value))
            })
            .flatten()
    })
}
pub(super) fn parse_element_scalar<T: FromStr>(value: &str, field: &str) -> Result<T, BindError> {
    parse_blue_fang_source_numeric_lexeme(value)
        .ok_or_else(|| simple_error(format!("invalid {field}: {value}")))
}

pub(super) fn id(value: &str) -> AssetId {
    AssetId::from_key(&canonicalize_source_document_record_key(value))
}
pub(super) fn asset(record: &RecordView<'_, '_>, names: &[&str]) -> AssetId {
    record
        .value(names)
        .filter(|value| !value.trim().is_empty())
        .map(id)
        .unwrap_or_default()
}
pub(super) fn optional_asset(record: &RecordView<'_, '_>, names: &[&str]) -> Option<AssetId> {
    record
        .value(names)
        .filter(|value| !value.trim().is_empty())
        .map(id)
}
pub(super) fn element_asset(element: &'_ OrderedSourceDocumentNode, names: &[&str]) -> AssetId {
    element
        .attribute_named_any(names)
        .filter(|value| !value.trim().is_empty())
        .map(id)
        .unwrap_or_default()
}
pub(super) fn asset_list(value: &str) -> Vec<AssetId> {
    value
        .split(|ch: char| ch == ',' || ch == ';' || ch.is_ascii_whitespace())
        .filter(|part| !part.is_empty())
        .map(id)
        .collect()
}
pub(super) fn element_number<T: FromStr + Copy>(
    element: &'_ OrderedSourceDocumentNode,
    names: &[&str],
    default: T,
) -> Result<T, BindError> {
    element
        .attribute_named_any(names)
        .map(|value| {
            parse_blue_fang_source_numeric_lexeme(value)
                .ok_or_else(|| simple_error(format!("invalid child field {}: {value}", names[0])))
        })
        .transpose()
        .map(|value| value.unwrap_or(default))
}
pub(super) fn required_element_number<T: FromStr>(
    element: &'_ OrderedSourceDocumentNode,
    names: &[&str],
) -> Result<T, BindError> {
    let value = required_element(element, names)?;
    parse_blue_fang_source_numeric_lexeme(value)
        .ok_or_else(|| simple_error(format!("invalid child field {}: {value}", names[0])))
}
pub(super) fn array<T: FromStr + Copy, const N: usize>(
    record: &RecordView<'_, '_>,
    names: &[&str; N],
    default: [T; N],
) -> Result<[T; N], BindError> {
    let mut output = default;
    for (index, name) in names.iter().enumerate() {
        if let Some(value) = record.value(&[*name]) {
            output[index] = parse(value, record, name)?;
        }
    }
    Ok(output)
}
pub(super) fn element_array<T: FromStr + Copy, const N: usize>(
    element: &'_ OrderedSourceDocumentNode,
    names: &[&str; N],
    default: [T; N],
) -> Result<[T; N], BindError> {
    let mut output = default;
    for (index, name) in names.iter().enumerate() {
        output[index] = element_number(element, &[*name], output[index])?;
    }
    Ok(output)
}
pub(super) fn matrix(record: &RecordView<'_, '_>) -> Result<[[f32; 4]; 4], BindError> {
    let names = [
        "m00", "m01", "m02", "m03", "m10", "m11", "m12", "m13", "m20", "m21", "m22", "m23", "m30",
        "m31", "m32", "m33",
    ];
    let values = names
        .iter()
        .map(|name| required_number(record, &[*name]))
        .collect::<Result<Vec<f32>, _>>()?;
    Ok([
        [values[0], values[1], values[2], values[3]],
        [values[4], values[5], values[6], values[7]],
        [values[8], values[9], values[10], values[11]],
        [values[12], values[13], values[14], values[15]],
    ])
}
pub(super) fn flags(value: Option<&str>, map: fn(&str) -> Option<u64>) -> Result<u64, BindError> {
    value
        .unwrap_or_default()
        .split(|ch: char| ch == ',' || ch == '|' || ch == ';' || ch.is_ascii_whitespace())
        .filter(|part| !part.is_empty())
        .try_fold(0, |bits, part| {
            map(&canonicalize_source_document_record_key(part))
                .map(|bit| bits | bit)
                .ok_or_else(|| simple_error(format!("unknown flag {part}")))
        })
}
pub(super) fn enum_value<T: Copy>(
    value: &str,
    values: &[(&str, T)],
    label: &str,
) -> Result<T, BindError> {
    let value = canonicalize_source_document_record_key(value);
    values
        .iter()
        .find_map(|(name, result)| (value == *name).then_some(*result))
        .ok_or_else(|| simple_error(format!("unknown {label} {value}")))
}
