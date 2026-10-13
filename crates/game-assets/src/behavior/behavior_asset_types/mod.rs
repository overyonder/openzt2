//! Loaded behavior-document asset and precedence-index data.

use std::collections::BTreeMap;

use bevy::prelude::*;
use openzt2_game_data::{behavior::document::BehaviorDocument, AssetId};

#[derive(Asset, TypePath, Debug)]
pub struct BehaviorDocumentAsset {
    pub(super) behavior_document: BehaviorDocument,
    pub(super) scripts: BTreeMap<
        AssetId,
        (
            String,
            Handle<crate::lua_script::lua_script_asset_loading::LuaScriptAsset>,
        ),
    >,
}

impl BehaviorDocumentAsset {
    #[cfg(any(test, feature = "testing"))]
    pub fn from_test_source(source: &[u8]) -> Self {
        use crate::source_document::{
            blue_fang_source_document_parsing::parse_blue_fang_source_document, path::AssetPath,
        };

        let document = parse_blue_fang_source_document(AssetPath::new("ai/tasks/test.tsk"), source)
            .expect("valid behavior source");
        Self::from_test_document(
            super::source::lower_behavior_source_document(&document)
                .expect("supported behavior source"),
        )
    }

    #[cfg(any(test, feature = "testing"))]
    pub fn from_test_document(document: BehaviorDocument) -> Self {
        Self {
            behavior_document: document,
            scripts: BTreeMap::new(),
        }
    }

    pub fn script_dependency(
        &self,
        file: &str,
    ) -> Option<(
        &str,
        &Handle<crate::lua_script::lua_script_asset_loading::LuaScriptAsset>,
    )> {
        self.scripts
            .get(&AssetId::from_virtual_path(file))
            .map(|(path, handle)| (path.as_str(), handle))
    }
}

#[derive(Clone, Debug)]
pub(super) struct BehaviorDeclarationLocation {
    pub(super) behavior_document_asset: Handle<BehaviorDocumentAsset>,
    pub(super) declaration_index: usize,
}

#[derive(Resource, Default)]
pub struct LoadedBehaviorDocumentCollection {
    pub(super) archive_revision: Option<u64>,
    pub(super) indexed_archive_revision: Option<u64>,
    pub(super) behavior_document_assets: Vec<Handle<BehaviorDocumentAsset>>,
    pub(super) behavior_sets_by_identifier: BTreeMap<AssetId, Vec<BehaviorDeclarationLocation>>,
    pub(super) behavior_tasks_by_unique_identifier:
        BTreeMap<AssetId, Vec<BehaviorDeclarationLocation>>,
}

#[derive(Clone, Copy)]
pub struct BehaviorDeclarationIndexView<'a> {
    pub(super) behavior_sets_by_identifier: &'a BTreeMap<AssetId, Vec<BehaviorDeclarationLocation>>,
    pub(super) behavior_tasks_by_unique_identifier:
        &'a BTreeMap<AssetId, Vec<BehaviorDeclarationLocation>>,
    pub(super) behavior_document_assets: &'a Assets<BehaviorDocumentAsset>,
}
