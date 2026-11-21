use bevy::prelude::*;
use openzt2_game_assets::animation::animation_clip_source_queries::AnimationClipSources;
use openzt2_game_assets::animation::animation_set_asset_types::AnimationClipSourceAsset;
use openzt2_game_assets::animation::animation_set_asset_types::AnimationSetAsset;
use openzt2_game_assets::source_document::path::AssetPath;
use openzt2_world_state::animation_graph::animation_graph_playback_state_types::AnimationGraphPlaybackState;
use openzt2_world_state::animation_graph::animation_presentation_relationship_types::AnimationPresentationOwner;
use openzt2_world_state::animation_graph::model_animation_asset_attachment::resolve_animation_clip_or_graph_node_asset_key;
use openzt2_world_state::animation_graph::model_animation_asset_attachment::select_initial_animation_clip_asset_key_from_graph_node;
use openzt2_world_state::animation_graph::model_animation_asset_binding_types::ModelAnimationAssetLibrary;
use openzt2_world_state::animation_graph::model_animation_asset_binding_types::ModelAnimationAttachmentResolved;
use openzt2_world_state::animation_graph::model_animation_asset_binding_types::PendingModelAnimationAssets;
use openzt2_world_state::animation_playback::animation_playback_controller_types::AnimationPlaybackClock;
use openzt2_world_state::animation_playback::animation_playback_controller_types::AnimationPlaybackController;
use openzt2_world_state::animation_playback::animation_playback_controller_types::AnimationPlaybackRepetitionPolicy;
use openzt2_world_state::model_render::ModelExpanded;
use openzt2_world_state::world_spawn::prefab_presentation_types::PrefabModel;
use openzt2_zoo_state::animal_lifecycle::types::SelectedAnimalPresentationModel;

#[derive(Component)]
pub(super) struct InitialAnimationSource(Handle<AnimationClipSourceAsset>);

