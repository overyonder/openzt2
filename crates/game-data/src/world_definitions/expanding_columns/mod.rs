use crate::AssetId;

#[allow(
    clippy::derive_partial_eq_without_eq,
    reason = "authored piece heights are floating point"
)]
#[derive(Clone, Copy, Debug, serde::Deserialize, PartialEq, serde::Serialize)]
pub struct ExpandingColumnPresentationDefinition {
    pub base_prefab: AssetId,
    pub repeating_prefab: AssetId,
    pub top_prefab: AssetId,
    pub base_attachment: AssetId,
    pub repeating_attachment: AssetId,
    pub repeating_next_attachment: AssetId,
    pub top_attachment: AssetId,
    pub base_height_metres: f32,
    pub repeating_height_metres: f32,
    pub top_height_metres: f32,
}
