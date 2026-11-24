use std::{
    sync::{
        atomic::{AtomicU32, Ordering},
        mpsc::{self, Receiver, Sender},
        Arc, Mutex,
    },
    time::Duration,
};

use bevy::{
    asset::AssetLoadFailedEvent,
    audio::{
        AudioPlayer, AudioSource, ChannelCount, Decodable, PlaybackMode, PlaybackSettings,
        SampleRate, SeekError, Source,
    },
    prelude::*,
};
use openzt2_game_data::{
    audio::{AudioStage, STAGE_INDOOR_OCCLUSION},
    AssetId,
};

use super::{
    audio_stage_playback_projection::StageAmbienceCrossfade,
    authored_room_stages::ListenerRoomStages,
    water_audio_stage_projection::ListenerAudioEnvironment,
};

#[derive(Clone, Copy, Default, PartialEq)]
pub(super) struct StageAcoustics {
    layer: Option<AssetId>,
    room: u8,
    room_effect: f32,
    occlusion: f32,
    exclusion: f32,
    obstruction: f32,
    indoor: bool,
}

impl From<&AudioStage> for StageAcoustics {
    fn from(stage: &AudioStage) -> Self {
        Self {
            layer: layer_key(&stage.filter_layer),
            room: ROOMS
                .iter()
                .position(|room| room.0.eq_ignore_ascii_case(&stage.room_type))
                .unwrap_or_default() as u8,
            room_effect: stage.room_effect,
            occlusion: stage.occlusion,
            exclusion: stage.exclusion,
            obstruction: stage.obstruction,
            indoor: stage.flags & STAGE_INDOOR_OCCLUSION != 0,
        }
    }
}

fn layer_key(layer: &str) -> Option<AssetId> {
    let layer = layer.trim();
    (!layer.is_empty()).then(|| AssetId::from_key(&layer.to_ascii_lowercase()))
}

#[derive(Component)]
pub(super) struct StageVoiceAcoustics {
    layer: Option<AssetId>,
    soften_gain: f32,
    emitter: Option<Entity>,
    crowd: bool,
    controls: Arc<VoiceControls>,
}

impl StageVoiceAcoustics {
    pub(super) fn new(layer: &str, soften_gain: f32, emitter: Option<Entity>, crowd: bool) -> Self {
        Self {
            layer: layer_key(layer),
            soften_gain,
            emitter,
            crowd,
            controls: Arc::new(VoiceControls::default()),
        }
    }
}

#[derive(Component)]
pub(super) struct PendingStageVoice(pub(super) Handle<AudioSource>);

#[derive(Default)]
struct VoiceControls {
    room: AtomicU32,
    dry: AtomicU32,
    wet: AtomicU32,
    cutoff: AtomicU32,
}

#[derive(Resource)]
pub(super) struct StageVoiceCompletion {
    sender: Sender<Entity>,
    finished: Mutex<Receiver<Entity>>,
}

impl Default for StageVoiceCompletion {
    fn default() -> Self {
        let (sender, finished) = mpsc::channel();
        Self {
            sender,
            finished: Mutex::new(finished),
        }
    }
}

pub(super) fn release_finished_dry_voices(
    mut commands: Commands,
    mut lifecycle: ResMut<StageVoiceCompletion>,
) {
    let Ok(finished) = lifecycle.bypass_change_detection().finished.get_mut() else {
        return;
    };
    for entity in finished.try_iter() {
        commands
            .entity(entity)
            .try_remove::<super::audio_player_spawning::CueVoiceOf>();
    }
}

