use bevy::{ecs::system::SystemParam, prelude::*};
use openzt2_game_data::ui_document::document::UiDocumentRole;

use openzt2_game_assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;

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

#[derive(Resource, Default)]
pub struct UiRoleRequests(Vec<PendingUiRoleRequest>);

pub struct PendingUiRoleRequest {
    pub request: ShowUiRole,
    pub preserve_temporary_pause: bool,
}

impl UiRoleRequests {
    pub fn request(&mut self, role: UiDocumentRole, owner: Entity) {
        self.request_preserving_temporary_pause(role, owner, false);
    }

    pub fn request_preserving_temporary_pause(
        &mut self,
        role: UiDocumentRole,
        owner: Entity,
        preserve_temporary_pause: bool,
    ) {
        if let Some(pending) = self
            .0
            .iter_mut()
            .find(|pending| pending.request.role == role && pending.request.owner == owner)
        {
            pending.preserve_temporary_pause |= preserve_temporary_pause;
            return;
        }
        self.0.push(PendingUiRoleRequest {
            request: ShowUiRole { role, owner },
            preserve_temporary_pause,
        });
    }

    pub fn temporary_pause_transfer_owners(&self) -> impl Iterator<Item = Entity> + '_ {
        self.0
            .iter()
            .filter(|pending| pending.preserve_temporary_pause)
            .map(|pending| pending.request.owner)
    }

    pub fn cancel(&mut self, owner: Entity, role: Option<UiDocumentRole>) {
        self.0.retain(|pending| {
            pending.request.owner != owner || role.is_some_and(|role| pending.request.role != role)
        });
    }

    pub fn take(&mut self) -> Vec<PendingUiRoleRequest> {
        std::mem::take(&mut self.0)
    }

    pub fn retain(&mut self, request: PendingUiRoleRequest) {
        self.0.push(request);
    }
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
