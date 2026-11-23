use std::{io, path::Path};

use bevy::prelude::*;

use crate::asset_source::AssetArchives;

use super::lua_script_asset_loading::LuaScriptAsset;

#[derive(Resource)]
pub struct LuaScriptSources {
    archives: AssetArchives,
    server: AssetServer,
}

impl FromWorld for LuaScriptSources {
    fn from_world(world: &mut World) -> Self {
        Self {
            archives: world.resource::<AssetArchives>().clone(),
            server: world.resource::<AssetServer>().clone(),
        }
    }
}

impl LuaScriptSources {
    pub fn load(
        &self,
        referring_path: &str,
        reference: &str,
    ) -> Option<(String, Handle<LuaScriptAsset>)> {
        let path = self.resolve(referring_path, reference)?;
        let handle = self.server.load(path.clone());
        Some((path, handle))
    }

    pub(super) fn resolve(&self, referring_path: &str, reference: &str) -> Option<String> {
        let path = self
            .archives
            .resolve_script_reference(Path::new(referring_path), reference)?;
        Some(path.to_string_lossy().replace('\\', "/"))
    }

    pub(super) fn read_script(
        &self,
        assets: &Assets<LuaScriptAsset>,
        path: &str,
    ) -> io::Result<LuaScriptAsset> {
        if let Some(asset) = self
            .server
            .get_handle::<LuaScriptAsset>(path.to_owned())
            .and_then(|handle| assets.get(&handle))
        {
            return Ok(asset.clone());
        }
        // Lua file calls can select the path from runtime values. Compile the
        // resolved source before returning to the caller in the same VM call.
        LuaScriptAsset::from_source_bytes(
            Path::new(path),
            self.archives.read_source(Path::new(path))?,
        )
    }
}