pub(super) fn publish_stage_voice_controls(
    environment: Option<
        Single<(
            Ref<ListenerAudioEnvironment>,
            Option<Ref<StageAmbienceCrossfade>>,
        )>,
    >,
    mut removed: RemovedComponents<ListenerAudioEnvironment>,
    mut finished_fades: RemovedComponents<StageAmbienceCrossfade>,
    voices: Query<Ref<StageVoiceAcoustics>>,
    added: Query<Entity, Added<StageVoiceAcoustics>>,
    moved_emitters: Query<&Children, Changed<GlobalTransform>>,
    poses: Query<&GlobalTransform>,
    rooms: ListenerRoomStages,
) {
    let environment_changed = (removed.read().count() != 0)
        | (finished_fades.read().count() != 0)
        | environment.as_ref().is_some_and(|environment| {
            let (stage, fade) = &**environment;
            stage.is_changed()
                || fade.as_ref().is_some_and(Ref::is_changed)
                || stage.room.is_some_and(|room| rooms.room_changed(room))
        });
    let (stage, room, occlude_crowd, outside_gain) = environment.map_or(
        (StageAcoustics::default(), None, false, 0.0),
        |environment| {
            let (stage, fade) = environment.into_inner();
            (
                stage.acoustics,
                stage.room,
                stage.occlude_crowd,
                fade.as_deref()
                    .map_or(0.0, StageAmbienceCrossfade::outside_voice_gain),
            )
        },
    );
    let publish = |voice: Ref<StageVoiceAcoustics>| {
        let spatial = voice.emitter.is_some();
        let allowed = !(voice.crowd && occlude_crowd)
            && (!spatial || voice.layer.is_none() || voice.layer == stage.layer);
        let within_room = room.zip(voice.emitter).is_none_or(|(room, emitter)| {
            poses
                .get(emitter)
                .is_ok_and(|pose| rooms.contains_source(room, pose.translation()))
        });
        let soften = if stage.indoor && spatial && !within_room {
            outside_gain
        } else if stage.indoor && spatial && voice.soften_gain > 0.0 {
            voice.soften_gain
        } else {
            1.0
        };
        let obstruction = if spatial {
            stage.obstruction.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let exclusion = if spatial {
            stage.exclusion.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let dry = if allowed {
            soften * (1.0 - obstruction)
        } else {
            0.0
        };
        let wet = if allowed {
            soften * stage.room_effect.max(0.0) * (1.0 - exclusion)
        } else {
            0.0
        };
        let cutoff = 1.0 - stage.occlusion.clamp(0.0, 1.0);
        for (parameter, value) in [
            (&voice.controls.dry, dry),
            (&voice.controls.wet, wet),
            (&voice.controls.cutoff, cutoff),
        ] {
            parameter.store(value.to_bits(), Ordering::Relaxed);
        }
        voice
            .controls
            .room
            .store(u32::from(stage.room), Ordering::Relaxed);
    };
    if environment_changed {
        voices.iter().for_each(publish);
    } else {
        let mut affected = added.iter().collect::<bevy::ecs::entity::EntityHashSet>();
        if room.is_some() {
            affected.extend(moved_emitters.iter().flat_map(|children| children.iter()));
        }
        voices.iter_many(affected).matched().for_each(publish);
    }
}

pub(super) fn prepare_stage_voice_players(
    mut commands: Commands,
    mut pending: Query<
        (
            Entity,
            &PendingStageVoice,
            &StageVoiceAcoustics,
            &mut PlaybackSettings,
        ),
        Without<super::audio_player_spawning::DeferredAudioPlayback>,
    >,
    new_voices: Query<
        Entity,
        (
            Added<PendingStageVoice>,
            Without<super::audio_player_spawning::DeferredAudioPlayback>,
        ),
    >,
    mut started: RemovedComponents<super::audio_player_spawning::DeferredAudioPlayback>,
    sources: Res<Assets<AudioSource>>,
    mut source_events: MessageReader<AssetEvent<AudioSource>>,
    mut failures: MessageReader<AssetLoadFailedEvent<AudioSource>>,
    asset_server: Res<AssetServer>,
    mut filtered_sources: ResMut<Assets<StageVoiceSource>>,
    completion: Res<StageVoiceCompletion>,
) {
    let changed = source_events
        .read()
        .filter_map(|event| match *event {
            AssetEvent::Added { id }
            | AssetEvent::Modified { id }
            | AssetEvent::LoadedWithDependencies { id } => Some(id),
            _ => None,
        })
        .chain(failures.read().map(|event| event.id))
        .collect::<bevy::platform::collections::HashSet<_>>();
    let mut affected = new_voices
        .iter()
        .chain(started.read())
        .collect::<bevy::ecs::entity::EntityHashSet>();
    if !changed.is_empty() {
        affected.extend(pending.iter().filter_map(|(entity, request, ..)| {
            changed.contains(&request.0.id()).then_some(entity)
        }));
    }
    let mut voices = pending.iter_many_mut(affected).matched();
    while let Some((entity, request, acoustics, mut playback)) = voices.fetch_next() {
        if let Some(source) = sources.get(&request.0) {
            let looping = matches!(playback.mode, PlaybackMode::Loop);
            if looping {
                // A buffered repetition would keep the room parameters from the first pass.
                playback.mode = PlaybackMode::Once;
            }
            let filtered = filtered_sources.add(StageVoiceSource {
                source: source.clone(),
                controls: acoustics.controls.clone(),
                looping,
                owner: entity,
                finished: completion.sender.clone(),
            });
            commands
                .entity(entity)
                .try_insert(AudioPlayer(filtered))
                .try_remove::<PendingStageVoice>();
        } else if matches!(
            asset_server.load_state(request.0.id()),
            bevy::asset::LoadState::Failed(_)
        ) {
            commands.entity(entity).try_despawn();
        }
    }
}

#[derive(Asset, TypePath)]
pub(super) struct StageVoiceSource {
    source: AudioSource,
    controls: Arc<VoiceControls>,
    looping: bool,
    owner: Entity,
    finished: Sender<Entity>,
}

impl Decodable for StageVoiceSource {
    type Decoder = StageVoiceDecoder;

    fn decoder(&self) -> Self::Decoder {
        let source = self.source.decoder();
        let channels = usize::from(source.channels().get());
        let sample_rate = source.sample_rate().get() as f32;
        StageVoiceDecoder {
            source,
            loop_source: self.looping.then(|| self.source.clone()),
            controls: self.controls.clone(),
            filter: vec![FilterHistory::default(); channels],
            reverb: RoomReverb::new(sample_rate),
            channel: 0,
            until_refresh: 0,
            coefficients: [1.0, 0.0, 0.0],
            dry: 1.0,
            wet: 0.0,
            tail_samples: 0,
            finished: Some((self.owner, self.finished.clone())),
            source_ended: false,
        }
    }
}

#[derive(Clone, Default)]
struct FilterHistory {
    inputs: [f32; 2],
    outputs: [f32; 2],
}

impl FilterHistory {
    fn low_pass(&mut self, sample: f32, [input, first, second]: [f32; 3]) -> f32 {
        let output = if input == 1.0 {
            sample
        } else {
            input * (sample + 2.0 * self.inputs[0] + self.inputs[1])
                - first * self.outputs[0]
                - second * self.outputs[1]
        };
        self.inputs = [sample, self.inputs[0]];
        self.outputs = [output, self.outputs[0]];
        output
    }
}

fn low_pass_coefficients(radians: f32) -> [f32; 3] {
    if radians >= std::f32::consts::FRAC_PI_2 * 0.999 {
        return [1.0, 0.0, 0.0];
    }
    let k = radians.max(0.0001).tan().recip();
    let square = k * k;
    let damping = std::f32::consts::SQRT_2 * k;
    let scale = (1.0 + square + damping).recip();
    [
        scale,
        2.0 * (1.0 - square) * scale,
        (1.0 - damping + square) * scale,
    ]
}

const COMB_DELAYS: [f32; 4] = [0.0297, 0.0371, 0.0411, 0.0437];
const ALL_PASS_DELAYS: [f32; 2] = [0.005, 0.0017];

struct DelayLine {
    samples: Vec<f32>,
    cursor: usize,
}

impl DelayLine {
    fn new(seconds: f32, sample_rate: f32) -> Self {
        Self {
            samples: vec![0.0; ((seconds * sample_rate) as usize).max(1)],
            cursor: 0,
        }
    }

    fn replace(&mut self, input: f32, feedback: f32) -> f32 {
        let old = self.samples[self.cursor];
        self.samples[self.cursor] = input + feedback * old;
        self.cursor = (self.cursor + 1) % self.samples.len();
        old
    }

    fn clear(&mut self) {
        self.samples.fill(0.0);
        self.cursor = 0;
    }
}

struct RoomReverb {
    sample_rate: f32,
    room: Option<usize>,
    input: f32,
    output: [f32; 2],
    pre_delay: DelayLine,
    pre_delay_samples: usize,
    filter: FilterHistory,
    coefficients: [f32; 3],
    combs: [DelayLine; 4],
    comb_feedback: [f32; 4],
    all_pass: [DelayLine; 2],
    all_pass_feedback: [f32; 2],
}

impl RoomReverb {
    fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate,
            room: None,
            input: 0.0,
            output: [0.0; 2],
            pre_delay: DelayLine::new(0.3, sample_rate),
            pre_delay_samples: 0,
            filter: FilterHistory::default(),
            coefficients: [1.0, 0.0, 0.0],
            combs: COMB_DELAYS.map(|delay| DelayLine::new(delay, sample_rate)),
            comb_feedback: [0.0; 4],
            all_pass: ALL_PASS_DELAYS.map(|delay| DelayLine::new(delay, sample_rate)),
            all_pass_feedback: [
                0.001_f32.powf(ALL_PASS_DELAYS[0] / 0.096_853_5),
                0.001_f32.powf(ALL_PASS_DELAYS[1] / 0.032_924),
            ],
        }
    }

    fn select_room(&mut self, room: usize) -> f32 {
        let (_, gain, decay, damping, delay) = ROOMS[room];
        if self.room != Some(room) {
            self.room = Some(room);
            self.pre_delay_samples =
                ((self.sample_rate * delay) as usize).min(self.pre_delay.samples.len());
            self.coefficients = low_pass_coefficients(std::f32::consts::FRAC_PI_8 * damping);
            self.comb_feedback = COMB_DELAYS.map(|delay| 0.001_f32.powf(delay / decay));
        }
        gain
    }

    fn advance_frame(&mut self, input: f32) {
        let delay = &mut self.pre_delay;
        let read =
            (delay.cursor + delay.samples.len() - self.pre_delay_samples) % delay.samples.len();
        let delayed = delay.samples[read];
        delay.replace(input, 0.0);
        let filtered = self.filter.low_pass(delayed, self.coefficients);
        let reflections: f32 = self
            .combs
            .iter_mut()
            .zip(self.comb_feedback)
            .map(|(comb, feedback)| comb.replace(filtered, feedback))
            .sum();
        let [first, second] = &mut self.all_pass;
        let [first_feedback, second_feedback] = self.all_pass_feedback;
        let first_delay = first.replace(reflections, first_feedback);
        let diffused = first_delay - first_feedback * (reflections + first_feedback * first_delay);
        let second_delay = second.replace(diffused * 0.25, second_feedback);
        let second_feedback_sample = second_feedback * second_delay;
        self.output = [
            second_delay - second_feedback * (diffused * 0.25 + second_feedback_sample),
            second_delay - second_feedback * (diffused * 0.15 + second_feedback_sample),
        ];
    }

    fn clear(&mut self) {
        self.input = 0.0;
        self.output = [0.0; 2];
        self.filter = FilterHistory::default();
        self.pre_delay.clear();
        for line in self.combs.iter_mut().chain(&mut self.all_pass) {
            line.clear();
        }
    }
}

