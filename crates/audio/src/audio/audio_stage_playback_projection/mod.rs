use bevy::prelude::*;
use openzt2_game_assets::audio::audio_asset_types::{AudioAsset, AudioAssets};
use openzt2_game_data::{
    audio::{AudioStage, STAGE_INDOOR_OCCLUSION, STAGE_KEEP_AMBIENCE},
    AssetId,
};

use super::audio_playback_message_types::PlayAudioCue;
use super::audio_request_lanes::{ProducedAudioRequest, WATER_STAGE};
use super::water_audio_stage_projection::ListenerAudioEnvironment;

#[derive(Component, Clone, Copy, PartialEq)]
pub(super) struct EnvironmentVoiceGain(pub(super) f32);

#[derive(Component)]
pub(super) struct StageAmbiencePlayback {
    stage: AssetId,
    flags: u8,
    owner: Option<Entity>,
}

#[derive(Component)]
pub(super) struct StageAmbienceCrossfade {
    timer: Timer,
    outgoing: Option<Entity>,
    incoming: Option<Entity>,
    outgoing_gain: f32,
    incoming_stage: AssetId,
}

#[derive(Component)]
pub(super) struct LappingAudio {
    owner: Entity,
}

pub(super) fn select_listener_stage_ambience(
    mut commands: Commands,
    assets: Res<Assets<AudioAsset>>,
    catalogue: Res<AudioAssets>,
    changed: Query<Entity, Changed<ListenerAudioEnvironment>>,
    mut worlds: Query<(
        Entity,
        &ListenerAudioEnvironment,
        Option<&mut StageAmbiencePlayback>,
        Option<&mut StageAmbienceCrossfade>,
    )>,
    gains: Query<&EnvironmentVoiceGain>,
    mut requests: MessageWriter<ProducedAudioRequest<WATER_STAGE>>,
) {
    let Some(view) = catalogue.get(&assets) else {
        return;
    };
    let refresh = catalogue.is_changed();
    let candidates = if refresh {
        worlds.iter().map(|(entity, ..)| entity).collect::<Vec<_>>()
    } else {
        changed.iter().collect::<Vec<_>>()
    };
    let duration = view
        .water()
        .and_then(|policy| {
            std::time::Duration::try_from_secs_f32(policy.environment_transition_seconds).ok()
        })
        .unwrap_or_default();
    let mut worlds = worlds.iter_many_mut(candidates).matched();
    while let Some((world, environment, playback, transition)) = worlds.fetch_next() {
        let stage = view.stage(environment.stage);
        let flags = stage.map_or(0, |stage| stage.flags);
        if !refresh
            && playback
                .as_ref()
                .is_some_and(|playback| playback.stage == environment.stage)
        {
            continue;
        }
        let outgoing = playback.as_ref().and_then(|playback| playback.owner);
        let previous_flags = playback.as_ref().map_or(0, |playback| playback.flags);
        let keep_ambience = flags & STAGE_KEEP_AMBIENCE != 0;
        let fades = !duration.is_zero() && (previous_flags | flags) & STAGE_INDOOR_OCCLUSION != 0;
        let mut owner = outgoing;
        if !keep_ambience {
            if let Some(previous) = transition
                .as_ref()
                .and_then(|transition| transition.outgoing)
            {
                commands.entity(previous).try_despawn();
            }
            owner = stage
                .filter(|stage| stage.loop_cue.is_some() || !stage.ambient_cues.is_empty())
                .map(|stage| {
                    let owner = commands
                        .spawn((
                            ChildOf(world),
                            EnvironmentVoiceGain(if fades { 0.0 } else { 1.0 }),
                        ))
                        .id();
                    if let Some(cue) = stage.loop_cue {
                        play_stage_cue(&mut requests, owner, cue, true);
                    }
                    if !fades {
                        play_stage_ambients(&mut requests, owner, stage);
                    }
                    owner
                });
            if fades {
                let next = StageAmbienceCrossfade {
                    timer: Timer::new(duration, TimerMode::Once),
                    outgoing,
                    incoming: owner,
                    outgoing_gain: outgoing
                        .and_then(|owner| gains.get(owner).ok())
                        .map_or(1.0, |gain| gain.0),
                    incoming_stage: environment.stage,
                };
                if let Some(mut transition) = transition {
                    *transition = next;
                } else {
                    commands.entity(world).insert(next);
                }
            } else {
                if let Some(outgoing) = outgoing {
                    commands.entity(outgoing).try_despawn();
                }
                if transition.is_some() {
                    commands.entity(world).remove::<StageAmbienceCrossfade>();
                }
            }
        }
        let next = StageAmbiencePlayback {
            stage: environment.stage,
            flags,
            owner,
        };
        if let Some(mut playback) = playback {
            *playback = next;
        } else {
            commands.entity(world).insert(next);
        }
    }
}

