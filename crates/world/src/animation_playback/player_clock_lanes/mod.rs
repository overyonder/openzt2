use bevy::{
    animation::{advance_animations, RepeatAnimation},
    prelude::*,
};
use openzt2_game_assets::animation::animation_set_asset_types::AnimationClipSourceAsset;
use openzt2_world_state::animation_playback::{
    animation_event_message_types::AnimationCompleted,
    animation_playback_controller_types::{
        AnimationPlaybackController, RetainedAnimationClipSource,
    },
    locomotion_root_motion::ConsumedLocomotionRootMotion,
};

use super::authored_animation_event_execution::visit_interval;
use super::authored_animation_text_action_execution_context::AuthoredAnimationTextActionExecutionContext;
use super::managed_clip_binding::{
    FixedAnimationPlayer, ManagedAnimationClip, RealTimeAnimationPlayer,
};

pub(super) fn advance_render_players(
    time: Res<Time>,
    clips: Res<Assets<AnimationClip>>,
    graphs: Res<Assets<AnimationGraph>>,
    players: Query<
        (&mut AnimationPlayer, &AnimationGraphHandle),
        (
            Without<FixedAnimationPlayer>,
            Without<RealTimeAnimationPlayer>,
        ),
    >,
) {
    advance_animations(time, clips, graphs, players.transmute_lens_inner().query());
}

pub(super) struct AnimationPlaybackInterval {
    pub(super) from: f32,
    pub(super) to: f32,
    pub(super) cycle_start: f32,
    pub(super) cycle_end: f32,
    pub(super) cycles: i64,
    pub(super) forward: bool,
    pub(super) clip_entry: bool,
}

pub(super) fn advance_managed_animation_players<Lane: Component, const REAL_TIME: bool>(
    time: Res<Time<Fixed>>,
    real_time: Res<Time<Real>>,
    clips: Res<Assets<AnimationClip>>,
    sources: Res<Assets<AnimationClipSourceAsset>>,
    mut players: Query<
        (
            Entity,
            &mut AnimationPlaybackController,
            Ref<ManagedAnimationClip>,
            &mut AnimationPlayer,
            Option<&RetainedAnimationClipSource>,
            Option<&mut ConsumedLocomotionRootMotion>,
        ),
        With<Lane>,
    >,
    mut actions: AuthoredAnimationTextActionExecutionContext<REAL_TIME>,
    mut completed: MessageWriter<AnimationCompleted>,
) {
    let delta = if REAL_TIME {
        real_time.delta_secs()
    } else {
        time.delta_secs()
    };
    for (entity, mut controller, binding, mut player, source, motion) in &mut players {
        let mut motion = motion;
        if let Some(motion) = motion.as_deref_mut() {
            motion.clear_sample();
        }
        let Some(clip) = clips.get(&binding.clip) else {
            continue;
        };
        let Some(active) = player.animation_mut(binding.node) else {
            continue;
        };
        let clip_entry = binding.is_changed();
        if active.is_paused() && !clip_entry {
            continue;
        }
        let from = active.seek_time();
        let speed = active.speed();
        let mut requested = if active.is_paused() {
            from
        } else {
            delta.mul_add(speed, from)
        };
        let duration = clip.duration();
        if duration <= 0.0 {
            continue;
        }
        let repeat = active.repeat_mode() == RepeatAnimation::Forever;
        let source = source.and_then(|source| sources.get(&source.0));
        let cycle = if repeat {
            source
                .and_then(|source| source.loop_bounds.clone())
                .unwrap_or(0.0..duration)
        } else {
            0.0..duration
        };
        let boundary = if speed > 0.0 { cycle.end } else { cycle.start };
        let tolerance = f32::EPSILON * duration.max(1.0) * 8.0;
        if (requested - boundary).abs() <= tolerance {
            requested = boundary;
        }
        let (to, cycles) = if repeat
            && ((speed > 0.0 && requested >= cycle.end) || (speed < 0.0 && requested < cycle.start))
        {
            let span = cycle.end - cycle.start;
            let offset = requested - cycle.start;
            (
                cycle.start + offset.rem_euclid(span),
                offset.div_euclid(span) as i64,
            )
        } else {
            (requested.clamp(0.0, duration), 0)
        };
        let finished = !repeat
            && !active.is_paused()
            && ((speed > 0.0 && requested >= duration) || (speed < 0.0 && requested <= 0.0));
        let to = if finished { boundary } else { to };
        if to != from {
            active.set_seek_time(to);
        }
        if finished {
            active.pause();
            completed.write(AnimationCompleted {
                animation_playback_controller_entity: entity,
                playback_generation: controller.playback_generation,
                explicit_clip_request_id: controller.explicit_clip_request_id,
            });
        }
        let Some(source) = source else {
            continue;
        };
        if let Some(mut motion) = motion {
            if motion.owns_playback(&controller) {
                if let Some(bound) = &binding.root_motion {
                    let root_cycle = if repeat {
                        &bound.loop_cycle
                    } else {
                        &bound.full_cycle
                    };
                    motion.sample_interval(clip, root_cycle, from..to, cycles);
                }
            }
        }
        if controller.playback_generation == binding.generation
            && !source.timeline.is_empty()
            && (clip_entry || from != to || cycles != 0)
        {
            let interval = AnimationPlaybackInterval {
                from,
                to,
                cycle_start: cycle.start,
                cycle_end: cycle.end,
                cycles,
                forward: speed >= 0.0,
                clip_entry,
            };
            visit_interval(&interval, &source.timeline, &mut |key| {
                for command in &key.commands {
                    actions.execute_one_crossed_authored_animation_text_command(
                        entity,
                        &mut controller,
                        key.milliseconds,
                        command,
                    );
                }
            });
        }
    }
}
