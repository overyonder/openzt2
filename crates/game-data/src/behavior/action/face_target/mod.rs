use serde::{Deserialize, Serialize};

/// `BFBehFaceTarget`: turn the subject toward its task target.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq)]
pub struct BehaviorFaceTargetAction {
    /// A heading already within this angle of the target is kept.
    pub rotation_tolerance_radians: f32,
    /// Added to the heading toward the target.
    pub rotation_offset_radians: f32,
}
