use bevy::animation::AnimatedBy;
use bevy::animation::RepeatAnimation;
use bevy::prelude::*;

use openzt2_game_assets::animation::animation_set_asset_types::AnimationSetAsset;
use openzt2_world_state::animation_graph::animation_graph_playback_state_types::AnimationGraphPlaybackState;

use crate::animation_playback::animation_playback_controller_ancestor_lookup::find_animation_playback_controller_ancestor;
use crate::animation_playback::standard_bevy_animation_player_projection_types::StandardBevyAnimationGraphAndClipNodeIndexes;
use crate::animation_playback::standard_bevy_animation_player_projection_types::StandardBevyAnimationGraphsByAnimationSetAsset;
use crate::animation_playback::standard_bevy_animation_player_projection_types::StandardBevyAnimationJointBlendPose;
use crate::animation_playback::standard_bevy_animation_player_projection_types::StandardBevyAnimationPlayerBinding;
use openzt2_world_state::animation_playback::animation_playback_controller_types::AnimationPlaybackController;
use openzt2_world_state::animation_playback::animation_playback_controller_types::AnimationPlaybackRepetitionPolicy;

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
    joints: Query<(Entity, &AnimatedBy, &Transform)>,
    mut animation_players: Query<(
        Entity,
        &mut AnimationPlayer,
        Has<AnimationTransitions>,
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
        let cached_animation_graphs = &mut standard_bevy_animation_graphs_by_animation_set_asset
            .animation_graphs_by_animation_set_asset;
        // Clips that load on demand join the set after its graph was built;
        // the set's asset event arrives later, so rebuild as soon as the set
        // holds clips the cached graph lacks.
        if cached_animation_graphs
            .get(&animation_set_asset_id)
            .is_some_and(|cached| {
                cached.clip_node_indexes.len() != animation_set_asset.animation_clips.len()
            })
        {
            cached_animation_graphs.remove(&animation_set_asset_id);
        }
        let bevy_animation_graph_and_clip_node_indexes = cached_animation_graphs
            .entry(animation_set_asset_id)
            .or_insert_with(|| {
                let (bevy_animation_graph, bevy_animation_graph_nodes) = AnimationGraph::from_clips(
                    animation_set_asset
                        .animation_clips
                        .iter()
                        .map(|animation_clip| animation_clip.animation_clip_handle.clone()),
                );
                StandardBevyAnimationGraphAndClipNodeIndexes {
                    animation_graph_handle: bevy_animation_graph_assets.add(bevy_animation_graph),
                    clip_node_indexes: bevy_animation_graph_nodes,
                }
            });
        let animation_graph_id = bevy_animation_graph_and_clip_node_indexes
            .animation_graph_handle
            .id();
        let requested_bevy_animation_graph_node =
            bevy_animation_graph_and_clip_node_indexes.clip_node_indexes[animation_clip_index];
        let requested_animation_player_binding = StandardBevyAnimationPlayerBinding {
            animation_playback_controller_entity,
            animation_set_asset_id,
            animation_graph_id,
            animation_clip_asset_key: animation_playback_controller
                .animation_clip_asset_key
                .clone(),
            playback_generation: animation_playback_controller.playback_generation,
        };
        let animation_player_binding_changed =
            current_animation_player_binding != Some(&requested_animation_player_binding);
        if animation_transitions {
            commands
                .entity(animation_player_entity)
                .remove::<AnimationTransitions>();
        }
        let clip_entry_changed = current_animation_player_binding.is_some_and(|binding| {
            binding.playback_generation != requested_animation_player_binding.playback_generation
                || binding.animation_clip_asset_key
                    != requested_animation_player_binding.animation_clip_asset_key
        });
        if clip_entry_changed
            && animation_graph_playback_state
                .and_then(AnimationGraphPlaybackState::active_blend_duration_milliseconds)
                .is_some_and(|duration| duration > 0)
        {
            for (joint, player, transform) in &joints {
                if player.0 == animation_player_entity {
                    commands
                        .entity(joint)
                        .insert(StandardBevyAnimationJointBlendPose {
                            playback_generation: animation_playback_controller.playback_generation,
                            pose: *transform,
                        });
                }
            }
        }
        let active_bevy_animation = if animation_player_binding_changed {
            animation_player.stop_all();
            animation_player.start(requested_bevy_animation_graph_node)
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
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::animation::animation_curves::{AnimatableCurve, AnimatableKeyframeCurve};
    use bevy::animation::{animated_field, AnimationTargetId};
    use bevy::asset::AssetPlugin;
    use openzt2_game_assets::animation::animation_set_asset_types::AnimationClipRecord;
    use openzt2_world_state::animation_playback::animation_playback_controller_types::{
        AnimationPlaybackClock, AnimationPlaybackState,
    };
    use std::time::Duration;

    #[test]
    fn clip_changes_and_restarts_sample_the_authored_seek_positions(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default(), AnimationPlugin))
            .init_resource::<Assets<AnimationSetAsset>>()
            .init_resource::<StandardBevyAnimationGraphsByAnimationSetAsset>()
            .insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
                Duration::from_millis(100),
            ))
            .add_systems(
                Update,
                project_animation_playback_controller_state_to_standard_bevy_animation_players,
            );
        app.finish();
        app.cleanup();
        let target = AnimationTargetId::from_name(&Name::new("root"));
        let mut records = Vec::new();
        for (key, offset) in [("walk", 0.0), ("run", 10.0)] {
            let mut clip = AnimationClip::default();
            clip.add_curve_to_target(
                target,
                AnimatableCurve::new(
                    animated_field!(Transform::translation),
                    AnimatableKeyframeCurve::new([
                        (0.0, Vec3::X * offset),
                        (2.0, Vec3::X * (offset + 4.0)),
                    ])?,
                ),
            );
            let clip = app
                .world_mut()
                .resource_mut::<Assets<AnimationClip>>()
                .add(clip);
            records.push(AnimationClipRecord::create_embedded_animation_clip_record(
                key.to_owned(),
                clip,
                2000,
            ));
        }
        let set = app
            .world_mut()
            .resource_mut::<Assets<AnimationSetAsset>>()
            .add(AnimationSetAsset::create_ready_animation_set_asset(
                String::new(),
                String::new(),
                records,
                Vec::new(),
                Vec::new(),
                Vec::new(),
                None,
            ));
        let actor = app
            .world_mut()
            .spawn((
                AnimationPlayer::default(),
                AnimationPlaybackController {
                    playback_generation: 0,
                    explicit_clip_request_id: None,
                    animation_set_asset: set,
                    animation_clip_asset_key: "walk".to_owned(),
                    elapsed_milliseconds: 500,
                    fractional_milliseconds: default(),
                    playback_speed_permille: 1000,
                    playback_state: AnimationPlaybackState::Playing,
                    playback_repetition_policy: AnimationPlaybackRepetitionPolicy::PlayOnce,
                    playback_clock: AnimationPlaybackClock::Simulation,
                },
            ))
            .id();
        let joint = app
            .world_mut()
            .spawn((Transform::default(), target, AnimatedBy(actor)))
            .id();
        // Bevy threads newly created graph assets after their asset event is delivered.
        app.update();
        // The two independently specified curves are x = 2t and x = 10 + 2t.
        // Render time advances each update, while the authored seek stays fixed.
        for (key, generation, time, expected) in [
            ("walk", 0, 500, 1.0),
            ("run", 1, 500, 11.0),
            ("run", 2, 0, 10.0),
            ("walk", 3, 1000, 2.0),
        ] {
            {
                let mut controller = app
                    .world_mut()
                    .get_mut::<AnimationPlaybackController>(actor)
                    .ok_or("missing playback controller")?;
                controller.animation_clip_asset_key = key.to_owned();
                controller.playback_generation = generation;
                controller.elapsed_milliseconds = time;
            }
            app.update();
            let translation = app
                .world()
                .get::<Transform>(joint)
                .ok_or("missing posed joint")?
                .translation;
            assert!(
                translation.abs_diff_eq(Vec3::X * expected, 1e-5),
                "{key} generation {generation}: {translation:?}"
            );
        }
        Ok(())
    }
}
