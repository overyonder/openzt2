//! Loads a Blue Fang audio document.

mod xml;

use std::path::Path;

use openzt2_game_data::{
    audio::{
        AudioAmbientPolicy, AudioCrowdPolicy, AudioCue, AudioDocument, AudioMixerPolicy,
        AudioSoundscapeAllowedEntities, AudioSoundscapeBiome, AudioSoundscapeContent,
        AudioSoundscapeCue, AudioStage, AudioUsage, AudioVariant, AudioWaterPolicy,
        STAGE_INDOOR_OCCLUSION, STAGE_KEEP_AMBIENCE, VARIANT_CUE_REFERENCE, VARIANT_KEEP_RESIDENT,
        VARIANT_LOOPED, VARIANT_RANDOM_PAN, VARIANT_RANDOM_PITCH, VARIANT_RANDOM_VOLUME,
        VARIANT_STREAMED,
    },
    AssetId,
};

use crate::assets::source_document::ordered_source_document_types::{
    OrderedSourceDocument, OrderedSourceDocumentNode, OrderedSourceDocumentSpan,
};

use self::xml::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct AudioDocumentDiagnostic {
    pub(super) path: String,
    pub(super) span: Option<OrderedSourceDocumentSpan>,
    pub(super) message: String,
}

pub(super) fn lower(
    document: &OrderedSourceDocument,
    resolve_clip: &impl Fn(&Path, &str) -> Option<String>,
) -> Result<AudioDocument, AudioDocumentDiagnostic> {
    let mut output = AudioDocument::default();
    (|| {
        match document.root.name.as_str() {
            "BFSoundMgr" => output.mixer = Some(lower_mixer(document)?),
            "BFSoundStageMgr" => output.stages = lower_stages(document)?,
            "UISoundMgr" | "soundtags" => output.cues = lower_cues(document, resolve_clip)?,
            "ambient" => output.soundscape_contents.push(lower_soundscape(document)),
            "ambientbiomes" => output.soundscape_biomes = lower_soundscape_biomes(document)?,
            "ambientsallowed" | "ambientallowed" => output
                .soundscape_allowed_entities
                .push(lower_allowed_entities(document)),
            "ZTAIAmbientsMgr" => output.ambient = Some(lower_ambient(document)?),
            "ZTWorldSndMgr" => output.water = Some(lower_water(document)?),
            "crowd" => output.crowd = Some(lower_crowd(document)?),
            "Root_Element" if !descendants(&document.root).any(is_factory) => {}
            _ if descendants(&document.root).any(is_factory) => {
                output.cues = lower_cues(document, resolve_clip)?;
            }
            root => {
                return Err(AudioDocumentDiagnostic {
                    path: document.path.as_str().to_owned(),
                    span: Some(document.root.span),
                    message: format!("unsupported audio document root {root}"),
                });
            }
        }
        Ok(())
    })()
    .map_err(|mut error: AudioDocumentDiagnostic| {
        error.path = document.path.as_str().to_owned();
        error
    })?;
    Ok(output)
}

fn lower_mixer(
    document: &OrderedSourceDocument,
) -> Result<AudioMixerPolicy, AudioDocumentDiagnostic> {
    let mut policy = AudioMixerPolicy {
        output_sample_rate: 44_100,
        output_channels: 2,
        voice_limit: 32,
        spatial_voice_limit: 8,
        spatial_in_range: 2_500.0,
        spatial_out_range: 3_000.0,
        max_world_distance: 45.0,
    };
    let Some(options) = descendants(&document.root).find(|node| node.name == "SoundOptions") else {
        return Ok(policy);
    };
    policy.output_sample_rate = number(options, "SampleRate")?.unwrap_or(policy.output_sample_rate);
    policy.output_channels = number(options, "Channels")?.unwrap_or(policy.output_channels);
    policy.voice_limit = number(options, "MixerChannels")?.unwrap_or(policy.voice_limit);
    policy.spatial_voice_limit = number(options, "Mixer3DChannels")?
        .or(number(options, "maxNumOf3DWorldSnds")?)
        .unwrap_or(policy.spatial_voice_limit);
    policy.spatial_in_range = number(options, "inRangeDist")?.unwrap_or(policy.spatial_in_range);
    policy.spatial_out_range = number(options, "outRangeDist")?.unwrap_or(policy.spatial_out_range);
    policy.max_world_distance =
        number(options, "maxDistToHear3DWorldSnds")?.unwrap_or(policy.max_world_distance);
    Ok(policy)
}