pub(super) struct StageVoiceDecoder {
    source: <AudioSource as Decodable>::Decoder,
    loop_source: Option<AudioSource>,
    controls: Arc<VoiceControls>,
    filter: Vec<FilterHistory>,
    reverb: RoomReverb,
    channel: usize,
    until_refresh: usize,
    coefficients: [f32; 3],
    dry: f32,
    wet: f32,
    tail_samples: usize,
    finished: Option<(Entity, Sender<Entity>)>,
    source_ended: bool,
}

impl StageVoiceDecoder {
    fn refresh_controls(&mut self) {
        let read = |value: &AtomicU32| f32::from_bits(value.load(Ordering::Relaxed));
        self.dry = read(&self.controls.dry);
        let room_gain = self
            .reverb
            .select_room(self.controls.room.load(Ordering::Relaxed) as usize);
        self.wet = read(&self.controls.wet) * room_gain;
        if !self.source_ended && self.wet > 0.0 {
            let (_, _, decay, _, delay) = ROOMS[self.reverb.room.unwrap_or_default()];
            self.tail_samples =
                ((decay + delay) * self.reverb.sample_rate) as usize * self.filter.len();
        }
        self.coefficients =
            low_pass_coefficients(std::f32::consts::FRAC_PI_2 * read(&self.controls.cutoff));
    }
}

