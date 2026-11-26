use serde::{Deserialize, Serialize};

use super::UiTrigger;
use crate::{ui_document::document::UiDocumentRole, AssetId};

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub struct UiScriptActionRecord {
    pub trigger: UiTrigger,
    pub context: String,
    pub script: Option<AssetId>,
    pub authored_path: String,
    pub entry: String,
    pub arguments: Vec<String>,
    pub receiver: AssetId,
    pub document_role: Option<UiDocumentRole>,
}