pub(super) fn advance_listener_stage_crossfades(
    mut commands: Commands,
    time: Res<Time>,
    assets: Res<Assets<AudioAsset>>,
    catalogue: Res<AudioAssets>,
    mut transitions: Query<(Entity, &mut StageAmbienceCrossfade)>,
    mut gains: Query<&mut EnvironmentVoiceGain>,
    mut requests: MessageWriter<ProducedAudioRequest<WATER_STAGE>>,
) {
    for (root, mut transition) in &mut transitions {
        let fraction = transition.timer.tick(time.delta()).fraction();
        if let Some(mut outgoing) = transition
            .outgoing
            .and_then(|entity| gains.get_mut(entity).ok())
        {
            outgoing.set_if_neq(EnvironmentVoiceGain(
                (transition.outgoing_gain - fraction).max(0.0),
            ));
        }
        if let Some(mut incoming) = transition
            .incoming
            .and_then(|entity| gains.get_mut(entity).ok())
        {
            incoming.set_if_neq(EnvironmentVoiceGain(fraction));
        }
        if transition.timer.is_finished() {
            if let Some(outgoing) = transition.outgoing {
                commands.entity(outgoing).try_despawn();
            }
            if let Some((owner, stage)) = transition.incoming.zip(
                catalogue
                    .get(&assets)
                    .and_then(|view| view.stage(transition.incoming_stage)),
            ) {
                play_stage_ambients(&mut requests, owner, stage);
            }
            commands.entity(root).remove::<StageAmbienceCrossfade>();
        }
    }
}

pub(super) fn ramp_listener_shore_lapping(
    mut commands: Commands,
    time: Res<Time>,
    assets: Res<Assets<AudioAsset>>,
    catalogue: Res<AudioAssets>,
    worlds: Query<(Entity, &ListenerAudioEnvironment, Option<&LappingAudio>)>,
    mut gains: Query<&mut EnvironmentVoiceGain>,
    mut requests: MessageWriter<ProducedAudioRequest<WATER_STAGE>>,
) {
    let Some(view) = catalogue.get(&assets) else {
        return;
    };
    let Some(policy) = view.water() else {
        return;
    };
    let cue = AssetId::from_key("audio-cue:water_surface_lapping");
    for (world, environment, lapping) in &worlds {
        if let Some(lapping) = lapping {
            let Ok(mut gain) = gains.get_mut(lapping.owner) else {
                continue;
            };
            let step = if policy.lapping_gain_scale > 0.0 {
                time.delta_secs() / policy.lapping_gain_scale
            } else {
                1.0
            };
            let distance = environment.lapping_gain - gain.0;
            let next = gain.0 + distance.clamp(-step, step);
            gain.set_if_neq(EnvironmentVoiceGain(next));
        } else if environment.lapping_gain > 0.0 && view.cue(cue).is_some() {
            let owner = commands
                .spawn((ChildOf(world), EnvironmentVoiceGain(0.0)))
                .id();
            commands.entity(world).insert(LappingAudio { owner });
            play_stage_cue(&mut requests, owner, cue, true);
        }
    }
}

fn play_stage_ambients(
    requests: &mut MessageWriter<ProducedAudioRequest<WATER_STAGE>>,
    owner: Entity,
    stage: &AudioStage,
) {
    for &cue in &stage.ambient_cues {
        play_stage_cue(requests, owner, cue, false);
    }
}

fn play_stage_cue(
    requests: &mut MessageWriter<ProducedAudioRequest<WATER_STAGE>>,
    owner: Entity,
    cue: AssetId,
    looped: bool,
) {
    requests.write(ProducedAudioRequest::PlayOwned(
        PlayAudioCue {
            cue,
            emitter: None,
            selection: owner.to_bits(),
            priority: if looped { 64 } else { 32 },
            force_looped: looped,
        },
        owner,
    ));
}
