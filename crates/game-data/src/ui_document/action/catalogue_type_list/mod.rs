//! Catalogue type-list population actions.

use super::UiTrigger;
use crate::AssetId;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub struct UiPopulateCatalogueTypeListActionRecord {
    pub trigger: UiTrigger,
    pub target_type_list_node: AssetId,
}