fn lower_ambient(
    document: &OrderedSourceDocument,
) -> Result<AudioAmbientPolicy, AudioDocumentDiagnostic> {
    let mut policy = AudioAmbientPolicy {
        voice_limit: 10,
        delay_seconds: [4.0, 6.0],
        probability: 1.0,
    };
    policy.voice_limit = root_number(document, "MaxAmbients")?.unwrap_or(policy.voice_limit);
    policy.delay_seconds = ordered(
        root_number(document, "MinDelaySecs")?.unwrap_or(policy.delay_seconds[0]),
        root_number(document, "MaxDelaySecs")?.unwrap_or(policy.delay_seconds[1]),
    );
    policy.probability = root_number(document, "Probability")?.unwrap_or(policy.probability);
    Ok(policy)
}

fn lower_water(
    document: &OrderedSourceDocument,
) -> Result<AudioWaterPolicy, AudioDocumentDiagnostic> {
    let mut policy = AudioWaterPolicy::default();
    if let Some(sizes) = descendants(&document.root).find(|node| node.name == "waterEnvSizes") {
        policy.area_thresholds = ordered(
            number(sizes, "maxAreaSmallWaterSize")?.unwrap_or(0.0),
            number(sizes, "maxAreaMediumWaterSize")?.unwrap_or(0.0),
        );
        policy.natural_stages = water_stage_ids(sizes, "natural");
        policy.tank_stages = water_stage_ids(sizes, "tanks");
    }
    if let Some(general) = descendants(&document.root).find(|node| node.name == "generalData") {
        policy.environment_transition_seconds = number(general, "envTransInSec")?.unwrap_or(0.0);
        policy.lapping_max_distance = number(general, "kMaxDistToHearLappingWater")?.unwrap_or(0.0);
        policy.lapping_gain_scale = number(general, "kLappingSoundInterpScalar")?.unwrap_or(0.0);
        policy.lapping_minimum_area =
            number(general, "kSmallestWaterAreaForLapping")?.unwrap_or(0.0);
    }
    Ok(policy)
}

fn lower_crowd(
    document: &OrderedSourceDocument,
) -> Result<AudioCrowdPolicy, AudioDocumentDiagnostic> {
    let mut policy = AudioCrowdPolicy::default();
    for (slot, name) in policy.thresholds.iter_mut().zip([
        "kQuietCrowdHighTrigger",
        "kSmallCrowdLowTrigger",
        "kSmallCrowdHighTrigger",
        "kMediumCrowdLowTrigger",
        "kMediumCrowdHighTrigger",
        "kLargeCrowdLowTrigger",
        "kLargeCrowdHighTrigger",
        "kHugeCrowdLowTrigger",
    ]) {
        *slot = root_number(document, name)?.unwrap_or(0.0);
    }
    if let Some(cues) = document.root.attribute("crowdSounds") {
        policy
            .cues
            .iter_mut()
            .zip(cues.split_whitespace())
            .for_each(|(slot, cue)| *slot = Some(cue_id(cue)));
    }
    Ok(policy)
}

fn lower_stages(
    document: &OrderedSourceDocument,
) -> Result<Vec<AudioStage>, AudioDocumentDiagnostic> {
    document
        .root
        .element_children()
        .filter(|node| node.name == "SoundStage")
        .map(|stage| {
            let name = required_attr(document, stage, "name")?.to_owned();
            let data = descendants_node(stage).find(|node| node.name == "data");
            let flags = data.map_or(Ok(0), |data| {
                Ok(
                    (u8::from(bool_attr(data, "useIndoorOcclusion")?) * STAGE_INDOOR_OCCLUSION)
                        | (u8::from(bool_attr(data, "dontChangeAmbientSounds")?)
                            * STAGE_KEEP_AMBIENCE),
                )
            })?;
            Ok(AudioStage {
                id: stage_id(&name),
                name,
                room_type: data
                    .and_then(|node| attr(node, "roomType"))
                    .unwrap_or("")
                    .to_owned(),
                filter_layer: data
                    .and_then(|node| attr(node, "soundFilterLayer"))
                    .unwrap_or("")
                    .to_owned(),
                room_effect: optional_number(data, "roomEffectLevel")?.unwrap_or(0.0),
                occlusion: optional_number(data, "globalOcclusionLevel")?.unwrap_or(0.0),
                exclusion: optional_number(data, "globalExclusionLevel")?.unwrap_or(0.0),
                obstruction: optional_number(data, "globalObstructionLevel")?.unwrap_or(0.0),
                loop_cue: data.and_then(|node| attr(node, "ambientSound")).map(cue_id),
                ambient_cues: descendants_node(stage)
                    .filter(|node| node.name == "snd")
                    .filter_map(|node| attr(node, "name"))
                    .map(cue_id)
                    .collect(),
                flags,
            })
        })
        .collect()
}

