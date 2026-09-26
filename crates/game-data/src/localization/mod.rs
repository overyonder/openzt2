//! Owned localized text, formatting, and rich-content presentation.

mod catalog_queries_and_formatting;

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::AssetId;

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct LocalizationCatalog {
    pub locale_identifier: String,
    pub localization_entries: Vec<LocalizationEntry>,
    pub localized_month_and_year_format_tokens: Vec<LocalizationDateFormatToken>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LocalizationEntry {
    pub localization_entry_identifier: AssetId,
    pub authored_localization_key: String,
    pub plain_localized_text: String,
    pub localized_text_format_tokens: Vec<LocalizationFormatToken>,
    pub localized_rich_content_nodes: Vec<LocalizationRichContentNode>,
    pub localized_text_presentation: LocalizationTextPresentation,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum LocalizationFormatToken {
    Literal(String),
    Argument {
        format_argument_index: u8,
        format_argument_kind: LocalizationFormatArgumentKind,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub enum LocalizationFormatArgumentKind {
    Text,
    Integer,
    Decimal,
    Currency,
    Percent,
}

#[derive(Clone, Copy, Debug)]
pub enum LocalizationFormatArgument<'a> {
    Text(&'a str),
    Integer(i64),
    Decimal(f64),
    CurrencyCents(i64),
    PercentBasisPoints(i64),
}

#[derive(Debug)]
pub enum LocalizationFormatError {
    MissingKey,
    MissingArgument(u8),
    ArgumentType,
    Write(fmt::Error),
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LocalizationRichContentNode {
    pub parent_rich_content_node_index: Option<u32>,
    pub rich_content_kind: LocalizationRichContentKind,
    pub localized_text: String,
    pub image_asset_path: Option<String>,
    pub linked_subject_path: Option<String>,
    pub rich_content_presentation: LocalizationRichContentPresentation,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub enum LocalizationRichContentKind {
    Cell,
    Text,
    Image,
    LineBreak,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct LocalizationTextPresentation {
    pub text_color_rgba: Option<[u8; 4]>,
    pub font_size_pixels: Option<f32>,
    pub bold_text: bool,
    pub shadow_offset_pixels: Option<[i16; 2]>,
    pub content_offset_pixels: Option<[f32; 2]>,
    pub background_image_asset_path: Option<String>,
    pub background_offset_pixels: [f32; 2],
    pub background_extent_pixels: [f32; 2],
}

// These booleans preserve independent rich-text presentation attributes.
#[allow(
    clippy::struct_excessive_bools,
    reason = "independent authored text flags"
)]
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
pub struct LocalizationRichContentPresentation {
    pub width_pixels: Option<f32>,
    pub height_pixels: Option<f32>,
    pub padding_pixels: Option<[f32; 2]>,
    pub source_rectangle_pixels: Option<[f32; 4]>,
    pub text_color_rgba: Option<[u8; 4]>,
    pub font_size_pixels: Option<f32>,
    pub bold_text: bool,
    pub italic_text: bool,
    pub underlined_text: bool,
    pub starts_paragraph: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum LocalizationDateFormatToken {
    Literal(String),
    MonthAbbreviation,
    Year,
}
