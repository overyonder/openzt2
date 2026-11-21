use bevy::{asset::AssetLoadFailedEvent, platform::collections::HashSet, prelude::*};
use openzt2_world_state::animation_playback::animation_playback_controller_types::AnimationPlaybackController;

use openzt2_game_assets::animation::animation_set_asset_types::AnimationSetAsset;
use openzt2_game_assets::model::native_model_asset_loading::EMBEDDED_ANIMATION_SET_LABEL;
use openzt2_world_state::animation_graph::animation_graph_playback_state_types::AnimationGraphPlaybackState;
use openzt2_world_state::animation_graph::model_animation_asset_binding_types::{
    ModelAnimationAssetLibrary, ModelAnimationAttachmentResolved, PendingModelAnimationAssets,
};
use openzt2_world_state::model_render::ModelExpanded;
use openzt2_world_state::world_spawn::prefab_presentation_types::PrefabModel;

#[derive(Component)]
pub(super) struct EmbeddedModelAnimationSet(Handle<AnimationSetAsset>);

pub(super) fn request_embedded_model_animation_sets(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    models: Query<
        (Entity, &PrefabModel),
        (
            With<ModelExpanded>,
            Without<AnimationPlaybackController>,
            Without<PendingModelAnimationAssets>,
            Without<ModelAnimationAttachmentResolved>,
            Without<EmbeddedModelAnimationSet>,
        ),
    >,
) {
    for (entity, model) in &models {
        let Some(model) = &model.handle else {
            continue;
        };
        let Some(path) = model.path() else {
            commands
                .entity(entity)
                .try_insert(ModelAnimationAttachmentResolved);
            continue;
        };
        let handle = asset_server.load(path.clone_owned().with_label(EMBEDDED_ANIMATION_SET_LABEL));
        commands.entity(entity).try_insert((
            EmbeddedModelAnimationSet(handle.clone()),
            PendingModelAnimationAssets {
                pending_model_animation_set_asset_handles: Box::new([handle]),
                requested_initial_animation_clip_asset_key: None,
            },
        ));
    }
}

pub(super) fn refresh_embedded_model_animation_bindings(
    mut commands: Commands,
    mut events: MessageReader<AssetEvent<AnimationSetAsset>>,
    mut failures: MessageReader<AssetLoadFailedEvent<AnimationSetAsset>>,
    sets: Res<Assets<AnimationSetAsset>>,
    mut models: Query<(
        Entity,
        &EmbeddedModelAnimationSet,
        Option<&mut AnimationPlaybackController>,
        Option<&PendingModelAnimationAssets>,
        Has<ModelAnimationAttachmentResolved>,
    )>,
) {
    let modified = events
        .read()
        .filter_map(|event| match *event {
            AssetEvent::Modified { id } => Some(id),
            _ => None,
        })
        .collect::<HashSet<_>>();
    let failed = failures
        .read()
        .map(|failure| failure.id)
        .collect::<HashSet<_>>();
    if modified.is_empty() && failed.is_empty() {
        return;
    }
    for (entity, embedded, controller, pending, resolved) in &mut models {
        let id = embedded.0.id();
        if failed.contains(&id)
            && pending.is_some_and(|pending| {
                pending
                    .pending_model_animation_set_asset_handles
                    .contains(&embedded.0)
            })
        {
            commands
                .entity(entity)
                .try_remove::<PendingModelAnimationAssets>()
                .try_insert(ModelAnimationAttachmentResolved);
            continue;
        }
        if !modified.contains(&id) || pending.is_some() {
            continue;
        }
        let Some(set) = sets.get(id) else {
            continue;
        };
        if let Some(mut controller) = controller {
            if controller.animation_set_asset.id() != id
                || set
                    .embedded_clip(&controller.animation_clip_asset_key)
                    .is_some()
            {
                continue;
            }
            if let Some(clip) = set.animation_clips.first() {
                controller
                    .animation_clip_asset_key
                    .clone_from(&clip.animation_clip_asset_key);
                controller.playback_generation = controller.playback_generation.wrapping_add(1);
                controller.explicit_clip_request_id = None;
                controller.preserve_seek_on_restart = false;
            } else {
                commands.entity(entity).try_remove::<(
                    AnimationPlaybackController,
                    AnimationGraphPlaybackState,
                    ModelAnimationAssetLibrary,
                )>();
            }
        } else if resolved && !set.animation_clips.is_empty() {
            commands
                .entity(entity)
                .try_remove::<ModelAnimationAttachmentResolved>()
                .try_insert(PendingModelAnimationAssets {
                    pending_model_animation_set_asset_handles: Box::new([embedded.0.clone()]),
                    requested_initial_animation_clip_asset_key: None,
                });
        }
    }
}
