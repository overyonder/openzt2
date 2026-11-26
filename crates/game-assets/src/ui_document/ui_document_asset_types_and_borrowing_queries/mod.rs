//! One UI document and its sorted typed dependency-handle indexes.

use bevy::{audio::AudioSource, prelude::*};
use openzt2_game_data::{
    ui_document::{document::UiDocument, node::MoneyTextFormat, widget::UiWidgetRecord},
    AssetId,
};

use crate::{
    lua_script::lua_script_asset_loading::LuaScriptAsset, scene_prefab::ScenePrefabAsset,
    texture::interactive_texture_metadata_asset_and_borrowing_queries::InteractiveTextureMetadataAsset,
};

#[derive(Asset, TypePath, Clone, Debug)]
pub struct UiDocumentAsset {
    pub(in crate::ui_document) canonical_ui_document: UiDocument,
    pub(in crate::ui_document) texture_image_handles: Box<[(AssetId, Handle<Image>)]>,
    pub(in crate::ui_document) interactive_texture_metadata_handles:
        Box<[(AssetId, Handle<InteractiveTextureMetadataAsset>)]>,
    pub(in crate::ui_document) scene_prefab_handles: Box<[(AssetId, Handle<ScenePrefabAsset>)]>,
    pub(in crate::ui_document) nested_ui_document_handles:
        Box<[(AssetId, Handle<UiDocumentAsset>)]>,
    pub(in crate::ui_document) audio_source_handles: Box<[(AssetId, Handle<AudioSource>)]>,
    pub(in crate::ui_document) fame_stars: Option<FameStarDisplay>,
    pub(in crate::ui_document) script_handles: Box<[(AssetId, Handle<LuaScriptAsset>)]>,
}

#[derive(Clone, Debug)]
pub struct FameStarDisplay {
    pub wells_node_index: u32,
    pub well_transforms: Vec<Transform>,
    pub viewport_transform: Transform,
    pub viewport_size_m: Vec2,
    pub animation_delay_seconds: f32,
    pub full: FameStarModel,
    pub half: FameStarModel,
    pub appear: FameStarModel,
    pub disappear: FameStarModel,
}

#[derive(Clone, Debug)]
pub struct FameStarModel {
    pub model: Handle<ScenePrefabAsset>,
    pub scale: f32,
    pub duration_seconds: f32,
    pub auto_start: bool,
    pub looping: bool,
    pub effects: Vec<FameStarEffect>,
}

#[derive(Clone, Debug)]
pub struct FameStarEffect {
    pub prefab: Handle<ScenePrefabAsset>,
    pub duration_seconds: Option<f32>,
}

pub struct GraphLabelText {
    pub format: AssetId,
    pub color: [u8; 4],
    pub money: Option<MoneyTextFormat>,
}

impl UiDocumentAsset {
    pub fn fame_star_display(&self) -> Option<&FameStarDisplay> {
        self.fame_stars.as_ref()
    }

    pub fn edge_pan_surface_node_index(&self) -> Option<u32> {
        self.canonical_ui_document.edge_pan_surface_node_index
    }

    pub fn graph_label_text_node(&self, index: u32) -> Option<GraphLabelText> {
        let node = self.canonical_ui_document.nodes.get(index as usize)?;
        if !matches!(node.widget, UiWidgetRecord::Text(_)) || node.text_format == AssetId::default()
        {
            return None;
        }
        let color = node
            .visuals
            .iter()
            .find_map(|visual| {
                (matches!(
                    visual.visual_state,
                    openzt2_game_data::ui_document::node_presentation::UiNodeVisualState::Default
                ) && visual.affects_text_color)
                    .then_some(visual.text_color)
            })
            .unwrap_or(node.style.color);
        Some(GraphLabelText {
            format: node.text_format,
            color: color.map(|channel| (channel.clamp(0.0, 1.0) * 255.0).round() as u8),
            money: node.money_text_format,
        })
    }

    pub fn canonical_ui_document(&self) -> &UiDocument {
        &self.canonical_ui_document
    }

    pub fn texture_image_handle(&self, asset_id: AssetId) -> Option<&Handle<Image>> {
        find_typed_dependency_handle(&self.texture_image_handles, asset_id)
    }

    pub fn cloned_texture_image_handle(&self, asset_id: AssetId) -> Option<Handle<Image>> {
        self.texture_image_handle(asset_id).cloned()
    }

    pub fn cloned_interactive_texture_metadata_handle(
        &self,
        asset_id: AssetId,
    ) -> Option<Handle<InteractiveTextureMetadataAsset>> {
        find_typed_dependency_handle(&self.interactive_texture_metadata_handles, asset_id).cloned()
    }

    pub fn scene_prefab_handle(&self, asset_id: AssetId) -> Option<&Handle<ScenePrefabAsset>> {
        find_typed_dependency_handle(&self.scene_prefab_handles, asset_id)
    }

    pub fn nested_ui_document_handle(&self, asset_id: AssetId) -> Option<&Handle<UiDocumentAsset>> {
        find_typed_dependency_handle(&self.nested_ui_document_handles, asset_id)
    }

    pub fn audio_source_handle(&self, asset_id: AssetId) -> Option<&Handle<AudioSource>> {
        find_typed_dependency_handle(&self.audio_source_handles, asset_id)
    }

    pub fn script_handle(&self, asset_id: AssetId) -> Option<&Handle<LuaScriptAsset>> {
        find_typed_dependency_handle(&self.script_handles, asset_id)
    }
}

fn find_typed_dependency_handle<T: Asset>(
    typed_dependency_handles: &[(AssetId, Handle<T>)],
    asset_id: AssetId,
) -> Option<&Handle<T>> {
    typed_dependency_handles
        .binary_search_by_key(&asset_id.0, |(candidate, _)| candidate.0)
        .ok()
        .map(|index| &typed_dependency_handles[index].1)
}
