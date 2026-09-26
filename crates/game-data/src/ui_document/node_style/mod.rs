use crate::AssetId;
use serde::{Deserialize, Serialize};

mod node_style_flag_and_default_operations;

#[derive(Deserialize, Serialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(transparent)]
pub struct UiStyleFlags(pub u16);

#[derive(Deserialize, Serialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum UiTextAlignment {
    #[default]
    Left,
    Center,
    Right,
    Justified,
}

/// Closed system-font vocabulary evidenced by the shipped UI documents.
///
/// This is presentation policy, not an asset identity: the original content
/// contains no font payload and the runtime delegates discovery, fallback,
/// shaping, and glyph caching to Bevy/Parley.
#[derive(Deserialize, Serialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum UiSystemFont {
    #[default]
    Arial,
    ComicSansMs,
}

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq)]
pub struct UiStyleRecord {
    pub background: AssetId,
    pub background_source: [i32; 4],
    pub background_width: f32,
    pub font: UiSystemFont,
    pub text_key: AssetId,
    pub color: [f32; 4],
    pub font_px: f32,
    pub padding: [f32; 4],
    pub border: [f32; 4],
    pub flags: UiStyleFlags,
    pub font_alignment: UiTextAlignment,
    pub font_offset: [i32; 2],
    pub font_shadow_offset: [i32; 2],
    pub font_bold: bool,
    pub font_underline: bool,
}
