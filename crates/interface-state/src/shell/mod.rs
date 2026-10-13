//! Shell screen and world-choice state shared with the interface and camera.

pub mod shell_screen_presentation_types;
pub mod shell_selection_types;

use bevy::prelude::*;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ShellNavigationAfterSettings;
