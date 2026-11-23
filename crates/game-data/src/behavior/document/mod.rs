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

impl BehaviorSet {
    /// Whether a running set's `program` names this set. `BFBehPlaySet`
    /// requests a set by its authored name, such as `Queue`, and keeps that
    /// request as the program, so the name keys of the set index count too.
    #[must_use]
    pub fn answers_program(&self, program: AssetId) -> bool {
        let name = self.name.trim();
        self.id == program
            || AssetId::from_key(name) == program
            || AssetId::from_key(&name.to_ascii_lowercase()) == program
    }
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
    /// The sensor that offers this task's targets, such as `viewanimals` for
    /// paths with an animal in view.
    #[serde(default)]
    pub sensor_tag: Option<AssetId>,
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
}

impl BehaviorTaskActionPhase {
    #[must_use]
    pub fn lowered_actions(&self) -> &[BehaviorAction] {
        let Self::Supported(actions) = self;
        actions
    }

    #[must_use]
    pub fn contains_invalid_actions(&self) -> bool {
        self.lowered_actions()
            .iter()
            .any(|action| matches!(action, BehaviorAction::InvalidAuthoredAction(_)))
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.lowered_actions().is_empty()
    }
}

#[cfg(test)]
mod set_program_tests {
    use super::*;

    #[test]
    fn a_set_answers_its_identifier_and_its_requested_name() {
        let set = BehaviorSet {
            id: AssetId::from_key("ai/tasks/guests/guest.beh#queue@guest"),
            name: "Queue".to_owned(),
            subjects: vec!["Guest".to_owned()],
            actions: BehaviorTaskActionPhase::Supported(Vec::new()),
        };
        assert!(set.answers_program(set.id));
        assert!(set.answers_program(AssetId::from_key("Queue")));
        assert!(set.answers_program(AssetId::from_key("queue")));
        assert!(!set.answers_program(AssetId::from_key("usecontainer")));
    }
}
