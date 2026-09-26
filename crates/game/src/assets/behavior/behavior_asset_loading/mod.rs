//! Bevy loading of behavior documents.

use std::io;

use super::{behavior_asset_types::BehaviorDocumentAsset, source};
use crate::assets::source_document::{
    blue_fang_source_document_parsing::parse_blue_fang_source_document, path::AssetPath,
};
use bevy::{
    asset::{io::Reader, AssetLoader, LoadContext},
    prelude::*,
};

#[derive(TypePath)]
pub(super) struct BehaviorDocumentAssetLoader {
    asset_archives: crate::asset_source::AssetArchives,
}

impl FromWorld for BehaviorDocumentAssetLoader {
    fn from_world(world: &mut World) -> Self {
        Self {
            asset_archives: world
                .resource::<crate::asset_source::AssetArchives>()
                .clone(),
        }
    }
}

impl AssetLoader for BehaviorDocumentAssetLoader {
    type Asset = BehaviorDocumentAsset;
    type Settings = ();
    type Error = io::Error;

    async fn load(
        &self,
        source_reader: &mut dyn Reader,
        _: &(),
        load_context: &mut LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let _performance_timer = self
            .asset_archives
            .measure_scene_loading_asset_translation("behavior");
        let mut source_bytes = Vec::new();
        source_reader.read_to_end(&mut source_bytes).await?;
        let behavior_asset_path = load_context
            .path()
            .path()
            .to_string_lossy()
            .replace('\\', "/");
        let source_document =
            parse_blue_fang_source_document(AssetPath::new(&behavior_asset_path), &source_bytes)
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        let behavior_document = source::lower_behavior_source_document(&source_document)?;
        let mut scripts = std::collections::BTreeMap::new();
        let mut register_scripts = |actions: &[openzt2_game_data::behavior::action_record::BehaviorAction]| -> io::Result<()> {
            for action in actions {
                if let openzt2_game_data::behavior::action_record::BehaviorAction::Script { file, .. } = action {
                    let key = openzt2_game_data::AssetId::from_virtual_path(file);
                    if scripts.contains_key(&key) { continue; }
                    let path = self.asset_archives.resolve_script_reference(std::path::Path::new(&behavior_asset_path), file)
                        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, format!("behavior script {file} is missing")))?
                        .to_string_lossy().replace('\\', "/");
                    let handle = load_context.load(path.clone());
                    scripts.insert(key, (path, handle));
                }
            }
            Ok(())
        };
        match &behavior_document {
            openzt2_game_data::behavior::document::BehaviorDocument::Sets(sets) => {
                for set in sets {
                    register_scripts(set.actions.lowered_actions())?;
                }
            }
            openzt2_game_data::behavior::document::BehaviorDocument::Tasks(tasks) => {
                for task in tasks {
                    for phase in [&task.execution, &task.completion, &task.failure] {
                        register_scripts(phase.lowered_actions())?;
                    }
                }
            }
        }
        Ok(BehaviorDocumentAsset {
            behavior_document,
            scripts,
        })
    }

    fn extensions(&self) -> &[&str] {
        &["beh", "tsk"]
    }
}
