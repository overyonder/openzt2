//! Research-start action.

use super::UiTrigger;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub struct UiStartResearchForSelectedCatalogueItemActionRecord {
    pub trigger: UiTrigger,
}
