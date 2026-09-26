use std::time::Duration;

use bevy::{animation::RepeatAnimation, prelude::*};

use crate::{
    assets::animation::animation_set_asset_types::AnimationSetAsset,
    plugins::animation_graph::animation_graph_playback_state_types::AnimationGraphPlaybackState,
};

use super::{
    animation_playback_controller_ancestor_lookup::find_animation_playback_controller_ancestor,
    animation_playback_controller_types::{
        AnimationPlaybackController, AnimationPlaybackRepetitionPolicy,
    },
    standard_bevy_animation_player_projection_types::{
        StandardBevyAnimationGraphAndClipNodeIndexes,
        StandardBevyAnimationGraphsByAnimationSetAsset, StandardBevyAnimationPlayerBinding,
    },
};

pub(super) fn project_animation_playback_controller_state_to_standard_bevy_animation_players(
    mut commands: Commands,
    animation_set_assets: Res<Assets<AnimationSetAsset>>,
    mut bevy_animation_graph_assets: ResMut<Assets<AnimationGraph>>,
    mut standard_bevy_animation_graphs_by_animation_set_asset: ResMut<
        StandardBevyAnimationGraphsByAnimationSetAsset,
    >,
    animation_playback_controllers: Query<(
        &AnimationPlaybackController,
        Option<&AnimationGraphPlaybackState>,
    )>,
    entity_parents: Query<&ChildOf>,
    mut animation_players: Query<(
        Entity,
        &mut AnimationPlayer,
        Option<&mut AnimationTransitions>,
        Option<&StandardBevyAnimationPlayerBinding>,
    )>,
) {
    for (
        animation_player_entity,
        mut animation_player,
        animation_transitions,
        current_animation_player_binding,
    ) in &mut animation_players
    {
        let Some((
            animation_playback_controller_entity,
            animation_playback_controller,
            animation_graph_playback_state,
        )) = find_animation_playback_controller_ancestor(
            animation_player_entity,
            &entity_parents,
            &animation_playback_controllers,
        )
        else {
            continue;
        };
        let Some(animation_set_asset) =
            animation_set_assets.get(&animation_playback_controller.animation_set_asset)
        else {
            continue;
        };
        let Some(animation_clip_index) =
            animation_set_asset
                .animation_clips
                .iter()
                .position(|animation_clip| {
                    animation_clip.animation_clip_asset_key
                        == animation_playback_controller.animation_clip_asset_key
                })
        else {
            continue;
        };
        let animation_set_asset_id = animation_playback_controller.animation_set_asset.id();
        let bevy_animation_graph_and_clip_node_indexes =
            standard_bevy_animation_graphs_by_animation_set_asset
                .animation_graphs_by_animation_set_asset
                .entry(animation_set_asset_id)
                .or_insert_with(|| {
                    let (bevy_animation_graph, bevy_animation_graph_nodes) =
                        AnimationGraph::from_clips(
                            animation_set_asset
                                .animation_clips
                                .iter()
                                .map(|animation_clip| animation_clip.animation_clip_handle.clone()),
                        );
                    StandardBevyAnimationGraphAndClipNodeIndexes {
                        animation_graph_handle: bevy_animation_graph_assets
                            .add(bevy_animation_graph),
                        clip_node_indexes: bevy_animation_graph_nodes,
                    }
                });
        let requested_bevy_animation_graph_node =
            bevy_animation_graph_and_clip_node_indexes.clip_node_indexes[animation_clip_index];
        let requested_animation_player_binding = StandardBevyAnimationPlayerBinding {
            animation_playback_controller_entity,
            animation_set_asset_id,
            animation_clip_asset_key: animation_playback_controller
                .animation_clip_asset_key
                .clone(),
        };
        let animation_player_binding_changed =
            current_animation_player_binding != Some(&requested_animation_player_binding);
        let bound_animation_graph_changed = current_animation_player_binding
            .is_some_and(|binding| binding.animation_set_asset_id != animation_set_asset_id);
        let authored_blend_duration = animation_graph_playback_state
            .and_then(AnimationGraphPlaybackState::active_blend_duration_milliseconds)
            .map_or(Duration::ZERO, |milliseconds| {
                Duration::from_millis(u64::from(milliseconds))
            });

        let mut newly_created_animation_transitions = None;
        let active_bevy_animation = if animation_player_binding_changed {
            if bound_animation_graph_changed {
                animation_player.stop_all();
            }
            match animation_transitions {
                Some(mut animation_transitions) if !bound_animation_graph_changed => {
                    animation_transitions.play(
                        &mut animation_player,
                        requested_bevy_animation_graph_node,
                        authored_blend_duration,
                    )
                }
                _ => {
                    let mut animation_transitions = AnimationTransitions::new();
                    let active_bevy_animation = animation_transitions.play(
                        &mut animation_player,
                        requested_bevy_animation_graph_node,
                        Duration::ZERO,
                    );
                    newly_created_animation_transitions = Some(animation_transitions);
                    active_bevy_animation
                }
            }
        } else {
            let Some(active_bevy_animation) =
                animation_player.animation_mut(requested_bevy_animation_graph_node)
            else {
                continue;
            };
            active_bevy_animation
        };

        active_bevy_animation
            .set_speed(f32::from(animation_playback_controller.playback_speed_permille) / 1000.0)
            .set_seek_time(animation_playback_controller.elapsed_milliseconds as f32 / 1000.0);
        let playback_loops = match animation_playback_controller.playback_repetition_policy {
            AnimationPlaybackRepetitionPolicy::UseAuthoredClipPolicy => animation_set_asset
                .animation_clips[animation_clip_index]
                .authored_playback_is_looped(),
            AnimationPlaybackRepetitionPolicy::PlayOnce => false,
            AnimationPlaybackRepetitionPolicy::Loop => true,
        };
        if playback_loops {
            active_bevy_animation.repeat();
        } else {
            active_bevy_animation.set_repeat(RepeatAnimation::Never);
        }
        // The controller already advances the selected simulation/real clock,
        // including authored rate, looping and completion. Bevy must evaluate
        // this seek position without advancing it a second time in PostUpdate.
        // Paused active animations still contribute their sampled pose.
        active_bevy_animation.pause();

        if animation_player_binding_changed {
            let mut animation_player_commands = commands.entity(animation_player_entity);
            animation_player_commands.insert((
                AnimationGraphHandle(
                    bevy_animation_graph_and_clip_node_indexes
                        .animation_graph_handle
                        .clone(),
                ),
                requested_animation_player_binding,
            ));
            if let Some(newly_created_animation_transitions) = newly_created_animation_transitions {
                animation_player_commands.insert(newly_created_animation_transitions);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::animation_playback_controller_types::{
        AnimationPlaybackClock, AnimationPlaybackState,
    };
    use super::*;
    use crate::assets::animation::animation_set_asset_types::AnimationClipRecord;

    #[test]
    fn standard_player_samples_controller_time_without_a_second_clock_advance() {
        let mut app = App::new();
        app.init_resource::<Assets<AnimationSetAsset>>()
            .init_resource::<Assets<AnimationClip>>()
            .init_resource::<Assets<AnimationGraph>>()
            .init_resource::<StandardBevyAnimationGraphsByAnimationSetAsset>()
            .init_resource::<Time>()
            .add_systems(
                Update,
                (
                    project_animation_playback_controller_state_to_standard_bevy_animation_players,
                    bevy::animation::advance_animations,
                )
                    .chain(),
            );
        let mut clip = AnimationClip::default();
        clip.set_duration(2.0);
        let clip = app
            .world_mut()
            .resource_mut::<Assets<AnimationClip>>()
            .add(clip);
        let set = AnimationSetAsset::create_ready_animation_set_asset(
            String::new(),
            String::new(),
            vec![AnimationClipRecord::create_embedded_animation_clip_record(
                "idle".to_owned(),
                clip,
                2000,
            )],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            None,
        );
        let set = app
            .world_mut()
            .resource_mut::<Assets<AnimationSetAsset>>()
            .add(set);
        let actor = app
            .world_mut()
            .spawn((
                AnimationPlayer::default(),
                AnimationPlaybackController {
                    playback_generation: 0,
                    explicit_clip_request_id: None,
                    animation_set_asset: set,
                    animation_clip_asset_key: "idle".to_owned(),
                    elapsed_milliseconds: 500,
                    playback_speed_permille: 1000,
                    playback_state: AnimationPlaybackState::Playing,
                    playback_repetition_policy: AnimationPlaybackRepetitionPolicy::Loop,
                    playback_clock: AnimationPlaybackClock::Simulation,
                },
            ))
            .id();
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(Duration::from_millis(100));
        for _ in 0..2 {
            app.update();
            let player = app.world().get::<AnimationPlayer>(actor).unwrap();
            let (_, active) = player.playing_animations().next().unwrap();
            assert_eq!(active.seek_time(), 0.5);
            assert!(active.is_paused());
        }
    }
}
