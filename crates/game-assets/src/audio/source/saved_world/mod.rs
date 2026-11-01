use std::io;

use crate::source_document::ordered_source_document_types::{
    OrderedSourceDocument, OrderedSourceDocumentNode,
};
use openzt2_game_data::{
    audio::{AudioCrowdPolicy, AudioDocument},
    AssetId,
};

pub fn lower_saved_world_audio_preset(
    document: &OrderedSourceDocument,
    manager: &OrderedSourceDocumentNode,
    biome: AssetId,
    biome_name: &str,
    mut crowd: AudioCrowdPolicy,
) -> io::Result<AudioDocument> {
    for (name, _) in manager.attributes() {
        if !matches!(
            name,
            "version"
                | "ambientSound"
                | "kQuietCrowdHighTrigger"
                | "kSmallCrowdLowTrigger"
                | "kSmallCrowdHighTrigger"
                | "kMediumCrowdLowTrigger"
                | "kMediumCrowdHighTrigger"
                | "kLargeCrowdLowTrigger"
                | "kLargeCrowdHighTrigger"
                | "kHugeCrowdLowTrigger"
        ) {
            return Err(invalid(format!(
                "saved world sound manager {name} requires preset-file conversion"
            )));
        }
    }
    let mut soundscape = super::lower_soundscape(&OrderedSourceDocument {
        path: document.path.clone(),
        format: document.format,
        root: manager.clone(),
    });
    soundscape.biome = biome;
    soundscape.name = biome_name.to_owned();
    let ambient = manager
        .attribute("ambientSound")
        .ok_or_else(|| invalid("saved world sound manager has no ambientSound"))?;
    soundscape.loop_cue = (!ambient.is_empty()).then(|| super::xml::cue_id(ambient));
    for (slot, key) in crowd.thresholds.iter_mut().zip([
        "kQuietCrowdHighTrigger",
        "kSmallCrowdLowTrigger",
        "kSmallCrowdHighTrigger",
        "kMediumCrowdLowTrigger",
        "kMediumCrowdHighTrigger",
        "kLargeCrowdLowTrigger",
        "kLargeCrowdHighTrigger",
        "kHugeCrowdLowTrigger",
    ]) {
        *slot = manager
            .attribute(key)
            .ok_or_else(|| invalid(format!("saved world sound manager has no {key}")))?
            .parse()
            .map_err(|_| invalid(format!("saved world sound manager {key} is not i32")))?;
    }
    Ok(AudioDocument {
        soundscape_contents: vec![soundscape],
        crowd: Some(crowd),
        ..Default::default()
    })
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}
