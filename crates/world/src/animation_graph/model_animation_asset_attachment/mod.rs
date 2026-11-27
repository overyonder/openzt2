use crate::world_spawn::prefab_renderable_components::PrefabAnimationBinding;
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
pub(crate) struct InitialAnimationSeek(pub(crate) f32);

#[derive(Component)]
pub(super) struct InitialAnimationSelection {
    set: Handle<AnimationSetAsset>,
    library: Box<[Handle<AnimationSetAsset>]>,
    clip: String,
    source: Handle<AnimationClipSourceAsset>,
    repetition: AnimationPlaybackRepetitionPolicy,
}

pub(super) fn attach_loaded_animation_set_assets_to_animation_subject_entities(
    mut commands: Commands,
    animation_set_assets: Res<Assets<AnimationSetAsset>>,
    asset_server: Res<AssetServer>,
    clip_sources: AnimationClipSources,
    presentation_owners: Query<&AnimationPresentationOwner>,
    live_world_members: Query<
        (),
        With<openzt2_world_state::world_spawn::world_membership_types::WorldMember>,
    >,
    parents: Query<&ChildOf>,
    mut pending_animation_subjects: Query<
        (
            Entity,
            Option<&PrefabModel>,
            Option<&SelectedAnimalPresentationModel>,
            Option<&ChildOf>,
            Ref<PendingModelAnimationAssets>,
            Option<&PrefabAnimationBinding>,
            Option<&mut InitialAnimationSelection>,
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
        authored_binding,
        initial_source,
        expanded,
    ) in &mut pending_animation_subjects
    {
        if prefab_model.is_some() && !expanded {
            continue;
        }
        let handles = &pending_model_animation_assets.pending_model_animation_set_asset_handles;
        let (
            initial_animation_set_asset_handle,
            loaded_model_animation_set_asset_handles,
            initial_animation_clip_asset_key,
            repetition,
        ) = if let Some(initial) = initial_source
            .as_deref()
            .filter(|_| !pending_model_animation_assets.is_changed())
        {
            if clip_sources.failed(&initial.source) {
                finish_animation_asset_attachment(&mut commands, animation_subject_entity);
                continue;
            }
            if !clip_sources.resolved(&initial.source) {
                continue;
            }
            (
                initial.set.clone(),
                initial.library.clone(),
                initial.clip.clone(),
                initial.repetition,
            )
        } else {
            if handles.iter().any(|handle| {
                !animation_set_assets.contains(handle.id())
                    && !matches!(
                        asset_server.load_state(handle.id()),
                        bevy::asset::LoadState::Failed(_)
                    )
            }) {
                continue;
            }
            let matching = handles
                .iter()
                .filter_map(|handle| {
                    let set = animation_set_assets.get(handle)?;
                    let model = prefab_model
                        .map(|model| model.model)
                        .or_else(|| animal_model.map(|model| model.model));
                    let matches = set.model_asset_path.is_empty()
                        || model.is_none_or(|model| {
                            openzt2_game_data::AssetId::from_virtual_path(
                                &AssetPath::new(&set.model_asset_path).key(),
                            ) == model
                        })
                        || prefab_model.is_some_and(|model| {
                            set.model_asset_path.eq_ignore_ascii_case(&model.model_path)
                        });
                    matches.then_some((handle, set))
                })
                .collect::<Vec<_>>();
            let explicit = pending_model_animation_assets
                .requested_initial_animation_clip_asset_key
                .as_deref();
            let selected = explicit
                .and_then(|key| {
                    matching.iter().find_map(|&(handle, set)| {
                        resolve_initial_animation_clip(set, key).map(|clip| {
                            let repetition = pending_model_animation_assets
                                .requested_initial_repetition_policy
                                .or_else(|| {
                                    authored_binding
                                        .and_then(|binding| binding.start.as_ref())
                                        .map(|start| {
                                            if start.looping {
                                                AnimationPlaybackRepetitionPolicy::Loop
                                            } else {
                                                AnimationPlaybackRepetitionPolicy::PlayOnce
                                            }
                                        })
                                })
                                .unwrap_or(
                                    AnimationPlaybackRepetitionPolicy::UseAuthoredClipPolicy,
                                );
                            (handle, clip, repetition)
                        })
                    })
                })
                .or_else(|| {
                    if authored_binding.is_none() {
                        matching.iter().find_map(|&(handle, set)| {
                            select_initial_animation_clip_asset_key(set).map(|clip| {
                                (
                                    handle,
                                    clip,
                                    AnimationPlaybackRepetitionPolicy::UseAuthoredClipPolicy,
                                )
                            })
                        })
                    } else {
                        matching.first().map(|(handle, _)| {
                            (
                                *handle,
                                String::new(),
                                AnimationPlaybackRepetitionPolicy::UseAuthoredClipPolicy,
                            )
                        })
                    }
                });
            let Some((set, clip, repetition)) = selected else {
                finish_animation_asset_attachment(&mut commands, animation_subject_entity);
                continue;
            };
            let library = matching
                .into_iter()
                .map(|(handle, _)| handle.clone())
                .collect::<Box<[_]>>();
            (set.clone(), library, clip, repetition)
        };
        let Some(initial_animation_set_asset) =
            animation_set_assets.get(&initial_animation_set_asset_handle)
        else {
            continue;
        };
        if !initial_animation_clip_asset_key.is_empty()
            && clip_sources
                .get(
                    initial_animation_set_asset,
                    &initial_animation_clip_asset_key,
                )
                .is_none()
        {
            let source = clip_sources.request(
                initial_animation_set_asset,
                &initial_animation_clip_asset_key,
            );
            if let Some(source) = source.filter(|source| !clip_sources.failed(source)) {
                let next = InitialAnimationSelection {
                    set: initial_animation_set_asset_handle,
                    library: loaded_model_animation_set_asset_handles,
                    clip: initial_animation_clip_asset_key,
                    source,
                    repetition,
                };
                if let Some(mut current) = initial_source {
                    *current = next;
                } else {
                    commands.entity(animation_subject_entity).try_insert(next);
                }
            } else {
                finish_animation_asset_attachment(&mut commands, animation_subject_entity);
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
        let in_live_world = std::iter::once(animation_subject_entity)
            .chain(parents.iter_ancestors::<ChildOf>(animation_subject_entity))
            .any(|ancestor| live_world_members.contains(ancestor));
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

                playback_repetition_policy: repetition,
                playback_clock: if in_live_world {
                    AnimationPlaybackClock::Simulation
                } else {
                    authored_binding
                        .map_or(AnimationPlaybackClock::Simulation, |binding| binding.clock)
                },
            },
        ));
        if let Some(start) = authored_binding.and_then(|binding| binding.start.as_ref()) {
            if start.time_seconds > 0.0 && !initial_animation_clip_asset_key.is_empty() {
                animation_subject_commands.try_insert(InitialAnimationSeek(start.time_seconds));
            }
        }
        animation_subject_commands.try_insert(AnimationPlayer::default());
        animation_subject_commands.try_insert(initial_animation_graph_playback_state);
        if let Some(source) = clip_sources.retained_handle(
            initial_animation_set_asset,
            &initial_animation_clip_asset_key,
        ) {
            animation_subject_commands.try_insert(openzt2_world_state::animation_playback::animation_playback_controller_types::RetainedAnimationClipSource(source));
        }
        animation_subject_commands
            .try_remove::<(PendingModelAnimationAssets, InitialAnimationSelection)>();
    }
}

fn finish_animation_asset_attachment(commands: &mut Commands, entity: Entity) {
    commands
        .entity(entity)
        .try_insert(ModelAnimationAttachmentResolved)
        .try_remove::<(PendingModelAnimationAssets, InitialAnimationSelection)>();
}

fn resolve_initial_animation_clip(set: &AnimationSetAsset, key: &str) -> Option<String> {
    resolve_animation_clip_or_graph_node_asset_key(set, key).or_else(|| {
        set.animation_clips()
            .iter()
            .map(|clip| &clip.animation_clip_asset_key)
            .chain(
                set.animation_clip_source_references()
                    .iter()
                    .map(|clip| &clip.animation_clip_asset_key),
            )
            .find(|clip| clip.eq_ignore_ascii_case(key))
            .cloned()
    })
}

fn select_initial_animation_clip_asset_key(
    animation_set_asset: &AnimationSetAsset,
) -> Option<String> {
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
                    .animation_clips()
                    .first()
                    .map(|animation_clip| animation_clip.animation_clip_asset_key.clone())
                    .or_else(|| {
                        animation_set_asset
                            .animation_clip_source_references()
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
