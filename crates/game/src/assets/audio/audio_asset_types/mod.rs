use std::collections::BTreeMap;

use bevy::{audio::AudioSource, prelude::*};
use openzt2_game_data::{
    audio::{
        AudioAmbientPolicy, AudioCue, AudioDocument, AudioMixerPolicy,
        AudioSoundscapeAllowedEntities, AudioSoundscapeContent, AudioStage, AudioWaterPolicy,
    },
    AssetId,
};

#[derive(Asset, TypePath, Debug)]
pub(crate) struct AudioAsset {
    pub(super) document: AudioDocument,
    pub(super) clip_paths: Box<[(AssetId, Box<str>)]>,
}

impl AudioAsset {
    fn clip(&self, server: &AssetServer, id: AssetId) -> Option<Handle<AudioSource>> {
        self.clip_paths
            .binary_search_by_key(&id, |(candidate, _)| *candidate)
            .ok()
            .map(|index| server.load(self.clip_paths[index].1.to_string()))
    }
}

#[derive(Resource, Default)]
pub(crate) struct AudioAssets {
    pub(super) revision: Option<u64>,
    pub(super) dirty: bool,
    pub(super) handles: Vec<Handle<AudioAsset>>,
    pub(super) cues: BTreeMap<AssetId, Handle<AudioAsset>>,
    pub(super) stages: BTreeMap<AssetId, Handle<AudioAsset>>,
    pub(super) soundscapes: BTreeMap<AssetId, (Handle<AudioAsset>, usize)>,
    pub(super) allowed_entities: BTreeMap<AssetId, Handle<AudioAsset>>,
    pub(super) mixer: Option<Handle<AudioAsset>>,
    pub(super) ambient: Option<Handle<AudioAsset>>,
    pub(super) water: Option<Handle<AudioAsset>>,
}

impl AudioAssets {
    pub(crate) fn get<'a>(&'a self, assets: &'a Assets<AudioAsset>) -> Option<AudioView<'a>> {
        (!self.dirty).then_some(AudioView {
            index: self,
            assets,
        })
    }
}

#[derive(Clone, Copy)]
pub(crate) struct AudioView<'a> {
    index: &'a AudioAssets,
    assets: &'a Assets<AudioAsset>,
}

#[derive(Clone, Copy)]
pub(crate) struct ResolvedAudioCue<'a> {
    pub(crate) cue: &'a AudioCue,
    asset: &'a AudioAsset,
}

impl ResolvedAudioCue<'_> {
    pub(crate) fn clip(&self, server: &AssetServer, id: AssetId) -> Option<Handle<AudioSource>> {
        self.asset.clip(server, id)
    }
}

impl<'a> AudioView<'a> {
    pub(crate) fn cue(self, id: AssetId) -> Option<ResolvedAudioCue<'a>> {
        let asset = self.assets.get(self.index.cues.get(&id)?)?;
        asset
            .document
            .cues
            .iter()
            .find(|cue| cue.id == id)
            .map(|cue| ResolvedAudioCue { cue, asset })
    }

    pub(crate) fn stage(self, id: AssetId) -> Option<&'a AudioStage> {
        self.assets
            .get(self.index.stages.get(&id)?)?
            .document
            .stages
            .iter()
            .find(|stage| stage.id == id)
    }

    pub(crate) fn soundscape(self, biome: AssetId) -> Option<&'a AudioSoundscapeContent> {
        let selected = self.index.soundscapes.get(&biome).or_else(|| {
            let ambient = AssetId::from_key("ambient");
            self.index.soundscapes.get(&ambient)
        })?;
        self.assets
            .get(&selected.0)?
            .document
            .soundscape_contents
            .get(selected.1)
    }

    pub(crate) fn soundscape_allowed_entities(
        self,
        biome: AssetId,
    ) -> Option<&'a AudioSoundscapeAllowedEntities> {
        self.assets
            .get(self.index.allowed_entities.get(&biome)?)?
            .document
            .soundscape_allowed_entities
            .iter()
            .find(|allowed| allowed.biome == biome)
    }

    pub(crate) fn mixer(self) -> Option<&'a AudioMixerPolicy> {
        self.index
            .mixer
            .as_ref()
            .and_then(|handle| self.assets.get(handle))
            .and_then(|asset| asset.document.mixer.as_ref())
    }

    pub(crate) fn ambient(self) -> Option<&'a AudioAmbientPolicy> {
        self.index
            .ambient
            .as_ref()
            .and_then(|handle| self.assets.get(handle))
            .and_then(|asset| asset.document.ambient.as_ref())
    }

    pub(crate) fn water(self) -> Option<&'a AudioWaterPolicy> {
        self.index
            .water
            .as_ref()
            .and_then(|handle| self.assets.get(handle))
            .and_then(|asset| asset.document.water.as_ref())
    }
}
