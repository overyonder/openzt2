//! Profile roots selected from the product options and platform data location.

use super::original_profile_save_discovery::{
    discover_original_saved_game_files, OriginalSavedGameFile,
};
use bevy::prelude::*;
use openzt2_game_assets::{
    asset_source::AssetArchives,
    source_document::{
        blue_fang_source_document_parsing::parse_blue_fang_source_document, path::AssetPath,
    },
};
use std::{
    io,
    path::{Component, Path, PathBuf},
    sync::Arc,
};

#[derive(Resource, Clone)]
pub(super) struct OriginalProfileSaveLocations {
    relative_product_directory: Result<PathBuf, Arc<io::Error>>,
    roaming_directories: Vec<PathBuf>,
    wine_prefixes: Vec<PathBuf>,
}

impl FromWorld for OriginalProfileSaveLocations {
    fn from_world(world: &mut World) -> Self {
        let relative_product_directory = world
            .get_resource::<AssetArchives>()
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    "installation archives are unavailable",
                )
            })
            .and_then(read_product_profile_directory)
            .map_err(Arc::new);
        let mut roaming_directories = Vec::new();
        let mut wine_prefixes = Vec::new();
        if cfg!(target_os = "windows") {
            if let Some(roaming) = dirs::data_dir() {
                roaming_directories.push(roaming);
            }
        } else if let Some(prefix) = std::env::var_os("WINEPREFIX") {
            wine_prefixes.push(PathBuf::from(prefix));
        } else if let Some(home) = dirs::home_dir() {
            wine_prefixes.push(home.join(".wine"));
        }
        Self {
            relative_product_directory,
            roaming_directories,
            wine_prefixes,
        }
    }
}

impl OriginalProfileSaveLocations {
    pub(super) fn discover(&self) -> io::Result<Vec<OriginalSavedGameFile>> {
        let relative = self
            .relative_product_directory
            .as_ref()
            .map_err(|error| io::Error::new(error.kind(), error.to_string()))?;
        if self
            .wine_prefixes
            .iter()
            .any(|prefix| !prefix.is_absolute())
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Wine prefix must be absolute",
            ));
        }
        discover_original_saved_game_files(
            self.roaming_directories.clone(),
            self.wine_prefixes.clone(),
            relative,
        )
    }
}

fn read_product_profile_directory(archives: &AssetArchives) -> io::Result<PathBuf> {
    let bytes = archives.read_source(Path::new("config/user.xml"))?;
    let document = parse_blue_fang_source_document(AssetPath::new("config/user.xml"), &bytes)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    if document.root.name.as_str() != "BFUserProfileMgr" {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "profile options have no profile manager",
        ));
    }
    let mut options = document
        .root
        .element_children()
        .filter(|node| node.name.as_str() == "OptionsInfo");
    let options = options
        .next()
        .filter(|_| options.next().is_none())
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "profile manager must contain one OptionsInfo",
            )
        })?;
    let app_name = options.attribute("AppName").ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "profile options have no AppName",
        )
    })?;
    let extra_path = options.attribute("ExtraPath").unwrap_or("");
    if app_name.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "profile AppName is empty",
        ));
    }
    let mut relative = PathBuf::new();
    for component in [extra_path, app_name] {
        if component.is_empty() {
            continue;
        }
        let path = Path::new(component);
        if path.as_os_str().is_empty()
            || path
                .components()
                .any(|part| !matches!(part, Component::Normal(_)))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid profile product directory",
            ));
        }
        relative.push(path);
    }
    Ok(relative)
}
