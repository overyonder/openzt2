use crate::AssetId;
use serde::{Deserialize, Serialize};

/// Authored object creation and the named rule to use when it is later detached.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BehaviorAttachObjectAction {
    pub entity: AssetId,
    pub animation: Option<String>,
    pub container: AssetId,
    pub detach_rule: AssetId,
    /// The authored carrying gait, such as `carry`.
    #[serde(default)]
    pub locomotion_modifier: Option<AssetId>,
    /// The subject's node that holds the item, such as `Bip01 Head` for a hat.
    #[serde(default)]
    pub subject_node: Option<String>,
    /// The target's node the item is taken from, such as a dish's
    /// `Dock_Attach`. Authored only with the target's held prop.
    #[serde(default)]
    pub target_node: Option<String>,
    /// Behavior played after the item detaches, before the next instruction.
    #[serde(default)]
    pub detach_behavior_set: Option<AssetId>,
}

impl BehaviorAttachObjectAction {
    /// `attachEntity="s_prop"`: the item is the target container's authored
    /// held prop, and nothing when that container holds none.
    #[must_use]
    pub fn attaches_target_held_prop(&self) -> bool {
        self.entity == AssetId::from_key("s_prop")
    }
}

/// `BFBehDetachObject`: apply a named detach rule to an item the actor holds,
/// such as turning an eaten burger into trash.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BehaviorDetachObjectAction {
    /// The item's type, or `object` for the item the task uses.
    pub entity: AssetId,
    pub detach_rule: AssetId,
}