pub(super) fn attach_loaded_animation_set_assets_to_animation_subject_entities(
    mut commands: Commands,
    animation_set_assets: Res<Assets<AnimationSetAsset>>,
    clip_sources: AnimationClipSources,
    presentation_owners: Query<&AnimationPresentationOwner>,
    parents: Query<&ChildOf>,
    pending_animation_subjects: Query<
        (
            Entity,
            Option<&PrefabModel>,
            Option<&SelectedAnimalPresentationModel>,
            Option<&ChildOf>,
            &PendingModelAnimationAssets,
            Option<&InitialAnimationSource>,
            Has<ModelExpanded>,
        ),
        (
            Without<AnimationPlaybackController>,
            Without<ModelAnimationAttachmentResolved>,
        ),
    >,
) {
    // Model owners can retire while loaded asset results await deferred application.
    for (
        animation_subject_entity,
        prefab_model,
        animal_model,
        animation_subject_parent,
        pending_model_animation_assets,
        initial_source,
        expanded,
    ) in &pending_animation_subjects
    {
        if prefab_model.is_some() && !expanded {
            continue;
        }
        let pending_model_animation_set_asset_handles = pending_model_animation_assets
            .pending_model_animation_set_asset_handles
            .clone();
        let Some(_) = pending_model_animation_set_asset_handles.first() else {
            commands
                .entity(animation_subject_entity)
                .try_insert(ModelAnimationAttachmentResolved)
                .try_remove::<PendingModelAnimationAssets>();
            continue;
        };
        assert_eq!(
            pending_model_animation_set_asset_handles.len(),
            1,
            "a prefab model must reference exactly one model-specific animation set"
        );
        let initial_animation_set_asset_handle =
            pending_model_animation_set_asset_handles[0].clone();
        let loaded_model_animation_set_asset_handles =
            vec![initial_animation_set_asset_handle.clone()].into_boxed_slice();
        let Some(initial_animation_set_asset) =
            animation_set_assets.get(&initial_animation_set_asset_handle)
        else {
            continue;
        };
        let subject_model = prefab_model
            .map(|model| model.model)
            .or_else(|| animal_model.map(|model| model.model));
        assert!(
            initial_animation_set_asset.model_asset_path.is_empty()
                || subject_model.is_none_or(|model| openzt2_game_data::AssetId::from_virtual_path(
                    &AssetPath::new(&initial_animation_set_asset.model_asset_path).key(),
                ) == model)
                // Archive paths are case-insensitive: manifests store them
                // lowercased while prefabs keep the authored casing.
                || prefab_model.is_some_and(|model| initial_animation_set_asset
                    .model_asset_path
                    .eq_ignore_ascii_case(&model.model_path)),
            "model-specific animation set {:?} does not belong to its prefab model {:?}",
            initial_animation_set_asset.model_asset_path,
            prefab_model.map(|model| &model.model_path),
        );
        let initial_animation_clip_asset_key = match pending_model_animation_assets
            .requested_initial_animation_clip_asset_key
            .clone()
            .and_then(|key| {
                resolve_animation_clip_or_graph_node_asset_key(initial_animation_set_asset, &key)
                    .or_else(|| {
                        initial_animation_set_asset
                            .animation_clips
                            .iter()
                            .map(|clip| &clip.animation_clip_asset_key)
                            .chain(
                                initial_animation_set_asset
                                    .animation_clip_source_references
                                    .iter()
                                    .map(|reference| &reference.animation_clip_asset_key),
                            )
                            .find(|clip| clip.eq_ignore_ascii_case(&key))
                            .cloned()
                    })
            })
            .map_or_else(
                || select_initial_animation_clip_asset_key(initial_animation_set_asset),
                Some,
            ) {
            Some(animation_clip_asset_key) => animation_clip_asset_key,
            None => {
                commands
                    .entity(animation_subject_entity)
                    .try_insert(ModelAnimationAttachmentResolved)
                    .try_remove::<PendingModelAnimationAssets>();
                continue;
            }
        };
        if clip_sources
            .get(
                initial_animation_set_asset,
                &initial_animation_clip_asset_key,
            )
            .is_none()
        {
            let source = initial_source.map(|source| source.0.clone()).or_else(|| {
                clip_sources.request(
                    initial_animation_set_asset,
                    &initial_animation_clip_asset_key,
                )
            });
            match source {
                Some(source) if !clip_sources.failed(&source) => {
                    if initial_source.is_none() {
                        commands
                            .entity(animation_subject_entity)
                            .try_insert(InitialAnimationSource(source));
                    }
                }
                _ => {
                    commands
                        .entity(animation_subject_entity)
                        .try_insert(ModelAnimationAttachmentResolved)
                        .try_remove::<(PendingModelAnimationAssets, InitialAnimationSource)>();
                }
            }
            continue;
        }
        let initial_animation_graph_playback_state = {
            let mut state = AnimationGraphPlaybackState::default();
            if let Some(index) = initial_animation_set_asset
                .animation_graph_nodes
                .iter()
                .position(|node| {
                    node.animation_clip_asset_keys
                        .contains(&initial_animation_clip_asset_key)
                })
                .and_then(|index| u32::try_from(index).ok())
            {
                state.current_animation_graph_node_index = index;
            }
            state
        };
        let mut animation_subject_commands = commands.entity(animation_subject_entity);
        animation_subject_commands.try_insert((
            ModelAnimationAttachmentResolved,
            AnimationPresentationOwner {
                gameplay_entity: presentation_owners
                    .get(animation_subject_entity)
                    .map_or_else(
                        |_| {
                            animation_subject_parent.map_or(animation_subject_entity, |parent| {
                                std::iter::successors(Some(parent.parent()), |entity| {
                                    parents.get(*entity).ok().map(ChildOf::parent)
                                })
                                .find_map(|entity| presentation_owners.get(entity).ok())
                                .map_or(parent.parent(), |owner| owner.gameplay_entity)
                            })
                        },
                        |owner| owner.gameplay_entity,
                    ),
            },
            ModelAnimationAssetLibrary {
                loaded_model_animation_set_asset_handles,
            },
            AnimationPlaybackController {
                playback_generation: 0,
                explicit_clip_request_id: None,
                animation_set_asset: initial_animation_set_asset_handle,
                animation_clip_asset_key: initial_animation_clip_asset_key.clone(),
                preserve_seek_on_restart: false,
                playback_speed_permille: 1000,

                playback_repetition_policy:
                    AnimationPlaybackRepetitionPolicy::UseAuthoredClipPolicy,
                playback_clock: AnimationPlaybackClock::Simulation,
            },
        ));
        animation_subject_commands.try_insert(AnimationPlayer::default());
        animation_subject_commands.try_insert(initial_animation_graph_playback_state);
        if let Some(source) = clip_sources.retained_handle(
            initial_animation_set_asset,
            &initial_animation_clip_asset_key,
        ) {
            animation_subject_commands.try_insert(openzt2_world_state::animation_playback::animation_playback_controller_types::RetainedAnimationClipSource(source));
        }
        animation_subject_commands
            .try_remove::<(PendingModelAnimationAssets, InitialAnimationSource)>();
    }
}

