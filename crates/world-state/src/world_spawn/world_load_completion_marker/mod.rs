use bevy::prelude::Component;
use bevy::prelude::SystemSet;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WorldLoadCompletionSet;

/// Durable completion fact for systems whose assets or resources may become
/// available after the one-frame world-load completion notification.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct WorldLoadCompleted;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WorldLoadPreparationSet {
    Accept,
    InitializeClock,
    HydrateTerrain,
}
