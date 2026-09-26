use crate::AssetId;
use serde::{Deserialize, Serialize};

/// Source-resolved alternatives for one file-skin replacement group.
/// Presentation chooses one candidate and applies it to every target.
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub struct UiImageSelectionGroupDefinition {
    pub target_node_ids: Vec<AssetId>,
    pub candidate_image_ids: Vec<AssetId>,
}
