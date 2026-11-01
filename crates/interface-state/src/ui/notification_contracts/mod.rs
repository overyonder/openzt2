use bevy::prelude::*;
use openzt2_game_data::AssetId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotificationMessage {
    Localized(AssetId),
    Text(String),
}

#[derive(Message, Debug, Clone, PartialEq, Eq)]
pub struct ShowNotification {
    pub operation: Entity,
    pub message: NotificationMessage,
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

#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct PendingNotification {
    pub operation: Entity,
    pub message: NotificationMessage,
}
