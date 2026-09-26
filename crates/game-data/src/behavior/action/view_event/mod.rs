//! An authored view event delivered through the source actor's registered viewers.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BehaviorViewEventAction {
    pub view_key: crate::AssetId,
    pub feedback: Option<super::feedback::BehaviorFeedbackAction>,
}
