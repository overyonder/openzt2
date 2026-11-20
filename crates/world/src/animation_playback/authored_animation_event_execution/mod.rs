use bevy::prelude::*;

use openzt2_game_assets::animation::animation_set_asset_types::AnimationSetAsset;

use crate::animation_playback::authored_animation_event_cursor_types::AuthoredAnimationEventCursor;
use crate::animation_playback::authored_animation_text_action_execution_context::AuthoredAnimationTextActionExecutionContext;
use openzt2_world_state::animation_playback::animation_playback_controller_types::{
    AnimationPlaybackClock, AnimationPlaybackController,
};

pub(super) fn execute_crossed_authored_animation_events<const REAL_TIME: bool>(
    animation_set_assets: Res<Assets<AnimationSetAsset>>,
    mut changed_animation_playback_controllers: Query<
        (
            Entity,
            &mut AnimationPlaybackController,
            &mut AuthoredAnimationEventCursor,
        ),
        Changed<AnimationPlaybackController>,
    >,
    mut authored_animation_text_action_execution: AuthoredAnimationTextActionExecutionContext,
) {
    let playback_clock = if REAL_TIME {
        AnimationPlaybackClock::RealTime
    } else {
        AnimationPlaybackClock::Simulation
    };
    for (
        animation_playback_controller_entity,
        mut animation_playback_controller,
        mut authored_animation_event_cursor,
    ) in &mut changed_animation_playback_controllers
    {
        if animation_playback_controller.playback_clock != playback_clock {
            continue;
        }
        let animation_playback_clip_changed = authored_animation_event_cursor
            .requires_initial_event_sample
            || authored_animation_event_cursor.playback_generation
                != animation_playback_controller.playback_generation
            || authored_animation_event_cursor.animation_clip_asset_key
                != animation_playback_controller.animation_clip_asset_key;
        if !animation_playback_clip_changed
            && authored_animation_event_cursor.previous_elapsed_milliseconds
                == animation_playback_controller.elapsed_milliseconds
        {
            continue;
        }
        let Some(animation_clip) = animation_set_assets
            .get(&animation_playback_controller.animation_set_asset)
            .and_then(|animation_set_asset| {
                animation_set_asset.find_animation_clip_record_by_asset_key(
                    &animation_playback_controller.animation_clip_asset_key,
                )
            })
        else {
            continue;
        };
        let animation_playback_moves_forward =
            animation_playback_controller.playback_speed_permille >= 0;
        let animation_playback_time_wrapped = !animation_playback_clip_changed
            && if animation_playback_moves_forward {
                animation_playback_controller.elapsed_milliseconds
                    < authored_animation_event_cursor.previous_elapsed_milliseconds
            } else {
                animation_playback_controller.elapsed_milliseconds
                    > authored_animation_event_cursor.previous_elapsed_milliseconds
            };
        let Some(authored_animation_clip_metadata) =
            animation_clip.authored_animation_clip_metadata.as_ref()
        else {
            update_authored_animation_event_cursor_after_execution(
                &mut authored_animation_event_cursor,
                &animation_playback_controller,
            );
            continue;
        };

        for (&event_time_milliseconds, text_key) in animation_clip
            .text_key_times_milliseconds
            .iter()
            .zip(&authored_animation_clip_metadata.authored_animation_text_keys)
        {
            if authored_animation_event_time_was_crossed_by_playback(
                event_time_milliseconds,
                animation_playback_controller.elapsed_milliseconds,
                authored_animation_event_cursor.previous_elapsed_milliseconds,
                animation_playback_clip_changed,
                animation_playback_moves_forward,
                animation_playback_time_wrapped,
            ) {
                for command in &text_key.animation_text_commands {
                    authored_animation_text_action_execution
                        .execute_one_crossed_authored_animation_text_command(
                            animation_playback_controller_entity,
                            &mut animation_playback_controller,
                            event_time_milliseconds,
                            command,
                        );
                }
            }
        }
        update_authored_animation_event_cursor_after_execution(
            &mut authored_animation_event_cursor,
            &animation_playback_controller,
        );
    }
}

fn update_authored_animation_event_cursor_after_execution(
    authored_animation_event_cursor: &mut AuthoredAnimationEventCursor,
    animation_playback_controller: &AnimationPlaybackController,
) {
    authored_animation_event_cursor
        .animation_clip_asset_key
        .clone_from(&animation_playback_controller.animation_clip_asset_key);
    authored_animation_event_cursor.previous_elapsed_milliseconds =
        animation_playback_controller.elapsed_milliseconds;
    authored_animation_event_cursor.requires_initial_event_sample = false;
    authored_animation_event_cursor.playback_generation =
        animation_playback_controller.playback_generation;
}

fn authored_animation_event_time_was_crossed_by_playback(
    animation_event_time_milliseconds: u32,
    current_elapsed_milliseconds: u32,
    previous_elapsed_milliseconds: u32,
    animation_playback_clip_changed: bool,
    animation_playback_moves_forward: bool,
    animation_playback_time_wrapped: bool,
) -> bool {
    if animation_playback_clip_changed {
        if animation_playback_moves_forward {
            animation_event_time_milliseconds <= current_elapsed_milliseconds
        } else {
            animation_event_time_milliseconds >= current_elapsed_milliseconds
        }
    } else if animation_playback_moves_forward && animation_playback_time_wrapped {
        animation_event_time_milliseconds > previous_elapsed_milliseconds
            || animation_event_time_milliseconds <= current_elapsed_milliseconds
    } else if animation_playback_moves_forward {
        animation_event_time_milliseconds > previous_elapsed_milliseconds
            && animation_event_time_milliseconds <= current_elapsed_milliseconds
    } else if animation_playback_time_wrapped {
        animation_event_time_milliseconds < previous_elapsed_milliseconds
            || animation_event_time_milliseconds >= current_elapsed_milliseconds
    } else {
        animation_event_time_milliseconds < previous_elapsed_milliseconds
            && animation_event_time_milliseconds >= current_elapsed_milliseconds
    }
}

#[cfg(test)]
mod tests;