fn lower_cues(
    document: &OrderedSourceDocument,
    resolve_clip: &impl Fn(&Path, &str) -> Option<String>,
) -> Result<Vec<AudioCue>, AudioDocumentDiagnostic> {
    descendants(&document.root)
        .filter(|node| is_factory(node))
        .filter_map(|factory| match lower_cue(document, factory, resolve_clip) {
            Ok(Some(cue)) => Some(Ok(cue)),
            Ok(None) => None,
            Err(error) => Some(Err(error)),
        })
        .collect()
}

fn lower_cue(
    document: &OrderedSourceDocument,
    factory: &OrderedSourceDocumentNode,
    resolve_clip: &impl Fn(&Path, &str) -> Option<String>,
) -> Result<Option<AudioCue>, AudioDocumentDiagnostic> {
    let name = required_attr(document, factory, "name")?;
    let variants = descendants_node(factory)
        .filter(|node| node.name == "snd")
        .filter_map(
            |variant| match lower_variant(document, factory, variant, resolve_clip) {
                Ok(Some(variant)) => Some(Ok(variant)),
                Ok(None) => None,
                Err(error) => Some(Err(error)),
            },
        )
        .collect::<Result<Vec<_>, _>>()?;
    if variants.is_empty() {
        return Ok(None);
    }
    Ok(Some(AudioCue {
        id: cue_id(name),
        name: name.to_owned(),
        usage: usage_for(
            document.path.as_str(),
            factory.name.as_str(),
            name,
            &variants,
        ),
        variants,
    }))
}

fn lower_variant(
    document: &OrderedSourceDocument,
    factory: &OrderedSourceDocumentNode,
    variant: &OrderedSourceDocumentNode,
    resolve_clip: &impl Fn(&Path, &str) -> Option<String>,
) -> Result<Option<AudioVariant>, AudioDocumentDiagnostic> {
    let source = required_attr(document, variant, "name")?;
    let cue_reference = bool_attr(variant, "soundtag")?
        || matches!(
            factory.name.as_str(),
            "BFAmbient2DSndFactory" | "BFAmbient3DSndFactory"
        );
    let source = if cue_reference {
        source.to_owned()
    } else {
        let owner = Path::new(document.path.as_str());
        let Some(resolved) = resolve_clip(owner, source).or_else(|| {
            crate::assets::original_game_asset_typo_fixups::corrected_audio_clip_path(source)
                .and_then(|corrected| resolve_clip(owner, corrected))
        }) else {
            bevy::log::warn!(
                document = document.path.as_str(),
                cue = factory.attribute("name").unwrap_or_default(),
                clip = source,
                "Audio clip is unavailable; skipping this variant"
            );
            return Ok(None);
        };
        resolved
    };
    let volume = inherited_number::<f32>(variant, factory, "volume")?.unwrap_or(1.0);
    let pitch = inherited_number::<f32>(variant, factory, "pitch")?.unwrap_or(1.0);
    let delay = inherited_number::<f32>(variant, factory, "delay")?.unwrap_or(0.0);
    let min_pitch = inherited_number::<f32>(variant, factory, "minpitch")?.unwrap_or(pitch);
    let max_pitch = inherited_number::<f32>(variant, factory, "maxpitch")?.unwrap_or(pitch);
    let mut flags = 0;
    set_flag(
        &mut flags,
        VARIANT_LOOPED,
        inherited_bool(variant, factory, "looping")?,
    );
    set_flag(
        &mut flags,
        VARIANT_STREAMED,
        inherited_bool(variant, factory, "streamed")?
            || inherited_bool(variant, factory, "streaming")?,
    );
    set_flag(
        &mut flags,
        VARIANT_RANDOM_VOLUME,
        inherited_bool(variant, factory, "randvolume")?,
    );
    set_flag(&mut flags, VARIANT_RANDOM_PITCH, min_pitch != max_pitch);
    set_flag(
        &mut flags,
        VARIANT_RANDOM_PAN,
        inherited_bool(variant, factory, "randpan")?,
    );
    set_flag(
        &mut flags,
        VARIANT_KEEP_RESIDENT,
        bool_attr(variant, "alwaysKeepSoundHandle")?,
    );
    set_flag(&mut flags, VARIANT_CUE_REFERENCE, cue_reference);
    let min_distance = number::<f32>(variant, "mindist")?;
    let max_distance = number::<f32>(variant, "maxdist")?;
    Ok(Some(AudioVariant {
        source_id: if cue_reference {
            cue_id(&source)
        } else {
            AssetId::from_virtual_path(&source)
        },
        source,
        weight: number(variant, "weight")?.unwrap_or(1.0),
        gain: ordered(
            inherited_number(variant, factory, "minvolume")?.unwrap_or(volume),
            inherited_number(variant, factory, "maxvolume")?.unwrap_or(volume),
        ),
        pitch: ordered(min_pitch, max_pitch),
        delay_seconds: ordered(
            inherited_number(variant, factory, "mindelay")?.unwrap_or(delay),
            inherited_number(variant, factory, "maxdelay")?.unwrap_or(delay),
        ),
        distance: min_distance.or(max_distance).map(|fallback| {
            ordered(
                min_distance.unwrap_or(fallback),
                max_distance.unwrap_or(fallback),
            )
        }),
        soften_gain: number(variant, "softenGain")?.unwrap_or(1.0),
        filter_layer: attr(variant, "layer")
            .or_else(|| attr(factory, "layer"))
            .unwrap_or("")
            .to_owned(),
        flags,
    }))
}

