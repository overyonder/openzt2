use bevy::prelude::*;
use openzt2_game_data::ui_document::document::UiDocumentRole;
use openzt2_game_data::AssetId;

use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use crate::plugins::ui::authored_ui_node_projection_components::UiDocumentOwner;
use crate::plugins::ui::authored_ui_node_projection_components::UiDocumentRoot;

use crate::plugins::information::{
    catalogue_types::SelectedCatalogueEntry, entity_selection_types::Inspectable,
};

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::plugins::information) struct PendingZoopediaEntry(
    pub(in crate::plugins::information) AssetId,
);

pub(in crate::plugins::information) struct ZoopediaNavigationActionContext<'a, 'world, 'state> {
    pub(in crate::plugins::information) source_document_role: &'a UiDocumentRole,
    pub(in crate::plugins::information) source_document_entity: Entity,
    pub(in crate::plugins::information) source_lifecycle_owner_entity: Entity,
    pub(in crate::plugins::information) selected_world_entity: Option<Entity>,
    pub(in crate::plugins::information) selected_catalogue_entry: &'a SelectedCatalogueEntry,
    pub(in crate::plugins::information) world_definitions: Option<WorldDefinitionsView<'a>>,
    pub(in crate::plugins::information) inspectable_world_entities:
        &'a Query<'world, 'state, &'static Inspectable>,
    pub(in crate::plugins::information) document_roots:
        &'a Query<'world, 'state, (&'static UiDocumentRoot, &'static ChildOf)>,
    pub(in crate::plugins::information) zoopedia_pages: &'a mut Query<
        'world,
        'state,
        (
            Entity,
            &'static UiDocumentOwner,
            &'static mut ZoopediaPage,
            Option<&'static mut ZoopediaHistory>,
        ),
    >,
    pub(in crate::plugins::information) commands: &'a mut Commands<'world, 'state>,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::plugins::information) struct ZoopediaPage {
    pub(in crate::plugins::information) subject: AssetId,
    pub(in crate::plugins::information) section: u16,
}

/// Bounded browser history for one Zoopedia panel.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::plugins::information) struct ZoopediaHistory {
    entries: [AssetId; 32],
    cursor: u8,
    len: u8,
}

impl ZoopediaHistory {
    pub(in crate::plugins::information) fn from_transition(
        previous_subject: AssetId,
        current_subject: AssetId,
    ) -> Self {
        let mut subject_entries = [AssetId::default(); 32];
        subject_entries[0] = previous_subject;
        let subject_changed = previous_subject != current_subject;
        if subject_changed {
            subject_entries[1] = current_subject;
        }
        Self {
            entries: subject_entries,
            cursor: u8::from(subject_changed),
            len: 1 + u8::from(subject_changed),
        }
    }

    pub(in crate::plugins::information) fn visit_subject(&mut self, subject: AssetId) {
        if self.entries[usize::from(self.cursor)] == subject {
            return;
        }
        let next_entry_index = usize::from(self.cursor).saturating_add(1);
        if next_entry_index < self.entries.len() {
            self.entries[next_entry_index] = subject;
            self.cursor = next_entry_index as u8;
            self.len = self.cursor.saturating_add(1);
        } else {
            self.entries.copy_within(1.., 0);
            self.entries[31] = subject;
            self.cursor = 31;
            self.len = self.entries.len() as u8;
        }
    }

    pub(in crate::plugins::information) fn visit_previous_subject(&mut self) -> Option<AssetId> {
        (self.cursor > 0).then(|| {
            self.cursor -= 1;
            self.entries[usize::from(self.cursor)]
        })
    }

    pub(in crate::plugins::information) fn visit_next_subject(&mut self) -> Option<AssetId> {
        (self.cursor.saturating_add(1) < self.len).then(|| {
            self.cursor += 1;
            self.entries[usize::from(self.cursor)]
        })
    }

    pub(in crate::plugins::information) fn previous_subject_is_available(&self) -> bool {
        self.cursor > 0
    }

    pub(in crate::plugins::information) fn next_subject_is_available(&self) -> bool {
        self.cursor.saturating_add(1) < self.len
    }
}
