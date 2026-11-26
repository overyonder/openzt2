use openzt2_game_data::scene_prefab::ScenePrefabRenderableVisibilityFlags;
use openzt2_game_data::AssetId;

use openzt2_game_assets::scene_prefab::ScenePrefabAsset;

use openzt2_world_state::world_spawn::prefab_presentation_types::PrefabMaterialOverride;
use openzt2_world_state::world_spawn::prefab_presentation_types::PrefabMaterialOverrides;
use openzt2_world_state::world_spawn::prefab_presentation_types::PrefabModel;
use openzt2_world_state::world_spawn::prefab_presentation_types::PrefabRenderable;
use openzt2_world_state::world_spawn::prefab_presentation_types::PrefabShadowPolicy;

pub(super) fn create_prefab_renderable_components(
    prefab: &ScenePrefabAsset,
    renderable: &openzt2_game_data::scene_prefab::PrefabRenderable,
) -> (
    PrefabRenderable,
    PrefabModel,
    PrefabMaterialOverrides,
    PrefabShadowPolicy,
) {
    let model = AssetId(renderable.model.0);
    let model_path = prefab
        .model_asset_path(model)
        .expect("validated prefab model dependency has a source path")
        .into();
    let overrides = renderable
        .material_overrides
        .iter()
        .map(|row| {
            let material = AssetId(row.material.0);
            PrefabMaterialOverride {
                material,
                material_path: prefab
                    .material_asset_path(material)
                    .expect("validated material override has a source path")
                    .into(),
                handle: None,
            }
        })
        .collect::<Vec<_>>()
        .into_boxed_slice();
    (
        PrefabRenderable,
        PrefabModel {
            model,
            model_path,
            handle: None,
            scene_name: (!renderable.scene_name.is_empty())
                .then(|| renderable.scene_name.clone().into_boxed_str()),
            material_name: (!renderable.material_name.is_empty()).then(|| {
                renderable
                    .material_name
                    .to_ascii_lowercase()
                    .into_boxed_str()
            }),
        },
        PrefabMaterialOverrides(overrides),
        PrefabShadowPolicy {
            casts: renderable.visibility.0 & ScenePrefabRenderableVisibilityFlags::CAST_SHADOW.0
                != 0,
            receives: renderable.visibility.0
                & ScenePrefabRenderableVisibilityFlags::RECEIVE_SHADOW.0
                != 0,
            visible_in_reflections: renderable.visibility.0
                & ScenePrefabRenderableVisibilityFlags::REFLECTION_VISIBLE.0
                != 0,
        },
    )
}

#[derive(bevy::prelude::Component)]
pub(crate) struct PrefabAnimationBinding {
    pub(crate) clock: openzt2_world_state::animation_playback::animation_playback_controller_types::AnimationPlaybackClock,
    pub(crate) start: Option<openzt2_game_data::scene_prefab::PrefabAnimationStart>,
}

pub(super) fn attach_prefab_renderable_animations(
    entity: &mut bevy::ecs::system::EntityCommands<'_>,
    prefab: &openzt2_game_assets::scene_prefab::ScenePrefabAsset,
    row: usize,
    renderable: &openzt2_game_data::scene_prefab::PrefabRenderable,
    clock: openzt2_world_state::animation_playback::animation_playback_controller_types::AnimationPlaybackClock,
) {
    use openzt2_world_state::animation_graph::model_animation_asset_binding_types::PendingModelAnimationAssets;

    let animations = prefab
        .loaded_animation_set_asset_handles()
        .cloned()
        .collect::<Box<[_]>>();
    if animations.is_empty() {
        return;
    }
    let start = prefab
        .authored_animation_start_for_renderable(row, renderable)
        .cloned();
    entity.insert((
        PendingModelAnimationAssets {
            pending_model_animation_set_asset_handles: animations,
            requested_initial_animation_clip_asset_key: start
                .as_ref()
                .map(|start| start.clip_key.clone()),
        },
        PrefabAnimationBinding { clock, start },
    ));
}
