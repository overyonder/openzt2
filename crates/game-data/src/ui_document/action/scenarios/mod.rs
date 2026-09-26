//! Scenario objectives, challenges, campaigns, earthquakes, awards, maps, and tutorials.

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
    TriggerEarthquake,
    RefreshEarthquake,
    MarkAwardsSeen,
    IncrementDiseaseHint,
    PopulateScenarioSelection,
    PopulateCampaignSelection,
    PlayNextScenario,
    SelectionChanged,
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
