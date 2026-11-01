use std::io;

use openzt2_game_data::{
    world_scenario::{WorldMapRecord, WorldMapSupportedGameModeFlags},
    AssetId,
};

use super::{
    starting_zoo::{at, local, required},
    world_scenario_source_reference_resolution::{SourceReference, SourceReferenceKind},
};
use crate::source_document::ordered_source_document_types::{
    OrderedSourceDocument, OrderedSourceDocumentNode,
};

pub(super) fn lower_map_index(
    document: &OrderedSourceDocument,
    dependencies: &[SourceReference],
) -> io::Result<Vec<WorldMapRecord>> {
    let modes = if document
        .path
        .key()
        .to_ascii_lowercase()
        .contains("maps/scenario/")
    {
        WorldMapSupportedGameModeFlags::CAMPAIGN
    } else {
        WorldMapSupportedGameModeFlags::FREEFORM
            .with_additional_flags(WorldMapSupportedGameModeFlags::CHALLENGE)
    };
    document
        .root
        .element_children()
        .enumerate()
        .filter(|(_, wrapper)| {
            let id = local(&wrapper.name);
            find_reference(dependencies, id, SourceReferenceKind::Terrain).is_some()
                && find_reference(dependencies, id, SourceReferenceKind::Texture).is_some()
        })
        .map(|(order, wrapper)| {
            let mut children = wrapper.element_children();
            let data = children
                .next()
                .ok_or_else(|| at(document, wrapper, "map wrapper has no ZTMapData"))?;
            if local(&data.name) != "ZTMapData" || children.next().is_some() {
                return Err(at(
                    document,
                    wrapper,
                    "map wrapper must contain exactly one ZTMapData",
                ));
            }
            let id = local(&wrapper.name);
            let terrain = find_reference(dependencies, id, SourceReferenceKind::Terrain)
                .expect("filtered map has a terrain asset");
            let thumbnail = find_reference(dependencies, id, SourceReferenceKind::Texture)
                .expect("filtered map has a thumbnail asset");
            lower_world_map_row(
                document,
                data,
                AssetId::from_key(id),
                AssetId::from_key(&format!("start:{id}")),
                terrain.id,
                thumbnail.id,
                u32::try_from(order).map_err(|_| at(document, data, "map order exceeds u32"))?,
                modes,
                None,
            )
        })
        .collect()
}

#[allow(
    clippy::too_many_arguments,
    reason = "map rows receive distinct record and dependency identities"
)]
pub(super) fn lower_world_map_row(
    document: &OrderedSourceDocument,
    data: &OrderedSourceDocumentNode,
    map: AssetId,
    starting_zoo: AssetId,
    terrain: AssetId,
    thumbnail: AssetId,
    catalogue_order: u32,
    supported_game_modes: WorldMapSupportedGameModeFlags,
    environment_path: Option<&str>,
) -> io::Result<WorldMapRecord> {
    let name = required(document, data, "name")?;
    let size = required(document, data, "size")?;
    let biome = required(document, data, "biome")?.to_ascii_lowercase();
    let location_name = required(document, data, "location")?;
    let expansion_pack_filter_identifier = data
        .attribute("xpack")
        .unwrap_or("0")
        .parse::<u16>()
        .map_err(|_| {
            at(
                document,
                data,
                "map xpack must be an unsigned 16-bit integer",
            )
        })?;
    let suffix = match size.to_ascii_lowercase().as_str() {
        "mapsize:large" => "",
        "mapsize:medium" => "_medium",
        "mapsize:small" => "_small",
        _ => return Err(at(document, data, format!("unknown map size {size}"))),
    };
    let environment_biome = if biome == "tropicalrainforest" {
        "rainforest"
    } else {
        &biome
    };
    let default_environment = format!("world/environments/env1{environment_biome}{suffix}.xml");
    Ok(WorldMapRecord {
        id: map,
        catalogue_order,
        name_key: AssetId::from_key(name),
        location: AssetId::from_key(location_name),
        biome: AssetId::from_key(&biome),
        biome_key: AssetId::from_key(&format!("{biome}:icon_stt")),
        size_key: AssetId::from_key(size),
        description_key: data
            .attribute("protected")
            .filter(|value| !value.is_empty())
            .map(AssetId::from_key),
        thumbnail,
        environment: AssetId::from_virtual_path(environment_path.unwrap_or(&default_environment)),
        camera: AssetId::from_virtual_path("world/cameras/overheadcam.xml"),
        terrain,
        starting_zoo,
        expansion_pack_filter_identifier,
        supported_game_modes,
    })
}

fn find_reference<'a>(
    dependencies: &'a [SourceReference],
    id: &str,
    kind: SourceReferenceKind,
) -> Option<&'a SourceReference> {
    dependencies
        .iter()
        .filter(|asset| asset.kind == kind)
        .find(|asset| {
            std::path::Path::new(&asset.path)
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| {
                    name.eq_ignore_ascii_case(id)
                        || name
                            .to_ascii_lowercase()
                            .starts_with(&format!("{}.", id.to_ascii_lowercase()))
                })
        })
}
