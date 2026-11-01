use crate::AssetId;

#[derive(Clone, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub struct EntityNamingDefinition {
    pub id: AssetId,
    pub counted_tags: Vec<AssetId>,
    pub generated_name: Option<GeneratedEntityNameDefinition>,
}

#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub struct GeneratedEntityNameDefinition {
    pub localization_key: AssetId,
    pub counter_tag: AssetId,
}
