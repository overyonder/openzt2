//! Ordered application update and fixed-step scheduling policy.

use bevy::prelude::*;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GameSet {
    Input,
    Intent,
    Ui,
    Presentation,
    Diagnostics,
}

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FixedGameSet {
    Clock,
    Think,
    Navigate,
    Act,
    Economy,
    Cleanup,
}