/// An actor without an authored initial clip should stand idle. The first
/// authored clip or graph node can be anything: the staff sets start with
/// `ClimbDown_2Stand`, which lowers the skeleton root underground.
fn select_initial_animation_clip_asset_key(
    animation_set_asset: &AnimationSetAsset,
) -> Option<String> {
    let clip_asset_keys = animation_set_asset
        .animation_clips
        .iter()
        .map(|clip| clip.animation_clip_asset_key.as_str())
        .chain(
            animation_set_asset
                .animation_clip_source_references
                .iter()
                .map(|reference| reference.animation_clip_asset_key.as_str()),
        );
    if let Some(idle) = preferred_idle_animation_clip_asset_key(clip_asset_keys) {
        return Some(idle.to_owned());
    }
    animation_set_asset
        .animation_graph_order
        .first()
        .and_then(|graph_node_asset_key| {
            animation_set_asset
                .animation_graph_nodes
                .iter()
                .find(|animation_graph_node| {
                    &animation_graph_node.animation_graph_node_asset_key == graph_node_asset_key
                })
        })
        .or_else(|| animation_set_asset.animation_graph_nodes.first())
        .map_or_else(
            || {
                animation_set_asset
                    .animation_clips
                    .first()
                    .map(|animation_clip| animation_clip.animation_clip_asset_key.clone())
                    .or_else(|| {
                        animation_set_asset
                            .animation_clip_source_references
                            .first()
                            .map(|animation_clip_source_reference| {
                                animation_clip_source_reference
                                    .animation_clip_asset_key
                                    .clone()
                            })
                    })
            },
            |graph_node| {
                select_initial_animation_clip_asset_key_from_graph_node(
                    animation_set_asset,
                    graph_node,
                )
            },
        )
}

/// `Stand_Idle` when the set has one, otherwise any `*_Idle` clip.
fn preferred_idle_animation_clip_asset_key<'a>(
    clip_asset_keys: impl Iterator<Item = &'a str>,
) -> Option<&'a str> {
    let clip_asset_keys = clip_asset_keys.collect::<Vec<_>>();
    clip_asset_keys
        .iter()
        .find(|key| key.eq_ignore_ascii_case("Stand_Idle"))
        .or_else(|| {
            clip_asset_keys
                .iter()
                .find(|key| key.to_ascii_lowercase().ends_with("_idle"))
        })
        .copied()
}

#[cfg(test)]
mod tests {
    use super::preferred_idle_animation_clip_asset_key;

    #[test]
    fn initial_clip_prefers_standing_idle_over_authored_order() {
        let staff = [
            "ClimbDown_2Stand",
            "SitBench_Idle",
            "Stand_Idle",
            "Walk_Ahead",
        ];
        assert_eq!(
            preferred_idle_animation_clip_asset_key(staff.into_iter()),
            Some("Stand_Idle")
        );
        let sitting = ["ClimbDown_2Stand", "SitBench_Idle"];
        assert_eq!(
            preferred_idle_animation_clip_asset_key(sitting.into_iter()),
            Some("SitBench_Idle")
        );
        let no_idle = ["ClimbDown_2Stand", "Walk_Ahead"];
        assert_eq!(
            preferred_idle_animation_clip_asset_key(no_idle.into_iter()),
            None
        );
    }
}
