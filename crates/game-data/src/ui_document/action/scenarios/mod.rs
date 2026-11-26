//! Scenario objectives, challenges, campaigns, awards, maps, and tutorials.

use super::UiTrigger;
use crate::AssetId;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub struct UiScenarioActionRecord {
    pub trigger: UiTrigger,
    pub action: UiScenarioAction,
}
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub enum UiScenarioAction {
    SetObjectiveFilter { filter: AssetId },
    SetObjectiveStatusFilter { filter: ObjectiveStatusFilter },
    AcceptChallenge,
    DeclineChallenge,
    MarkAwardsSeen,
    IncrementDiseaseHint,
    PopulateScenarioSelection,
    PopulateCampaignSelection,
    PlayNextScenario,
    SelectionChanged { scenario_path: Option<String> },
    ClearSelection,
    SetStartingCash,
    PlayTutorial,
}

#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectiveStatusFilter {
    All,
    Success,
    Failure,
    Neutral,
}