impl Iterator for StageVoiceDecoder {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        if self.until_refresh == 0 {
            self.refresh_controls();
            self.until_refresh = 128 * self.filter.len();
        }
        self.until_refresh -= 1;
        let sample = match self.source.next() {
            Some(sample) => sample,
            None if self.loop_source.is_some() => {
                self.source = self.loop_source.as_ref()?.decoder();
                self.source.next()?
            }
            None => {
                self.source_ended = true;
                if let Some((owner, sender)) = self.finished.take() {
                    let _ = sender.send(owner);
                }
                self.tail_samples = self.tail_samples.checked_sub(1)?;
                0.0
            }
        };
        let direct = self.filter[self.channel].low_pass(sample, self.coefficients);
        let reflected = self.reverb.output[self.channel % 2];
        self.reverb.input += direct;
        self.channel += 1;
        if self.channel == self.filter.len() {
            self.channel = 0;
            let input = std::mem::take(&mut self.reverb.input);
            self.reverb
                .advance_frame(self.wet * input / self.filter.len() as f32);
        }
        Some(direct * self.dry + reflected)
    }
}

impl Source for StageVoiceDecoder {
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> ChannelCount {
        self.source.channels()
    }

    fn sample_rate(&self) -> SampleRate {
        self.source.sample_rate()
    }

