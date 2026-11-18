use bevy::{ecs::system::SystemParam, prelude::*};
use openzt2_game_data::{ui_document::document::UiDocumentRole, AssetId};

use openzt2_game_assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;

use crate::input::input_types::ActionSource;

#[derive(Message, Debug, Clone)]
pub struct ShowUiDocument {
    pub document: Handle<UiDocumentAsset>,
    pub owner: Entity,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShowUiRole {
    pub role: UiDocumentRole,
    pub owner: Entity,
}

#[derive(Message)]
pub struct AuthoredUiDocumentOperation {
    pub owner: Entity,
    pub role: UiDocumentRole,
    pub operation: UiDocumentOperation,
    pub transfer_pause: bool,
}

#[derive(Clone, Copy)]
pub enum UiDocumentOperation {
    Open,
    Hide,
    ShowNode {
        target: AssetId,
        visible: bool,
    },
    ActivateSelectedChild {
        target: AssetId,
        source: ActionSource,
    },
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct HideUiDocument {
    pub owner: Entity,
}

/// Owner of the running game's view. Its documents live as long as the game.
#[derive(Component)]
pub struct InGameUiOwner;

/// Menus and dialogs opened during a game belong with the game view, as when
/// F5 and F6 open the save and load dialogs, so a menu or prompt that closes
/// as it opens another cannot take that one, or navigation waiting on its
/// save, along with it. Outside a game they belong with their opener.
#[derive(SystemParam)]
pub struct GameViewDocumentOwner<'w, 's> {
    game_views: Query<'w, 's, Entity, With<InGameUiOwner>>,
}

impl GameViewDocumentOwner<'_, '_> {
    pub fn owner(&self, opening_document_owner: Entity) -> Entity {
        self.game_views
            .iter()
            .next()
            .unwrap_or(opening_document_owner)
    }
}
