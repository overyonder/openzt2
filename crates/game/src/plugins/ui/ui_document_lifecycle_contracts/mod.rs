use bevy::prelude::*;
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
