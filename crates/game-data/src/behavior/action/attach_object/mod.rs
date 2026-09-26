use crate::AssetId;
use serde::{Deserialize, Serialize};

/// Authored object creation and the named rule to use when it is later detached.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BehaviorAttachObjectAction {
    pub entity: AssetId,
    pub animation: Option<String>,
    pub container: AssetId,
    pub detach_rule: AssetId,
}
