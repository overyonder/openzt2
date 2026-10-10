use bevy::{ecs::system::SystemParam, prelude::*};
use openzt2_game_data::ui_document::document::UiDocumentRole;

use crate::assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;

#[derive(Message, Debug, Clone)]
pub(crate) struct ShowUiDocument {
    pub(crate) document: Handle<UiDocumentAsset>,
    pub(crate) owner: Entity,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ShowUiRole {
    pub(crate) role: UiDocumentRole,
    pub(crate) owner: Entity,
}

#[derive(Resource, Default)]
pub(crate) struct UiRoleRequests(Vec<ShowUiRole>);

impl UiRoleRequests {
    pub(crate) fn request(&mut self, role: UiDocumentRole, owner: Entity) {
        self.0.push(ShowUiRole { role, owner });
    }

    pub(super) fn take(&mut self) -> Vec<ShowUiRole> {
        std::mem::take(&mut self.0)
    }

    pub(super) fn retain(&mut self, request: ShowUiRole) {
        self.0.push(request);
    }
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HideUiDocument {
    pub(crate) owner: Entity,
}

/// Owner of the running game's view. Its documents live as long as the game.
#[derive(Component)]
pub(crate) struct InGameUiOwner;

/// Menus and dialogs opened during a game belong with the game view, as when
/// F5 and F6 open the save and load dialogs, so a menu or prompt that closes
/// as it opens another cannot take that one, or navigation waiting on its
/// save, along with it. Outside a game they belong with their opener.
#[derive(SystemParam)]
pub(crate) struct GameViewDocumentOwner<'w, 's> {
    game_views: Query<'w, 's, Entity, With<InGameUiOwner>>,
}

impl GameViewDocumentOwner<'_, '_> {
    pub(crate) fn owner(&self, opening_document_owner: Entity) -> Entity {
        self.game_views
            .iter()
            .next()
            .unwrap_or(opening_document_owner)
    }
}
