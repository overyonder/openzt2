use bevy::prelude::*;
use openzt2_game_data::AssetId;

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ShowNotification {
    pub(crate) operation: Entity,
    pub(crate) message: AssetId,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NotificationPresented {
    pub(crate) operation: Entity,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NotificationRejected {
    pub(crate) operation: Entity,
    pub(crate) reason: NotificationFailure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NotificationFailure {
    MissingLocalization(AssetId),
    ProjectionFailed,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PendingNotification {
    pub(crate) operation: Entity,
    pub(crate) message: AssetId,
}
