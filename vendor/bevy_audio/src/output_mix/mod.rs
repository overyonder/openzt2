//! Optional processing of the audio output after playback gain and spatialization.

use std::time::Duration;

use bevy_app::App;
use bevy_ecs::component::Component;
use rodio::{ChannelCount, Sample, SampleRate, Source, mixer::MixerSource};

use crate::audio_output::AudioOutput;

/// Routes a player to the selected input of the output processor.
/// Players without this component use input zero.
#[derive(Component, Default)]
pub struct AudioMixInput(pub usize);

/// Installs an output processor and configures its input mixers for the audio device.
///
/// Call once after adding the audio plugin and before creating playback sinks.
/// The processor receives one output frame and one frame from each input mixer.
/// The device invokes it on the audio thread.
pub fn install_audio_output_mix<const INPUTS: usize, P>(
    app: &mut App,
    build_processor: impl FnOnce(ChannelCount, SampleRate) -> P,
) where
    P: FnMut(&[Box<[Sample]>; INPUTS], &mut [Sample]) + Send + 'static,
{
    assert!(INPUTS > 0, "an audio output needs at least one input");
    let mut audio_output = app.world_mut().resource_mut::<AudioOutput>();
    let Some(stream) = audio_output.stream.as_ref() else {
        return;
    };
    assert!(
        audio_output.mix_inputs.is_empty(),
        "the audio output processor is already installed"
    );
    let channels = stream.config().channel_count();
    let sample_rate = stream.config().sample_rate();
    let mut mixers = Vec::with_capacity(INPUTS);
    let sources: [MixerSource; INPUTS] = std::array::from_fn(|_| {
        let (input, output) = rodio::mixer::mixer(channels, sample_rate);
        mixers.push(input);
        output
    });
    let frame = || vec![0.0; usize::from(channels.get())].into_boxed_slice();
    stream.mixer().add(ProcessedOutputMix {
        channels,
        sample_rate,
        sources,
        input: std::array::from_fn(|_| frame()),
        output: frame(),
        index: 0,
        processor: build_processor(channels, sample_rate),
    });
    audio_output.mix_inputs = mixers;
}

struct ProcessedOutputMix<P, const INPUTS: usize> {
    channels: ChannelCount,
    sample_rate: SampleRate,
    sources: [MixerSource; INPUTS],
    input: [Box<[Sample]>; INPUTS],
    output: Box<[Sample]>,
    index: usize,
    processor: P,
}

impl<P, const INPUTS: usize> Iterator for ProcessedOutputMix<P, INPUTS>
where
    P: FnMut(&[Box<[Sample]>; INPUTS], &mut [Sample]),
{
    type Item = Sample;

    fn next(&mut self) -> Option<Sample> {
        if self.index == 0 {
            for (source, frame) in self.sources.iter_mut().zip(&mut self.input) {
                for sample in &mut **frame {
                    *sample = source.next().unwrap_or(0.0);
                }
            }
            (self.processor)(&self.input, &mut self.output);
        }
        let sample = self.output[self.index];
        self.index = (self.index + 1) % self.output.len();
        Some(sample)
    }
}

impl<P, const INPUTS: usize> Source for ProcessedOutputMix<P, INPUTS>
where
    P: FnMut(&[Box<[Sample]>; INPUTS], &mut [Sample]),
{
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> ChannelCount {
        self.channels
    }

    fn sample_rate(&self) -> SampleRate {
        self.sample_rate
    }

    fn total_duration(&self) -> Option<Duration> {
        None
    }
}
