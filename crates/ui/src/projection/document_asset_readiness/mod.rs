use std::collections::HashSet;

use bevy::{
    asset::{AssetLoadFailedEvent, RecursiveDependencyLoadState, UntypedAssetLoadFailedEvent},
    ecs::system::SystemParam,
    prelude::*,
};
use openzt2_game_assets::{
    localization::{
        localization_asset_types::LocalizationAsset,
        localization_precedence_index::LocalizationPrecedenceIndex,
    },
    ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset,
};

#[derive(SystemParam)]
pub(crate) struct UiDocumentAssetEvents<'w, 's> {
    documents: MessageReader<'w, 's, AssetEvent<UiDocumentAsset>>,
    failures: MessageReader<'w, 's, AssetLoadFailedEvent<UiDocumentAsset>>,
    dependency_failures: MessageReader<'w, 's, UntypedAssetLoadFailedEvent>,
    localization: Res<'w, LocalizationPrecedenceIndex>,
    localizations: Res<'w, Assets<LocalizationAsset>>,
}

impl UiDocumentAssetEvents<'_, '_> {
    pub(super) fn collect_changes(&mut self) -> UiDocumentAssetChanges {
        let mut documents = self
            .documents
            .read()
            .filter_map(|event| match *event {
                AssetEvent::Added { id }
                | AssetEvent::Modified { id }
                | AssetEvent::LoadedWithDependencies { id } => Some(id),
                _ => None,
            })
            .collect::<HashSet<_>>();
        documents.extend(self.failures.read().map(|failure| failure.id));
        UiDocumentAssetChanges {
            documents,
            shared_inputs_changed: self.dependency_failures.read().count() != 0
                || self.localization.is_changed()
                || self.localizations.is_changed(),
        }
    }
}

pub(super) struct UiDocumentAssetChanges {
    documents: HashSet<AssetId<UiDocumentAsset>>,
    shared_inputs_changed: bool,
}

impl UiDocumentAssetChanges {
    pub(super) fn affects(&self, id: AssetId<UiDocumentAsset>) -> bool {
        self.shared_inputs_changed || self.documents.contains(&id)
    }

    pub(super) fn is_empty(&self) -> bool {
        !self.shared_inputs_changed && self.documents.is_empty()
    }
}

pub(super) fn document_dependencies_resolved(
    server: &AssetServer,
    handle: &Handle<UiDocumentAsset>,
) -> bool {
    // A dependency failure leaves optional presentation data unavailable.
    // Project the document so its image and audio fallbacks can take effect.
    !matches!(
        server.get_recursive_dependency_load_state(handle.id()),
        Some(RecursiveDependencyLoadState::Loading)
    )
}
