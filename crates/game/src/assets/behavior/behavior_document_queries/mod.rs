//! Queries over loaded behavior documents and their precedence-aware declaration indexes.

use bevy::prelude::*;
use openzt2_game_data::{
    behavior::document::{BehaviorDocument, BehaviorSet, BehaviorTask},
    AssetId,
};

use super::behavior_asset_types::{
    BehaviorDeclarationIndexView, BehaviorDeclarationLocation, BehaviorDocumentAsset,
    LoadedBehaviorDocumentCollection,
};

impl BehaviorDocumentAsset {
    pub(crate) fn behavior_set_at_index(&self, declaration_index: usize) -> Option<&BehaviorSet> {
        match &self.behavior_document {
            BehaviorDocument::Sets(behavior_sets) => behavior_sets.get(declaration_index),
            BehaviorDocument::Tasks(_) => None,
        }
    }

    pub(crate) fn behavior_task_at_index(&self, declaration_index: usize) -> Option<&BehaviorTask> {
        match &self.behavior_document {
            BehaviorDocument::Tasks(behavior_tasks) => behavior_tasks.get(declaration_index),
            BehaviorDocument::Sets(_) => None,
        }
    }
}

impl LoadedBehaviorDocumentCollection {
    pub(crate) fn create_declaration_index_view<'a>(
        &'a self,
        behavior_document_assets: &'a Assets<BehaviorDocumentAsset>,
    ) -> Option<BehaviorDeclarationIndexView<'a>> {
        (self.indexed_archive_revision.is_some()
            && self.indexed_archive_revision == self.archive_revision)
            .then_some(BehaviorDeclarationIndexView {
                behavior_sets_by_identifier: &self.behavior_sets_by_identifier,
                behavior_tasks_by_unique_identifier: &self.behavior_tasks_by_unique_identifier,
                behavior_document_assets,
            })
    }
}

impl<'a> BehaviorDeclarationIndexView<'a> {
    /// Borrows one winning declaration per authored UniqueID in stable key order.
    pub(crate) fn behavior_tasks(
        self,
    ) -> impl Iterator<Item = (&'a Handle<BehaviorDocumentAsset>, usize, &'a BehaviorTask)> {
        self.behavior_tasks_by_unique_identifier
            .values()
            .filter_map(move |locations| {
                let location = locations.last()?;
                let task = self
                    .behavior_document_assets
                    .get(&location.behavior_document_asset)?
                    .behavior_task_at_index(location.declaration_index)?;
                Some((
                    &location.behavior_document_asset,
                    location.declaration_index,
                    task,
                ))
            })
    }

    pub(crate) fn find_behavior_task_location(
        self,
        behavior_task_unique_identifier: AssetId,
    ) -> Option<(Handle<BehaviorDocumentAsset>, usize, &'a BehaviorTask)> {
        let declaration_location = self
            .behavior_tasks_by_unique_identifier
            .get(&behavior_task_unique_identifier)?
            .last()?;
        let behavior_task = self
            .behavior_document_assets
            .get(&declaration_location.behavior_document_asset)?
            .behavior_task_at_index(declaration_location.declaration_index)?;
        Some((
            declaration_location.behavior_document_asset.clone(),
            declaration_location.declaration_index,
            behavior_task,
        ))
    }

    pub(crate) fn find_behavior_set_location(
        self,
        behavior_set_identifier: AssetId,
        subject_type_identifiers: impl Iterator<Item = AssetId> + Clone,
    ) -> Option<(Handle<BehaviorDocumentAsset>, usize)> {
        let declaration_locations = self
            .behavior_sets_by_identifier
            .get(&behavior_set_identifier)?;
        let matching_declaration = declaration_locations.iter().rev().find(|location| {
            self.behavior_set_at_location(location)
                .is_some_and(|behavior_set| {
                    behavior_set.id == behavior_set_identifier
                        || behavior_set.subjects.is_empty()
                        || behavior_set.subjects.iter().any(|authored_subject_type| {
                            let required = AssetId::from_key(
                                &authored_subject_type.trim().to_ascii_lowercase(),
                            );
                            subject_type_identifiers
                                .clone()
                                .any(|subject| subject == required)
                        })
                })
        })?;
        Some((
            matching_declaration.behavior_document_asset.clone(),
            matching_declaration.declaration_index,
        ))
    }

    fn behavior_set_at_location(
        self,
        location: &BehaviorDeclarationLocation,
    ) -> Option<&'a BehaviorSet> {
        match &self
            .behavior_document_assets
            .get(&location.behavior_document_asset)?
            .behavior_document
        {
            BehaviorDocument::Sets(behavior_sets) => behavior_sets.get(location.declaration_index),
            BehaviorDocument::Tasks(_) => None,
        }
    }
}
