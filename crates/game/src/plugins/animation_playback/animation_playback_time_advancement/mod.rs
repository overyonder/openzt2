use bevy::prelude::*;

use crate::assets::animation::animation_set_asset_types::AnimationSetAsset;

use super::{
    animation_event_message_types::AnimationCompleted,
    animation_playback_controller_types::{
        AnimationPlaybackClock, AnimationPlaybackController, AnimationPlaybackRepetitionPolicy,
        AnimationPlaybackState,
    },
};

pub(super) fn advance_simulation_clock_animation_playback_controllers(
    fixed_time: Res<Time<Fixed>>,
    animation_set_assets: Res<Assets<AnimationSetAsset>>,
    mut animation_playback_controllers: Query<(Entity, &mut AnimationPlaybackController)>,
    mut completed_animations: MessageWriter<AnimationCompleted>,
) {
    let elapsed_milliseconds = convert_duration_to_saturating_u32_milliseconds(fixed_time.delta());
    for (animation_playback_controller_entity, mut animation_playback_controller) in
        &mut animation_playback_controllers
    {
        if animation_playback_controller.playback_clock == AnimationPlaybackClock::Simulation
            && advance_one_animation_playback_controller_and_report_clip_completion(
                &mut animation_playback_controller,
                elapsed_milliseconds,
                &animation_set_assets,
            )
        {
            completed_animations.write(AnimationCompleted {
                animation_playback_controller_entity,
                playback_generation: animation_playback_controller.playback_generation,
                explicit_clip_request_id: animation_playback_controller.explicit_clip_request_id,
            });
        }
    }
}

pub(super) fn advance_real_time_clock_animation_playback_controllers(
    real_time: Res<Time<Real>>,
    animation_set_assets: Res<Assets<AnimationSetAsset>>,
    mut animation_playback_controllers: Query<(Entity, &mut AnimationPlaybackController)>,
    mut completed_animations: MessageWriter<AnimationCompleted>,
) {
    let elapsed_milliseconds = convert_duration_to_saturating_u32_milliseconds(real_time.delta());
    for (animation_playback_controller_entity, mut animation_playback_controller) in
        &mut animation_playback_controllers
    {
        if animation_playback_controller.playback_clock == AnimationPlaybackClock::RealTime
            && advance_one_animation_playback_controller_and_report_clip_completion(
                &mut animation_playback_controller,
                elapsed_milliseconds,
                &animation_set_assets,
            )
        {
            completed_animations.write(AnimationCompleted {
                animation_playback_controller_entity,
                playback_generation: animation_playback_controller.playback_generation,
                explicit_clip_request_id: animation_playback_controller.explicit_clip_request_id,
            });
        }
    }
}

fn advance_one_animation_playback_controller_and_report_clip_completion(
    animation_playback_controller: &mut AnimationPlaybackController,
    elapsed_milliseconds: u32,
    animation_set_assets: &Assets<AnimationSetAsset>,
) -> bool {
    if animation_playback_controller.playback_state != AnimationPlaybackState::Playing
        || animation_playback_controller.playback_speed_permille == 0
    {
        return false;
    }
    let Some(animation_clip) = animation_set_assets
        .get(&animation_playback_controller.animation_set_asset)
        .and_then(|animation_set_asset| {
            animation_set_asset.find_animation_clip_record_by_asset_key(
                &animation_playback_controller.animation_clip_asset_key,
            )
        })
    else {
        return false;
    };
    let scaled_elapsed_milliseconds = (elapsed_milliseconds as f32
        * f32::from(
            animation_playback_controller
                .playback_speed_permille
                .unsigned_abs(),
        )
        * animation_clip.authored_playback_rate()
        / 1000.0)
        .round()
        .clamp(0.0, u32::MAX as f32) as u32;
    let animation_playback_loops = match animation_playback_controller.playback_repetition_policy {
        AnimationPlaybackRepetitionPolicy::UseAuthoredClipPolicy => {
            animation_clip.authored_playback_is_looped()
        }
        AnimationPlaybackRepetitionPolicy::PlayOnce => false,
        AnimationPlaybackRepetitionPolicy::Loop => true,
    };

    if animation_playback_controller.playback_speed_permille > 0 {
        let next_elapsed_milliseconds = animation_playback_controller
            .elapsed_milliseconds
            .saturating_add(scaled_elapsed_milliseconds);
        if animation_playback_loops {
            animation_playback_controller.elapsed_milliseconds =
                next_elapsed_milliseconds % animation_clip.duration_milliseconds;
        } else if next_elapsed_milliseconds >= animation_clip.duration_milliseconds {
            animation_playback_controller.elapsed_milliseconds =
                animation_clip.duration_milliseconds;
            animation_playback_controller.playback_state = AnimationPlaybackState::Finished;
            return true;
        } else {
            animation_playback_controller.elapsed_milliseconds = next_elapsed_milliseconds;
        }
    } else if scaled_elapsed_milliseconds > animation_playback_controller.elapsed_milliseconds {
        if animation_playback_loops {
            animation_playback_controller.elapsed_milliseconds = (animation_clip
                .duration_milliseconds
                - ((scaled_elapsed_milliseconds
                    - animation_playback_controller.elapsed_milliseconds)
                    % animation_clip.duration_milliseconds))
                % animation_clip.duration_milliseconds;
        } else {
            animation_playback_controller.elapsed_milliseconds = 0;
            animation_playback_controller.playback_state = AnimationPlaybackState::Finished;
            return true;
        }
    } else {
        animation_playback_controller.elapsed_milliseconds -= scaled_elapsed_milliseconds;
    }

    false
}

fn convert_duration_to_saturating_u32_milliseconds(elapsed_duration: std::time::Duration) -> u32 {
    elapsed_duration.as_millis().min(u128::from(u32::MAX)) as u32
}
