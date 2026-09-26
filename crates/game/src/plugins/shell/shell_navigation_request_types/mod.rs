use bevy::prelude::*;
use openzt2_game_data::AssetId;

use crate::game_session_types::WorldSessionMode;

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectWorldSessionMode(pub WorldSessionMode);

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChooseWorld(pub AssetId);

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct StartSelectedWorld;

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct NavigateShellBack;

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShowOptions;

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShowDownloads;

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShowSavedGames;

/// Requests the application-level transition back to the main menu.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReturnToMainMenu;

/// Requests a clean Bevy application exit.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExitApplication;

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct SplashFinished;
