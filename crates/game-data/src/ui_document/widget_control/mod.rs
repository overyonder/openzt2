use super::node_layout::UiNodeRegionDefinition;
use crate::AssetId;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum UiTooltipPresentation {
    Name,
    Short,
    Long,
    Help,
}

#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum UiDragOperation {
    Move,
    Resize,
    Scroll,
}

#[derive(Deserialize, Serialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UiGridRecord {
    pub auto_size: bool,
    pub auto_size_parent: bool,
    pub columns: i32,
    pub rows: i32,
    pub x_spacing: i32,
    pub y_spacing: i32,
    pub column_width: i32,
    pub row_height: i32,
    pub initial_x: i32,
    pub initial_y: i32,
}

// These booleans are independent authored button behaviors.
#[allow(
    clippy::struct_excessive_bools,
    reason = "independent authored button flags"
)]
#[derive(Deserialize, Serialize, Clone, Copy, Debug, Default, PartialEq)]
pub struct UiButtonRecord {
    pub auto_size: bool,
    pub minimum_height: i32,
    /// Initial selected state authored by the source `toggle` attribute.
    /// Whether a node is selectable is carried by `UiNodeKind::Toggle`.
    pub initially_selected: bool,
    pub sticky: bool,
    pub repress: bool,
    pub repeat_delay_seconds: f32,
    pub hold_change: f32,
    pub hold_interval_cap_seconds: f32,
    pub broadcast: bool,
    pub delayed_activation_seconds: f32,
    pub activation_data: AssetId,
    pub child_button: AssetId,
    pub hover_child: AssetId,
}

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq)]
pub struct UiSliderRecord {
    pub minimum: f32,
    pub maximum: f32,
    pub increment: f32,
    pub initial: f32,
    pub minimum_thumb_size: f32,
    pub axis: UiAxis,
    pub value_type: AssetId,
    pub span: f32,
    pub thumb: AssetId,
    pub thumb_region: UiNodeRegionDefinition,
    pub style: AssetId,
    pub field: AssetId,
    pub field_format: UiNumericFormat,
    pub on_change: AssetId,
}

#[derive(Deserialize, Serialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UiNumericFormat {
    pub minimum_width: u8,
    pub decimal_places: u8,
    pub percent_suffix: bool,
}

#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum UiAxis {
    X,
    Y,
    Both,
}

#[derive(Deserialize, Serialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct UiTextRecord {
    pub auto_size: bool,
    pub minimum_height: i32,
    pub localization_key: AssetId,
    pub value: String,
    pub format: AssetId,
}
