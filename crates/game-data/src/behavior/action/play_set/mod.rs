//! Selection of an authored behavior set through the current interaction container.

use crate::AssetId;

#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub struct BehaviorPlaySetAction {
    pub behavior_set_asset_id: AssetId,
}
