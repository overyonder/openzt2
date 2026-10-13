use serde::{Deserialize, Serialize};

/// `BFBehHeadLook`: turn the actor's head toward its target, or release it.
/// It doesn't wait for the head to turn.
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub struct BehaviorHeadLookAction {
    /// `disengage="true"` releases the head instead of aiming it.
    pub disengage: bool,
    /// The target's model node to look at, when authored.
    pub target_node: Option<String>,
}
