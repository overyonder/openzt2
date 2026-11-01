//! In-memory sources reconstructed from the selected save container.

use crate::world_scenario::{
    world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset,
    world_scenario_document_source::WorldScenarioDocumentSource,
    world_scenario_document_source_encoding::{
        encode_world_scenario_document_source, WORLD_SCENARIO_DOCUMENT_SOURCE_HEADER_BYTE_COUNT,
    },
};
use bevy::{
    asset::io::{memory::MemoryAssetReader, AssetSourceBuilder},
    prelude::*,
};
use openzt2_game_data::{terrain::TerrainGrid, AssetId};
use std::{io, path::Path};

#[derive(Resource)]
pub struct SavedWorldAssetSource {
    reader: MemoryAssetReader,
    maximum_byte_count: usize,
    current: Option<(String, Handle<WorldScenarioDocumentAsset>)>,
}

impl SavedWorldAssetSource {
    pub fn new(maximum_byte_count: usize) -> Self {
        Self {
            reader: MemoryAssetReader::default(),
            maximum_byte_count,
            current: None,
        }
    }

    pub fn asset_source(&self) -> AssetSourceBuilder {
        let reader = self.reader.clone();
        AssetSourceBuilder::new(move || Box::new(reader.clone()))
    }

    pub fn register(
        &mut self,
        mut source: WorldScenarioDocumentSource,
        terrain_identifier: AssetId,
        terrain: &TerrainGrid,
        server: &AssetServer,
    ) -> io::Result<Handle<WorldScenarioDocumentAsset>> {
        let mut matches = source
            .terrains
            .iter_mut()
            .filter(|path| path.id == terrain_identifier);
        let dependency = matches.next().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "saved world has no selected terrain dependency",
            )
        })?;
        dependency.path = "./terrain.ozterrain".into();
        if matches.next().is_some() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "duplicate selected terrain dependency",
            ));
        }
        let document_size =
            postcard::experimental::serialized_size(&source).map_err(io::Error::other)?;
        let terrain_size =
            postcard::experimental::serialized_size(terrain).map_err(io::Error::other)?;
        if document_size
            .checked_add(WORLD_SCENARIO_DOCUMENT_SOURCE_HEADER_BYTE_COUNT)
            .and_then(|size| size.checked_add(terrain_size))
            .is_none_or(|size| size > self.maximum_byte_count)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "saved world source exceeds its byte budget",
            ));
        }
        let document = encode_world_scenario_document_source(&source)?;
        let terrain = postcard::to_allocvec(terrain).map_err(io::Error::other)?;
        if document
            .len()
            .checked_add(terrain.len())
            .is_none_or(|size| size > self.maximum_byte_count)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "saved world source exceeds its byte budget",
            ));
        }
        let mut hash = blake3::Hasher::new();
        hash.update(&(document.len() as u64).to_le_bytes());
        hash.update(&document);
        hash.update(&terrain);
        let key = hash.finalize().to_hex().to_string();
        if let Some((current, handle)) = &self.current {
            if current == &key {
                return Ok(handle.clone());
            }
        }
        self.reader
            .root
            .insert_asset(Path::new(&format!("{key}/world.ozworld")), document);
        self.reader
            .root
            .insert_asset(Path::new(&format!("{key}/terrain.ozterrain")), terrain);
        let handle = server
            .load::<WorldScenarioDocumentAsset>(format!("saved-worlds://{key}/world.ozworld"));
        if let Some((previous, _)) = self.current.replace((key, handle.clone())) {
            drop(
                self.reader
                    .root
                    .remove_asset(Path::new(&format!("{previous}/world.ozworld"))),
            );
            drop(
                self.reader
                    .root
                    .remove_asset(Path::new(&format!("{previous}/terrain.ozterrain"))),
            );
        }
        Ok(handle)
    }
}
