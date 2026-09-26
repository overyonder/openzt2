use super::{UiStyleFlags, UiStyleRecord, UiSystemFont, UiTextAlignment};
use crate::AssetId;

impl UiStyleFlags {
    pub const NINE_SLICE: Self = Self(1 << 0);
    pub const PRESERVE_ASPECT: Self = Self(1 << 1);
    pub const WRAP_TEXT: Self = Self(1 << 2);
    pub const ELLIPSIS: Self = Self(1 << 3);
    pub const PRESSED_OFFSET: Self = Self(1 << 4);
    pub const DRAW_3D: Self = Self(1 << 5);
    pub const ALL: u16 = (1 << 6) - 1;
}

impl std::ops::BitOr for UiStyleFlags {
    type Output = Self;

    fn bitor(self, additional_flags: Self) -> Self::Output {
        Self(self.0 | additional_flags.0)
    }
}

impl Default for UiStyleRecord {
    fn default() -> Self {
        Self {
            background: AssetId::default(),
            background_source: [0; 4],
            background_width: 0.0,
            font: UiSystemFont::default(),
            text_key: AssetId::default(),
            color: [1.0; 4],
            font_px: 16.0,
            padding: [0.0; 4],
            border: [0.0; 4],
            flags: UiStyleFlags::default(),
            font_alignment: UiTextAlignment::default(),
            font_offset: [0; 2],
            font_shadow_offset: [0; 2],
            font_bold: false,
            font_underline: false,
        }
    }
}