fn lower_soundscape(document: &OrderedSourceDocument) -> AudioSoundscapeContent {
    let name = soundscape_name(document.path.as_str());
    AudioSoundscapeContent {
        biome: AssetId::from_key(&name),
        name,
        loop_cue: document.root.attribute("loop").map(cue_id),
        cues: descendants(&document.root)
            .filter(|node| node.name == "snd")
            .filter_map(|node| attr(node, "name"))
            .map(|cue| AudioSoundscapeCue {
                cue: cue_id(cue),
                weight: 1.0,
                day_mask: 0b11,
            })
            .collect(),
    }
}

fn lower_allowed_entities(document: &OrderedSourceDocument) -> AudioSoundscapeAllowedEntities {
    let name = parent_directory_name(document.path.as_str()).unwrap_or_else(|| "default".into());
    AudioSoundscapeAllowedEntities {
        biome: AssetId::from_key(&name),
        name,
        allowed_entities: document
            .root
            .element_children()
            .filter(|node| node.name == "a")
            .filter_map(|node| attr(node, "name"))
            .map(AssetId::from_key)
            .collect(),
    }
}

fn lower_soundscape_biomes(
    document: &OrderedSourceDocument,
) -> Result<Vec<AudioSoundscapeBiome>, AudioDocumentDiagnostic> {
    document
        .root
        .element_children()
        .filter(|node| node.name == "b")
        .map(|node| {
            let name = required_attr(document, node, "name")?.to_owned();
            Ok(AudioSoundscapeBiome {
                biome: AssetId::from_key(&name),
                name,
                directory: required_attr(document, node, "dir")?.replace('\\', "/"),
            })
        })
        .collect()
}

fn usage_for(path: &str, factory: &str, cue: &str, variants: &[AudioVariant]) -> AudioUsage {
    let path = canonical_path(path);
    let cue = canonical_token(cue);
    if path.starts_with("ui/") && !cue.contains("music") && !cue.ends_with("_loop") {
        AudioUsage::Ui
    } else if path.contains("/guest_") || path.ends_with("/mc.xml") || cue.contains("narrat") {
        AudioUsage::Voice
    } else if factory.starts_with("BF3DSndFactory")
        && !variants
            .iter()
            .any(|variant| variant.flags & VARIANT_STREAMED != 0)
    {
        AudioUsage::Effect
    } else if cue.contains("music") || cue.contains("theme") {
        AudioUsage::Music
    } else if factory.starts_with("BFAmbient")
        || cue.starts_with("ambient")
        || cue.starts_with("a_")
        || cue.ends_with("_loop")
        || variants
            .iter()
            .any(|variant| variant.flags & VARIANT_STREAMED != 0)
    {
        AudioUsage::Ambient
    } else {
        AudioUsage::Effect
    }
}
