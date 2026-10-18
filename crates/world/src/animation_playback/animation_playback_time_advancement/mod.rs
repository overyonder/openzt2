use bevy::prelude::*;

use openzt2_game_assets::animation::animation_set_asset_types::AnimationSetAsset;

use openzt2_world_state::animation_playback::animation_event_message_types::AnimationCompleted;
use openzt2_world_state::animation_playback::animation_playback_controller_types::AnimationPlaybackClock;
use openzt2_world_state::animation_playback::animation_playback_controller_types::AnimationPlaybackController;
use openzt2_world_state::animation_playback::animation_playback_controller_types::AnimationPlaybackRepetitionPolicy;
use openzt2_world_state::animation_playback::animation_playback_controller_types::AnimationPlaybackState;
use openzt2_world_state::animation_playback::locomotion_root_motion::ConsumedLocomotionRootMotion;

pub(super) fn advance_simulation_clock_animation_playback_controllers(
    fixed_time: Res<Time<Fixed>>,
    animation_set_assets: Res<Assets<AnimationSetAsset>>,
    clips: Res<Assets<AnimationClip>>,
    mut animation_playback_controllers: Query<(
        Entity,
        &mut AnimationPlaybackController,
        Option<&mut ConsumedLocomotionRootMotion>,
    )>,
    mut completed_animations: MessageWriter<AnimationCompleted>,
) {
    for (animation_playback_controller_entity, mut animation_playback_controller, consumed) in
        &mut animation_playback_controllers
    {
        let elapsed_milliseconds = advance_scaled_animation_clock(
            &mut animation_playback_controller,
            fixed_time.delta(),
            &animation_set_assets,
            AnimationPlaybackClock::Simulation,
        );
        if let Some(mut consumed) = consumed {
            consumed.clear_sample();
            if animation_playback_controller.playback_clock == AnimationPlaybackClock::Simulation {
                sample_consumed_animation_motion(
                    &animation_playback_controller,
                    &mut consumed,
                    elapsed_milliseconds,
                    &animation_set_assets,
                    &clips,
                );
            }
        }
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
    for (animation_playback_controller_entity, mut animation_playback_controller) in
        &mut animation_playback_controllers
    {
        let elapsed_milliseconds = advance_scaled_animation_clock(
            &mut animation_playback_controller,
            real_time.delta(),
            &animation_set_assets,
            AnimationPlaybackClock::RealTime,
        );
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
    if elapsed_milliseconds == 0 {
        return false;
    }
    let animation_playback_loops = match animation_playback_controller.playback_repetition_policy {
        AnimationPlaybackRepetitionPolicy::UseAuthoredClipPolicy => {
            animation_clip.authored_playback_is_looped()
        }
        AnimationPlaybackRepetitionPolicy::PlayOnce => false,
        AnimationPlaybackRepetitionPolicy::Loop => true,
    };

    let duration = i64::from(animation_clip.duration_milliseconds);
    if duration == 0 {
        return false;
    }
    let forward = animation_playback_controller.playback_speed_permille > 0;
    let advance = i64::from(elapsed_milliseconds);
    let next = i64::from(animation_playback_controller.elapsed_milliseconds)
        + if forward { advance } else { -advance };
    let completed = !animation_playback_loops && if forward { next >= duration } else { next <= 0 };
    let next = if animation_playback_loops {
        next.rem_euclid(duration)
    } else {
        next.clamp(0, duration)
    };
    animation_playback_controller.elapsed_milliseconds = u32::try_from(next).unwrap_or_default();
    if completed {
        animation_playback_controller.playback_state = AnimationPlaybackState::Finished;
    }
    completed
}

fn sample_consumed_animation_motion(
    controller: &AnimationPlaybackController,
    consumed: &mut ConsumedLocomotionRootMotion,
    elapsed_milliseconds: u32,
    sets: &Assets<AnimationSetAsset>,
    clips: &Assets<AnimationClip>,
) {
    if !consumed.owns_playback(controller)
        || controller.playback_state != AnimationPlaybackState::Playing
    {
        return;
    }
    let Some(record) = sets.get(&controller.animation_set_asset).and_then(|set| {
        set.find_animation_clip_record_by_asset_key(&controller.animation_clip_asset_key)
    }) else {
        return;
    };
    let Some(root) = record
        .authored_animation_clip_metadata
        .as_ref()
        .and_then(|metadata| metadata.root_motion_joint.as_ref())
    else {
        return;
    };
    let Some(clip) = clips.get(&record.animation_clip_handle) else {
        return;
    };
    let duration = i64::from(record.duration_milliseconds);
    if duration == 0 {
        return;
    }
    let advance = i64::from(elapsed_milliseconds);
    let next = i64::from(controller.elapsed_milliseconds)
        + if controller.playback_speed_permille < 0 {
            -advance
        } else {
            advance
        };
    let loops = match controller.playback_repetition_policy {
        AnimationPlaybackRepetitionPolicy::Loop => true,
        AnimationPlaybackRepetitionPolicy::PlayOnce => false,
        AnimationPlaybackRepetitionPolicy::UseAuthoredClipPolicy => {
            record.authored_playback_is_looped()
        }
    };
    let (next, cycles) = if loops {
        (next.rem_euclid(duration), next.div_euclid(duration))
    } else {
        (next.clamp(0, duration), 0)
    };
    consumed.sample_interval(
        clip,
        &root.joint_asset_key,
        std::time::Duration::from_millis(u64::from(controller.elapsed_milliseconds)).as_secs_f32()
            ..std::time::Duration::from_millis(next.unsigned_abs()).as_secs_f32(),
        cycles,
        std::time::Duration::from_millis(duration.unsigned_abs()).as_secs_f32(),
    );
}

fn advance_scaled_animation_clock(
    controller: &mut AnimationPlaybackController,
    delta: std::time::Duration,
    sets: &Assets<AnimationSetAsset>,
    clock: AnimationPlaybackClock,
) -> u32 {
    if controller.playback_clock != clock
        || controller.playback_state != AnimationPlaybackState::Playing
    {
        return 0;
    }
    let Some(record) = sets.get(&controller.animation_set_asset).and_then(|set| {
        set.find_animation_clip_record_by_asset_key(&controller.animation_clip_asset_key)
    }) else {
        return 0;
    };
    let rate = f64::from(controller.playback_speed_permille.unsigned_abs()) / 1000.0
        * f64::from(record.authored_playback_rate());
    controller.fractional_milliseconds.advance(delta, rate)
}