    fn total_duration(&self) -> Option<Duration> {
        None
    }

    fn try_seek(&mut self, position: Duration) -> Result<(), SeekError> {
        self.source.try_seek(position)?;
        self.filter.fill(FilterHistory::default());
        self.reverb.clear();
        self.channel = 0;
        self.tail_samples = 0;
        self.source_ended = false;
        Ok(())
    }
}

// Room wet level, decay time, damping, and initial reflection delay in seconds.
const ROOMS: [(&str, f32, f32, f32, f32); 26] = [
    ("ENVIRONMENT_GENERIC", 0.0, 1.493, 0.5, 0.011),
    ("ENVIRONMENT_PADDEDCELL", 0.25, 0.1, 0.0, 0.002),
    ("ENVIRONMENT_ROOM", 0.417, 0.4, 0.666, 0.003),
    ("ENVIRONMENT_BATHROOM", 0.75, 1.499, 0.166, 0.011),
    ("ENVIRONMENT_LIVINGROOM", 0.208, 0.478, 0.0, 0.004),
    ("ENVIRONMENT_STONEROOM", 0.3, 2.309, 0.68, 0.017),
    ("ENVIRONMENT_AUDITORIUM", 0.403, 4.279, 0.5, 0.03),
    ("ENVIRONMENT_CONCERTHALL", 0.5, 3.961, 0.5, 0.029),
    ("ENVIRONMENT_CAVE", 0.5, 2.886, 1.304, 0.022),
    ("ENVIRONMENT_ARENA", 0.361, 7.284, 0.332, 0.03),
    ("ENVIRONMENT_HANGAR", 0.5, 10.0, 0.3, 0.03),
    ("ENVIRONMENT_CARPETEDHALLWAY", 0.153, 0.259, 0.55, 0.03),
    ("ENVIRONMENT_HALLWAY", 0.361, 1.493, 0.5, 0.011),
    ("ENVIRONMENT_STONECORRIDOR", 0.444, 2.697, 0.638, 0.02),
    ("ENVIRONMENT_ALLEY", 0.25, 1.752, 0.776, 0.011),
    ("ENVIRONMENT_FOREST", 0.111, 3.145, 0.472, 0.088),
    ("ENVIRONMENT_CITY", 0.111, 2.767, 0.224, 0.011),
    ("ENVIRONMENT_MOUNTAINS", 0.2, 0.8, 0.2, 0.3),
    ("ENVIRONMENT_QUARRY", 0.55, 3.0, 0.55, 0.043),
    ("ENVIRONMENT_PLAIN", 0.12, 0.8, 0.18, 0.1),
    ("ENVIRONMENT_PARKINGLOT", 0.208, 1.652, 1.5, 0.012),
    ("ENVIRONMENT_SEWERPIPE", 0.652, 2.886, 0.25, 0.021),
    ("ENVIRONMENT_UNDERWATER", 1.0, 1.499, 0.1, 0.011),
    ("ENVIRONMENT_DRUGGED", 0.875, 8.392, 1.388, 0.011),
    ("ENVIRONMENT_DIZZY", 0.139, 10.0, 1.5, 0.011),
    ("ENVIRONMENT_PSYCHOTIC", 0.486, 7.563, 4.0, 0.011),
];
