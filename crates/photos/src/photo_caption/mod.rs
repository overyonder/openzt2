use std::{
    cell::{Cell, RefCell},
    collections::BTreeSet,
};

use bevy::prelude::*;
use mlua::{Function, Lua};
use openzt2_game_assets::{
    asset_source::ArchiveOverlayChanged,
    localization::{
        loaded_localization_queries::LoadedLocalizationView,
        localization_asset_types::LocalizationAsset,
        localization_precedence_index::LocalizationPrecedenceIndex,
    },
    lua_script::{
        lua_script_asset_loading::LuaScriptAsset,
        lua_script_module_execution::{
            execute_lua_script_module_once_per_virtual_machine,
            install_lua_script_dependency_loading_functions,
        },
        lua_script_sources::LuaScriptSources,
    },
};
use openzt2_game_data::AssetId;
use openzt2_world_state::world_spawn::world_membership_types::{WorldMember, WorldRoot};

use super::{
    photo_capture_types::{CapturedPhotoEvidence, PendingPhotoScore},
    photo_challenge_lua_scoring::create_lua_photo_exposure,
};

#[derive(Component)]
pub struct PhotoCaption(pub String);

#[derive(Resource, Default, PartialEq, Eq)]
pub(super) struct PhotoCaptionSource(Option<(String, Handle<LuaScriptAsset>)>);

#[derive(Component, Default)]
pub(super) struct PhotoCaptionInterpreter {
    lua: Option<Lua>,
    modules: BTreeSet<AssetId>,
}

pub(super) fn default_photo_caption_key() -> AssetId {
    AssetId::from_key("ZTPhotoMode:default_photo_caption")
}

pub(super) fn resolve_authored_photo_caption_script(
    sources: Res<LuaScriptSources>,
    mut source: ResMut<PhotoCaptionSource>,
    mut overlay: MessageReader<ArchiveOverlayChanged>,
) {
    let overlay_changed = overlay.read().count() != 0;
    if source.is_added() || overlay_changed {
        source.set_if_neq(PhotoCaptionSource(sources.load("", "photo/caption.lua")));
    }
}

pub(super) fn generate_captions_once_for_captured_photos(
    sources: Res<LuaScriptSources>,
    source: Res<PhotoCaptionSource>,
    scripts: Res<Assets<LuaScriptAsset>>,
    locale: Res<LocalizationPrecedenceIndex>,
    localizations: Res<Assets<LocalizationAsset>>,
    mut interpreters: Query<&mut PhotoCaptionInterpreter, With<WorldRoot>>,
    photos: Query<
        (Entity, &WorldMember, &CapturedPhotoEvidence),
        (With<PendingPhotoScore>, Without<PhotoCaption>),
    >,
    mut commands: Commands,
) {
    if source.is_changed() || scripts.is_changed() {
        for mut interpreter in &mut interpreters {
            if interpreter.lua.is_some() {
                *interpreter = PhotoCaptionInterpreter::default();
            }
        }
    }
    let localization = locale.borrow_loaded_localization_view(&localizations);
    let fallback = || {
        localization
            .and_then(|localization| {
                localization.find_plain_localized_text(default_photo_caption_key())
            })
            .unwrap_or_default()
            .to_owned()
    };
    for (entity, member, evidence) in &photos {
        let caption = source.0.as_ref().and_then(|(path, handle)| {
            let mut interpreter = interpreters.get_mut(member.root).ok()?;
            if !scripts.contains(handle.id()) {
                return None;
            }
            match interpreter.generate_caption(
                path,
                handle,
                &scripts,
                &sources,
                evidence,
                localization,
            ) {
                Ok(text) => Some(text),
                Err(error) => {
                    warn!(%error, "photo caption script failed");
                    None
                }
            }
        });
        commands
            .entity(entity)
            .insert(PhotoCaption(caption.unwrap_or_else(fallback)));
    }
}

impl PhotoCaptionInterpreter {
    fn generate_caption(
        &mut self,
        path: &str,
        handle: &Handle<LuaScriptAsset>,
        scripts: &Assets<LuaScriptAsset>,
        sources: &LuaScriptSources,
        evidence: &CapturedPhotoEvidence,
        localization: Option<LoadedLocalizationView<'_>>,
    ) -> mlua::Result<String> {
        let lua = self.lua.get_or_insert_with(Lua::new);
        let modules = RefCell::new(std::mem::take(&mut self.modules));
        let active_paths = RefCell::new(Vec::new());
        let cursor = Cell::new(0);
        let result = lua.scope(|scope| {
            install_lua_script_dependency_loading_functions(
                lua,
                scope,
                scripts,
                sources,
                &modules,
                &active_paths,
            )?;
            lua.globals().set(
                "localize",
                scope.create_function(move |lua, key: String| {
                    let text = localization
                        .and_then(|localization| {
                            localization.find_plain_localized_text(AssetId::from_key(&key))
                        })
                        .unwrap_or_default();
                    lua.create_string(text)
                })?,
            )?;
            execute_lua_script_module_once_per_virtual_machine(
                lua,
                path,
                handle,
                scripts,
                &modules,
                &active_paths,
            )?;
            let exposure = create_lua_photo_exposure(lua, scope, evidence, &cursor)?;
            let caption: Function = lua.globals().get("generateCaption")?;
            active_paths.borrow_mut().push(path.to_owned());
            caption.call(exposure)
        });
        self.modules = modules.into_inner();
        result
    }
}
