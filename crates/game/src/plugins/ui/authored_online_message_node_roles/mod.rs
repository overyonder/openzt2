use bevy::prelude::*;

/// The one authored main-menu rail where optional online-message rows are
/// presented. Content transport is owned by the information plugin.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct UiAuthoredOnlineMessageSurface;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct UiAuthoredOnlineMessageList;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct UiAuthoredOnlineMessageText;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct UiAuthoredOnlineMessageIcon;
