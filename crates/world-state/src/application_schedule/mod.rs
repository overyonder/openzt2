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

/// Fixed-update consumers of the authored slow simulation interval.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ZooSlowTick;

/// Real-time consumers that run before the frame's fixed simulation steps.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ZooHeartbeat;
