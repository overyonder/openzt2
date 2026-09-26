//! Authored Zoopedia page identity, content, hierarchy, and display order.

use crate::AssetId;

#[derive(Clone, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub struct ZoopediaEntry {
    pub id: AssetId,
    pub subject: AssetId,
    pub title_key: AssetId,
    pub body_key: AssetId,
    pub image: AssetId,
    pub related: Vec<AssetId>,
    pub order: u16,
}
