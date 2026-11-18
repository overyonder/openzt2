use std::collections::BTreeMap;

use bevy::{audio::AudioSource, prelude::*};
use openzt2_game_data::{
    AssetId,
    audio::{
        AudioAmbientPolicy, AudioCrowdPolicy, AudioCue, AudioDocument, AudioMixerPolicy,
        AudioSoundscapeAllowedEntities, AudioSoundscapeContent, AudioStage, AudioWaterPolicy,
    },
};

#[derive(Asset, TypePath, Debug)]
pub struct AudioAsset {
    pub(super) document: AudioDocument,
    pub(super) clip_paths: Box<[(AssetId, Box<str>)]>,
}

impl AudioAsset {
    pub fn from_world_preset(
        soundscape: AudioSoundscapeContent,
        crowd: Option<AudioCrowdPolicy>,
    ) -> Self {
        Self {
            document: AudioDocument {
                soundscape_contents: vec![soundscape],
                crowd,
                ..Default::default()
            },
            clip_paths: Box::new([]),
        }
    }

    pub fn document(&self) -> &AudioDocument {
        &self.document
    }

    pub fn soundscape_content(&self) -> Option<&AudioSoundscapeContent> {
        self.document.soundscape_contents.first()
    }

    pub fn crowd_policy(&self) -> Option<&AudioCrowdPolicy> {
        self.document.crowd.as_ref()
    }

    fn clip(&self, server: &AssetServer, id: AssetId) -> Option<Handle<AudioSource>> {
        self.clip_paths
            .binary_search_by_key(&id, |(candidate, _)| *candidate)
            .ok()
            .map(|index| server.load(self.clip_paths[index].1.to_string()))
    }
}

#[derive(Clone, Copy)]
pub struct AudioView<'a> {
    index: &'a AudioAssets,
    assets: &'a Assets<AudioAsset>,
}

#[derive(Clone, Copy)]
pub struct ResolvedAudioCue<'a> {
    pub cue: &'a AudioCue,
    asset: &'a AudioAsset,
}

impl ResolvedAudioCue<'_> {
    pub fn clip(&self, server: &AssetServer, id: AssetId) -> Option<Handle<AudioSource>> {
        self.asset.clip(server, id)
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) enum AudioRowKey {
    Cue(AssetId),
    Stage(AssetId),
    Soundscape(AssetId),
    AllowedEntities(AssetId),
    Mixer,
    Ambient,
    Water,
    Crowd,
}

pub(super) struct AudioCatalogueSource {
    pub(super) handle: Handle<AudioAsset>,
    pub(super) rank: (usize, usize),
    pub(super) rows: BTreeMap<AudioRowKey, usize>,
    pub(super) biomes: BTreeMap<String, AssetId>,
}

#[derive(Resource, Default)]
pub struct AudioAssets {
    pub(super) discovered: bool,
    pub(super) sources: BTreeMap<std::path::PathBuf, AudioCatalogueSource>,
    pub(super) pending: std::collections::BTreeSet<bevy::asset::AssetId<AudioAsset>>,
    pub(super) affected_keys: std::collections::BTreeSet<AudioRowKey>,
    pub(super) biome_directories: BTreeMap<String, AssetId>,
    pub(super) winners: BTreeMap<AudioRowKey, (Handle<AudioAsset>, usize)>,
}

impl AudioAssets {
    pub fn get<'a>(&'a self, assets: &'a Assets<AudioAsset>) -> Option<AudioView<'a>> {
        (self.discovered && self.pending.is_empty()).then_some(AudioView {
            index: self,
            assets,
        })
    }
}

impl<'a> AudioView<'a> {
    fn row(self, key: AudioRowKey) -> Option<(&'a AudioAsset, usize)> {
        let (handle, row) = self.index.winners.get(&key)?;
        Some((self.assets.get(handle)?, *row))
    }

    pub fn cue(self, id: AssetId) -> Option<ResolvedAudioCue<'a>> {
        let (asset, index) = self.row(AudioRowKey::Cue(id))?;
        let cue = asset.document.cues.get(index).filter(|cue| cue.id == id)?;
        Some(ResolvedAudioCue { cue, asset })
    }

    pub fn stage(self, id: AssetId) -> Option<&'a AudioStage> {
        let (asset, index) = self.row(AudioRowKey::Stage(id))?;
        asset
            .document
            .stages
            .get(index)
            .filter(|stage| stage.id == id)
    }

    pub fn soundscape(self, biome: AssetId) -> Option<&'a AudioSoundscapeContent> {
        let (asset, index) = self
            .row(AudioRowKey::Soundscape(biome))
            .or_else(|| self.row(AudioRowKey::Soundscape(AssetId::from_key("ambient"))))?;
        asset.document.soundscape_contents.get(index)
    }

    pub fn soundscape_allowed_entities(
        self,
        biome: AssetId,
    ) -> Option<&'a AudioSoundscapeAllowedEntities> {
        let (asset, index) = self.row(AudioRowKey::AllowedEntities(biome))?;
        asset
            .document
            .soundscape_allowed_entities
            .get(index)
            .filter(|allowed| allowed.biome == biome)
    }

    pub fn mixer(self) -> Option<&'a AudioMixerPolicy> {
        self.row(AudioRowKey::Mixer)?.0.document.mixer.as_ref()
    }

    pub fn ambient(self) -> Option<&'a AudioAmbientPolicy> {
        self.row(AudioRowKey::Ambient)?.0.document.ambient.as_ref()
    }

    pub fn water(self) -> Option<&'a AudioWaterPolicy> {
        self.row(AudioRowKey::Water)?.0.document.water.as_ref()
    }

    pub fn crowd(self) -> Option<&'a AudioCrowdPolicy> {
        self.row(AudioRowKey::Crowd)?.0.document.crowd.as_ref()
    }
}
