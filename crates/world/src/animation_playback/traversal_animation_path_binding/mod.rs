use bevy::prelude::*;
use openzt2_game_assets::animation::{
    animation_clip_source_queries::AnimationClipSources,
    animation_set_asset_types::{AnimationClipSourceAsset, AnimationSetAsset},
};
use openzt2_world_state::animation_playback::traversal_animation_path_types::{
    PrefabTraversalAnimationManifests, TraversalAnimationPath,
};

#[derive(Component)]
pub(super) struct PendingTraversalClips(Vec<(String, Handle<AnimationClipSourceAsset>)>);

pub(super) fn bind_authored_traversal_animation_paths(
    mut commands: Commands,
    sets: Res<Assets<AnimationSetAsset>>,
    server: Res<AssetServer>,
    children: Query<&Children>,
    paths: Query<(), With<TraversalAnimationPath>>,
    mut removed: RemovedComponents<PrefabTraversalAnimationManifests>,
    mut ready: MessageReader<AssetEvent<AnimationSetAsset>>,
    mut failed: MessageReader<bevy::asset::AssetLoadFailedEvent<AnimationSetAsset>>,

    sources: Res<Assets<AnimationClipSourceAsset>>,
    clip_loading: AnimationClipSources,
    binders: Query<(
        Entity,
        Ref<PrefabTraversalAnimationManifests>,
        Option<&PendingTraversalClips>,
    )>,
) {
    let changed_sets = ready
        .read()
        .filter_map(|event| match *event {
            AssetEvent::Added { id } | AssetEvent::Modified { id } => Some(id),
            _ => None,
        })
        .chain(failed.read().map(|event| event.id))
        .collect::<bevy::platform::collections::HashSet<_>>();
    for binder in removed.read() {
        if !binders.contains(binder) {
            for child in children
                .get(binder)
                .into_iter()
                .flat_map(|children| children.iter())
            {
                if paths.contains(child) {
                    commands.entity(child).try_despawn();
                }
            }
            commands
                .entity(binder)
                .try_remove::<PendingTraversalClips>();
        }
    }
    for (binder, manifests, pending) in &binders {
        let needs_rebind = manifests.is_changed()
            || manifests
                .0
                .iter()
                .any(|handle| changed_sets.contains(&handle.id()));
        if needs_rebind {
            for child in children
                .get(binder)
                .into_iter()
                .flat_map(|children| children.iter())
            {
                if paths.contains(child) {
                    commands.entity(child).try_despawn();
                }
            }
        }
        let waiting_for_manifest = manifests.0.iter().any(|manifest| {
            !sets.contains(manifest.id())
                && !matches!(
                    server.load_state(manifest.id()),
                    bevy::asset::LoadState::Failed(_)
                )
        });
        if waiting_for_manifest {
            if needs_rebind && pending.is_some() {
                commands
                    .entity(binder)
                    .try_remove::<PendingTraversalClips>();
            }
            continue;
        }
        if needs_rebind {
            let manifests = manifests.0.iter().filter_map(|manifest| sets.get(manifest));
            let clips = manifests
                .flat_map(|manifest| {
                    manifest
                        .animation_clip_source_references
                        .iter()
                        .filter_map(|reference| {
                            let key = &reference.animation_clip_asset_key;
                            Some((key.clone(), clip_loading.request(manifest, key)?))
                        })
                })
                .collect::<Vec<_>>();
            commands
                .entity(binder)
                .try_insert(PendingTraversalClips(clips));
            continue;
        }
        let Some(pending) = pending else {
            continue;
        };
        if pending
            .0
            .iter()
            .any(|(_, source)| !sources.contains(source.id()) && !clip_loading.failed(source))
        {
            continue;
        }
        for (key, source) in &pending.0 {
            let Some(clip) = sources.get(source) else {
                continue;
            };
            if clip
                .authored_animation_clip_metadata
                .root_motion_joint
                .is_some()
            {
                commands.spawn((
                    Name::new(key.clone()),
                    Transform::IDENTITY,
                    ChildOf(binder),
                    TraversalAnimationPath {
                        clip_key: key.clone(),
                        source: source.clone(),
                    },
                ));
            }
        }
        commands
            .entity(binder)
            .try_remove::<PendingTraversalClips>();
    }
}
