//! Authored recipients and effects of a view event.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BehaviorViewEventAction {
    pub view_key: crate::AssetId,
    #[serde(default)]
    pub radius: Option<BehaviorViewEventRadius>,
    #[serde(default)]
    pub education_value_property: Option<crate::AssetId>,
    #[serde(default)]
    pub education_donation_property: Option<crate::AssetId>,
    pub feedback: Option<super::feedback::BehaviorFeedbackAction>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct BehaviorViewEventRadius {
    pub metres: f32,
    pub target_type: Option<crate::AssetId>,
}
