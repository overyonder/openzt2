use bevy::prelude::*;
use openzt2_game_data::ui_document::action::presentation::UiPresentationAction;

#[derive(Message, Debug, Clone)]
pub struct RequestUiPresentationAction {
    pub document_root: Entity,
    pub action: UiPresentationAction,
}

/// Marks a gameplay-owned thought or emote bubble addressed by authored
/// presentation actions.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct EmotePresentation;

/// One of the mutually exclusive animated hover pointers in the authored main menu.
#[derive(Component)]
pub(super) struct UiExclusiveHoverPresentation;

/// Messages cached by an authored control. Each advance reveals the next
/// available entry and leaves the last entry displayed at the end of the queue.
#[derive(Component, Default)]
pub struct UiCachedMessages {
    messages: Vec<String>,
    next: usize,
}

impl UiCachedMessages {
    pub fn new(messages: Vec<String>) -> Self {
        Self { messages, next: 0 }
    }
}

pub(crate) mod presentation_batch;
