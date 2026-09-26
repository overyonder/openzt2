use bevy::{platform::collections::HashMap, prelude::*};

use crate::assets::animation::animation_set_asset_types::AnimationSetAsset;

#[derive(Resource, Default)]
pub(super) struct StandardBevyAnimationGraphsByAnimationSetAsset {
    pub(super) animation_graphs_by_animation_set_asset: HashMap<
        bevy::asset::AssetId<AnimationSetAsset>,
        StandardBevyAnimationGraphAndClipNodeIndexes,
    >,
}

pub(super) struct StandardBevyAnimationGraphAndClipNodeIndexes {
    pub(super) animation_graph_handle: Handle<AnimationGraph>,
    pub(super) clip_node_indexes: Vec<AnimationNodeIndex>,
}

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub(super) struct StandardBevyAnimationPlayerBinding {
    pub(super) animation_playback_controller_entity: Entity,
    pub(super) animation_set_asset_id: bevy::asset::AssetId<AnimationSetAsset>,
    pub(super) animation_clip_asset_key: String,
}
