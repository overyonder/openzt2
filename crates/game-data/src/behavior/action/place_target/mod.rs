use serde::{Deserialize, Serialize};

use crate::AssetId;

/// `ZTBehPlaceTarget`: the subject places a new object for its target, as a
/// keeper puts down the food its target animal eats.
///
/// The object type comes from the target, it is held in the subject's named
/// container while `placement_behavior_set` takes the subject to the chosen
/// site, and it enters the world when that set finishes.
#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct BehaviorPlaceTargetAction {
    pub placed_type: BehaviorPlacedTargetType,
    /// Subject entity container holding the object until it is put down.
    pub holding_container: AssetId,
    /// Behavior set the subject plays with the placement site as its target.
    pub placement_behavior_set: AssetId,
}

/// The target string attribute naming the definition to place.
#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum BehaviorPlacedTargetType {
    /// The target animal's `s_KeeperEatType`.
    KeeperFood,
    /// The target animal's `s_KeeperDrinkType`.
    KeeperDrink,
}
