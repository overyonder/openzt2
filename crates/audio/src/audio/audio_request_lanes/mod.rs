use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use super::audio_playback_message_types::{PlayAudioCue, StopAudioCue};

pub(super) const WATER_STAGE: u8 = 0;
pub(super) const SOUNDSCAPE_SETUP: u8 = 1;
pub(super) const SOUNDSCAPE_TIMER: u8 = 2;
pub(super) const CROWD: u8 = 3;
pub(super) const AMBIENT_EMITTER: u8 = 4;
pub(super) const MENU_MUSIC: u8 = 5;

#[derive(Message)]
pub(super) enum ProducedAudioRequest<const LANE: u8> {
    Play(PlayAudioCue),
    Stop(StopAudioCue),
    PlayOwned(PlayAudioCue, Entity),
}

#[derive(Message)]
pub(super) struct AdmitAudioCue {
    pub(super) cue: PlayAudioCue,
    pub(super) lifetime_owner: Option<Entity>,
}

#[derive(SystemParam)]
pub(super) struct AudioRequestLanes<'w, 's> {
    water: MessageReader<'w, 's, ProducedAudioRequest<WATER_STAGE>>,
    soundscape: MessageReader<'w, 's, ProducedAudioRequest<SOUNDSCAPE_SETUP>>,
    timer: MessageReader<'w, 's, ProducedAudioRequest<SOUNDSCAPE_TIMER>>,
    crowd: MessageReader<'w, 's, ProducedAudioRequest<CROWD>>,
    ambient: MessageReader<'w, 's, ProducedAudioRequest<AMBIENT_EMITTER>>,
    menu: MessageReader<'w, 's, ProducedAudioRequest<MENU_MUSIC>>,
}

pub(super) fn merge_produced_audio_requests(
    mut external: MessageReader<PlayAudioCue>,
    mut lanes: AudioRequestLanes,
    mut cues: MessageWriter<AdmitAudioCue>,
    mut stops: MessageWriter<StopAudioCue>,
) {
    for &cue in external.read() {
        cues.write(AdmitAudioCue {
            cue,
            lifetime_owner: None,
        });
    }
    merge_lane(&mut lanes.water, &mut cues, &mut stops);
    merge_lane(&mut lanes.soundscape, &mut cues, &mut stops);
    merge_lane(&mut lanes.timer, &mut cues, &mut stops);
    merge_lane(&mut lanes.crowd, &mut cues, &mut stops);
    merge_lane(&mut lanes.ambient, &mut cues, &mut stops);
    merge_lane(&mut lanes.menu, &mut cues, &mut stops);
}

fn merge_lane<const LANE: u8>(
    lane: &mut MessageReader<ProducedAudioRequest<LANE>>,
    cues: &mut MessageWriter<AdmitAudioCue>,
    stops: &mut MessageWriter<StopAudioCue>,
) {
    for request in lane.read() {
        match *request {
            ProducedAudioRequest::Play(cue) => {
                cues.write(AdmitAudioCue {
                    cue,
                    lifetime_owner: None,
                });
            }
            ProducedAudioRequest::PlayOwned(cue, owner) => {
                cues.write(AdmitAudioCue {
                    cue,
                    lifetime_owner: Some(owner),
                });
            }
            ProducedAudioRequest::Stop(stop) => {
                stops.write(stop);
            }
        }
    }
}
