use bevy::prelude::*;

use crate::assets::animation::animation_set_asset_types::AnimationSetAsset;

use super::{
    animation_playback_controller_types::AnimationPlaybackController,
    authored_animation_event_cursor_types::AuthoredAnimationEventCursor,
    authored_animation_text_action_execution_context::AuthoredAnimationTextActionExecutionContext,
};

pub(super) fn initialize_authored_animation_event_cursors_for_new_playback_controllers(
    mut commands: Commands,
    animation_playback_controllers: Query<
        (Entity, &AnimationPlaybackController),
        Added<AnimationPlaybackController>,
    >,
) {
    for (animation_playback_controller_entity, animation_playback_controller) in
        &animation_playback_controllers
    {
        commands
            .entity(animation_playback_controller_entity)
            .insert(AuthoredAnimationEventCursor {
                playback_generation: animation_playback_controller.playback_generation,
                animation_clip_asset_key: animation_playback_controller
                    .animation_clip_asset_key
                    .clone(),
                previous_elapsed_milliseconds: animation_playback_controller.elapsed_milliseconds,
                requires_initial_event_sample: true,
            });
    }
}

pub(super) fn execute_crossed_authored_animation_events(
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
    for (
        animation_playback_controller_entity,
        mut animation_playback_controller,
        mut authored_animation_event_cursor,
    ) in &mut changed_animation_playback_controllers
    {
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
        let animation_playback_clip_changed = authored_animation_event_cursor
            .requires_initial_event_sample
            || authored_animation_event_cursor.playback_generation
                != animation_playback_controller.playback_generation
            || authored_animation_event_cursor.animation_clip_asset_key
                != animation_playback_controller.animation_clip_asset_key;
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

        for (event_time_milliseconds, authored_animation_text_command) in
            authored_animation_clip_metadata
                .authored_animation_text_keys
                .iter()
                .flat_map(|authored_text_key| {
                    let event_time_milliseconds = calculate_authored_text_key_time_milliseconds(
                        authored_text_key,
                        animation_clip.duration_milliseconds,
                    );
                    authored_text_key.animation_text_commands.iter().map(
                        move |authored_animation_text_command| {
                            (event_time_milliseconds, authored_animation_text_command)
                        },
                    )
                })
        {
            if authored_animation_event_time_was_crossed_by_playback(
                event_time_milliseconds,
                animation_playback_controller.elapsed_milliseconds,
                authored_animation_event_cursor.previous_elapsed_milliseconds,
                animation_playback_clip_changed,
                animation_playback_moves_forward,
                animation_playback_time_wrapped,
            ) {
                authored_animation_text_action_execution
                    .execute_one_crossed_authored_animation_text_command(
                        animation_playback_controller_entity,
                        &mut animation_playback_controller,
                        event_time_milliseconds,
                        authored_animation_text_command,
                    );
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

fn calculate_authored_text_key_time_milliseconds(
    authored_text_key: &openzt2_game_data::animation::animation_text_key::AuthoredAnimationTextKey,
    animation_duration_milliseconds: u32,
) -> u32 {
    authored_text_key
        .authored_time_seconds
        .map(|seconds| (seconds.max(0.0) * 1000.0).round() as u32)
        .or_else(|| {
            authored_text_key
                .authored_frame_number
                .map(|frame| (frame.max(0.0) * 1000.0 / 30.0).round() as u32)
        })
        .or_else(|| {
            authored_text_key
                .authored_playback_percentage
                .map(|percent| {
                    let fraction = if percent > 1.0 {
                        percent / 100.0
                    } else {
                        percent
                    };
                    (fraction.clamp(0.0, 1.0) * animation_duration_milliseconds as f32).round()
                        as u32
                })
        })
        .unwrap_or(0)
}

#[cfg(test)]
mod tests;
