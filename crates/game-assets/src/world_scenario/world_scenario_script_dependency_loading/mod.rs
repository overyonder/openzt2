use super::world_scenario_source_reference_resolution::unresolved_reference;
use crate::{asset_source::AssetArchives, lua_script::lua_script_asset_loading::LuaScriptAsset};
use bevy::{asset::LoadContext, prelude::*};
use openzt2_game_data::world_scenario::{
    PhotoChallengeScriptBinding, ScenarioScriptBinding, WorldScenarioDocument,
};
use std::{io, path::Path};

pub(super) fn resolve_scenario_script_path(
    archives: &AssetArchives,
    declaring_path: &str,
    reference: &str,
) -> io::Result<String> {
    archives
        .resolve_script_reference(Path::new(declaring_path), reference)
        .map(|path| path.to_string_lossy().replace('\\', "/"))
        .ok_or_else(|| unresolved_reference(declaring_path, "scenario script", reference))
}

pub(super) fn load_scenario_script_dependencies(
    document: &WorldScenarioDocument,
    archives: &AssetArchives,
    path: &str,
    context: &mut LoadContext<'_>,
) -> io::Result<Box<[(ScenarioScriptBinding, String, Handle<LuaScriptAsset>)]>> {
    document
        .scenario_script_bindings
        .iter()
        .map(|binding| {
            let resolved = resolve_scenario_script_path(archives, path, &binding.script)?;
            Ok((binding.clone(), resolved.clone(), context.load(resolved)))
        })
        .collect()
}

pub(super) fn load_photo_challenge_script_dependencies(
    document: &WorldScenarioDocument,
    archives: &AssetArchives,
    path: &str,
    context: &mut LoadContext<'_>,
) -> Box<[(PhotoChallengeScriptBinding, String, Handle<LuaScriptAsset>)]> {
    // Optional catalogue entries may refer to scripts absent from the installation.
    document
        .photo_challenge_script_bindings
        .iter()
        .filter_map(|binding| {
            let resolved = archives.resolve_script_reference(Path::new(path), &binding.script)?;
            let resolved = resolved.to_string_lossy().replace('\\', "/");
            Some((binding.clone(), resolved.clone(), context.load(resolved)))
        })
        .collect()
}
