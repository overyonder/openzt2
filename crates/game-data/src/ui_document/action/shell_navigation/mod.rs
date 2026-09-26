//! Splash, main-menu, options, downloads, saved-game, exit, and globe actions.

use super::UiTrigger;
use crate::AssetId;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub struct UiShellActionRecord {
    pub trigger: UiTrigger,
    pub action: UiShellAction,
}

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub enum UiShellAction {
    FinishSplashPresentation,
    NavigateBackFromDownloads,
    NavigateBackFromMapSelection,
    SelectFreeformModeOrStartSelectedWorld,
    SelectChallengeModeOrStartSelectedWorld,
    SelectCampaignModeOrStartSelectedWorld,
    StartSelectedWorld,
    ShowOptions,
    ShowDownloads,
    ShowSavedGames,
    ShowExitConfirmationDialog,
    ExitApplication,
    DismissExitConfirmationDialog,
    MarkExitConfirmationPending,
    ReturnToMainMenu,
    ShowInGameOptionsOverlay,
    ReturnToMainMenuAfterWorldSnapshotSave,
    ExitApplicationAfterWorldSnapshotSave,
    CaptureScreenshotFromSoleActive3dCamera,
    FilterWorldChoicesToLocation { world_location: AssetId },
    FilterWorldChoicesToExpansionPack { expansion_pack_identifier: u16 },
    ShowSecondaryGlobeBiome { biome: AssetId },
    NavigateBackFromOptions,
}
