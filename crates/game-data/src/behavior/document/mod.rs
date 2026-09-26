use serde::{Deserialize, Serialize};

use crate::AssetId;

use super::{
    action_record::BehaviorAction, eligibility::BehaviorCandidateEligibilityRequirements,
    score::BehaviorScore,
};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum BehaviorDocument {
    Sets(Vec<BehaviorSet>),
    Tasks(Vec<BehaviorTask>),
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BehaviorSet {
    pub id: AssetId,
    pub name: String,
    pub subjects: Vec<String>,
    pub actions: BehaviorTaskActionPhase,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BehaviorTask {
    pub id: AssetId,
    pub name: String,
    pub priority: Option<f32>,
    #[serde(default)]
    pub task_delay_seconds: Option<[f32; 2]>,
    #[serde(default)]
    pub reservation_tag: Option<AssetId>,
    pub subjects: Vec<String>,
    pub targets: Vec<String>,
    pub objects: Vec<String>,
    pub candidate_eligibility_requirements: Vec<BehaviorCandidateEligibilityRequirements>,
    pub scores: Vec<BehaviorScore>,
    pub execution: BehaviorTaskActionPhase,
    pub completion: BehaviorTaskActionPhase,
    pub failure: BehaviorTaskActionPhase,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum BehaviorTaskActionPhase {
    Supported(Vec<BehaviorAction>),
    SupportedPrefix {
        actions: Vec<BehaviorAction>,
        first_unsupported_action: String,
    },
    Unsupported {
        first_unsupported_action: String,
    },
}

impl BehaviorTaskActionPhase {
    #[must_use]
    pub fn lowered_actions(&self) -> &[BehaviorAction] {
        match self {
            Self::Supported(actions) | Self::SupportedPrefix { actions, .. } => actions,
            Self::Unsupported { .. } => &[],
        }
    }

    #[must_use]
    pub const fn has_unsupported_suffix(&self) -> bool {
        matches!(
            self,
            Self::SupportedPrefix { .. } | Self::Unsupported { .. }
        )
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        !self.has_unsupported_suffix() && self.lowered_actions().is_empty()
    }
}
