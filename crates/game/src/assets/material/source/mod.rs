//! Blue Fang material parameter deserialization.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(rename = "material")]
pub(super) struct BlueFangMaterialSourceDocument {
    #[serde(rename = "@fx")]
    pub(super) effect_source: Option<String>,
    #[serde(rename = "param", default)]
    pub(super) parameter_assignments: Vec<D3d9EffectParameterAssignment>,
}

#[derive(Debug, Deserialize)]
pub(super) struct D3d9EffectParameterAssignment {
    #[serde(rename = "@name")]
    pub(super) parameter_name: String,
    #[serde(rename = "@type")]
    pub(super) parameter_type: String,
    #[serde(rename = "$text", default)]
    pub(super) authored_value: String,
}
