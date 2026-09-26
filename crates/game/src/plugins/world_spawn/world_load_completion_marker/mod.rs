use bevy::prelude::Component;

/// Durable completion fact for systems whose assets or resources may become
/// available after the one-frame world-load completion notification.
#[derive(Component, Debug, Clone, Copy, Default)]
pub(crate) struct WorldLoadCompleted;
