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

/// Orders camera water queries, environment overrides, and dependent rendering.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WaterPresentationSet {
    View,
    Environment,
    Rendering,
}
