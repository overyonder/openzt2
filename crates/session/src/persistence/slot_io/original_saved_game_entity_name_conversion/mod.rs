use std::io;

use openzt2_game_assets::source_document::{
    ordered_source_document_types::OrderedSourceDocumentNode,
    source_document_semantic_name::canonicalize_source_document_record_key,
};
use openzt2_game_data::AssetId;
use openzt2_zoo_state::entity_naming_types::EntityNameTagCounters;

pub(super) fn read_original_saved_entity_name_counters(
    status: &OrderedSourceDocumentNode,
) -> io::Result<EntityNameTagCounters> {
    let containers = status
        .element_children()
        .filter(|node| node.name == "entityIndex")
        .collect::<Vec<_>>();
    let [container] = containers.as_slice() else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "saved Status requires one entityIndex",
        ));
    };
    let entries = container
        .element_children()
        .map(|node| {
            let raw = node.attribute("i").ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "saved entityIndex entry has no i",
                )
            })?;
            let count = raw
                .parse::<u32>()
                .or_else(|_| {
                    raw.parse::<i32>()
                        .map(|value| u32::from_ne_bytes(value.to_ne_bytes()))
                })
                .map_err(|_| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        "saved entityIndex entry is not a 32-bit integer",
                    )
                })?;
            Ok((
                AssetId::from_key(&canonicalize_source_document_record_key(&node.name)),
                count,
            ))
        })
        .collect::<io::Result<Vec<_>>>()?;
    EntityNameTagCounters::from_saved_entries(entries).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "saved entityIndex repeats a canonical tag",
        )
    })
}
