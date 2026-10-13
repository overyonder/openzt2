use bevy::prelude::*;
use openzt2_game_data::AssetId;

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShowNotification {
    pub operation: Entity,
    pub message: AssetId,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct NotificationPresented {
    pub operation: Entity,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct NotificationRejected {
    pub operation: Entity,
    pub reason: NotificationFailure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationFailure {
    MissingLocalization(AssetId),
    ProjectionFailed,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PendingNotification {
    pub operation: Entity,
    pub message: AssetId,
}
