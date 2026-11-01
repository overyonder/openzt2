use std::io;

use openzt2_game_data::{
    world_scenario::{WorldMapSupportedGameModeFlags, WorldScenarioDocument},
    AssetId,
};

use super::{
    starting_zoo::{at, descendants, local, required},
    world_map_row_lowering::lower_world_map_row,
    world_scenario_document_asset_and_dependency_handles::DemandLoadedAssetPath,
    world_scenario_document_source::WorldScenarioDocumentSource,
};
use crate::source_document::ordered_source_document_types::{
    OrderedSourceDocument, OrderedSourceDocumentNode,
};

/// Lowers a saved world's own map metadata and explicit local asset dependencies.
pub fn lower_saved_world_map_metadata(
    document: &OrderedSourceDocument,
    source_asset_id: AssetId,
    terrain_path: &str,
    thumbnail_path: Option<&str>,
) -> io::Result<WorldScenarioDocumentSource> {
    if !matches!(local(&document.root.name), "BFContext" | "saveRoot") {
        return Err(at(
            document,
            &document.root,
            "saved map has no BFContext or saveRoot",
        ));
    }
    let roots = document.root.element_children().collect::<Vec<_>>();
    let data = unique_descendant(document, &roots, "ZTMapData")?;
    let world = unique_descendant(document, &roots, "ZTWorldMgr")?;
    let environment = required(document, world, "environment")?;
    if environment.is_empty() {
        return Err(at(document, world, "saved ZTWorldMgr environment is empty"));
    }
    let terrain = dependency_path(document, terrain_path)?;
    let thumbnail = thumbnail_path
        .map(|path| dependency_path(document, path))
        .transpose()?;
    let identity = source_asset_id.to_lowercase_hexadecimal_string();
    let map = lower_world_map_row(
        document,
        data,
        AssetId::from_key(&format!("saved-map:{identity}")),
        AssetId::from_key(&format!("saved-start:{identity}")),
        terrain.id,
        thumbnail
            .as_ref()
            .map_or_else(AssetId::default, |asset| asset.id),
        0,
        saved_world_mode_flags(document)?,
        Some(environment),
    )?;
    Ok(WorldScenarioDocumentSource {
        document: WorldScenarioDocument {
            maps: vec![map],
            ..Default::default()
        },
        starting_zoos: Box::default(),
        terrains: vec![terrain].into_boxed_slice(),
        scenes: Box::default(),
        textures: thumbnail.into_iter().collect::<Vec<_>>().into_boxed_slice(),
    })
}

/// Reads the saved world's authored mode selection.
pub fn saved_world_mode_flags(
    document: &OrderedSourceDocument,
) -> io::Result<WorldMapSupportedGameModeFlags> {
    let roots = document.root.element_children().collect::<Vec<_>>();
    let context = match local(&document.root.name) {
        "BFContext" => &document.root,
        "saveRoot" => unique_descendant(document, &roots, "BFContext")?,
        _ => {
            return Err(at(
                document,
                &document.root,
                "saved map has no BFContext or saveRoot",
            ));
        }
    };
    let template = required(document, context, "templateFile")?;
    let path = crate::source_document::path::AssetPath::new(template).key();
    match path.as_str() {
        "config/freeform_mode.xml" => Ok(WorldMapSupportedGameModeFlags::FREEFORM),
        "config/challenge_mode.xml" => Ok(WorldMapSupportedGameModeFlags::CHALLENGE),
        "config/campaign_mode.xml" => Ok(WorldMapSupportedGameModeFlags::CAMPAIGN),
        _ => Err(at(
            document,
            context,
            format!("unsupported saved world mode template {template}"),
        )),
    }
}

fn unique_descendant<'a>(
    document: &OrderedSourceDocument,
    roots: &'a [&'a OrderedSourceDocumentNode],
    name: &str,
) -> io::Result<&'a OrderedSourceDocumentNode> {
    let mut matching = descendants(roots).filter(|node| local(&node.name) == name);
    let node = matching
        .next()
        .ok_or_else(|| at(document, &document.root, format!("saved map has no {name}")))?;
    if matching.next().is_some() {
        return Err(at(
            document,
            &document.root,
            format!("saved map has multiple {name} nodes"),
        ));
    }
    Ok(node)
}

fn dependency_path(
    document: &OrderedSourceDocument,
    path: &str,
) -> io::Result<DemandLoadedAssetPath> {
    let parsed = bevy::asset::AssetPath::try_parse(path)
        .map_err(|error| at(document, &document.root, error))?;
    if parsed.path().as_os_str().is_empty() {
        return Err(at(
            document,
            &document.root,
            "saved map dependency path is empty",
        ));
    }
    Ok(DemandLoadedAssetPath {
        id: AssetId::from_virtual_path(path),
        path: path.into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        source_document::{
            blue_fang_source_document_parsing::parse_blue_fang_source_document, path::AssetPath,
        },
        world_scenario::world_scenario_document_source_encoding::{
            decode_world_scenario_document_source, encode_world_scenario_document_source,
        },
    };

    #[test]
    fn saved_map_identity_and_dependencies_survive_source_encoding() -> io::Result<()> {
        let document = parse_blue_fang_source_document(AssetPath::new("saved/world.zt2"), br#"
            <saveRoot><BFContext templateFile="config/challenge_mode.xml">
              <ZTMapData name="mapname:grassland" size="mapsize:large" biome="grassland" location="sandiego" protected="" xpack="2"/>
              <ZTWorldMgr environment="world/environments/saved_environment.xml"/>
            </BFContext></saveRoot>"#).map_err(io::Error::other)?;
        let terrain = "original-saves://first/saved/terrain.dat";
        let thumbnail = "original-saves://first/saved/thumb.jpg";
        let first = lower_saved_world_map_metadata(
            &document,
            AssetId::from_key("first"),
            terrain,
            Some(thumbnail),
        )?;
        let second =
            lower_saved_world_map_metadata(&document, AssetId::from_key("second"), terrain, None)?;
        let decoded =
            decode_world_scenario_document_source(&encode_world_scenario_document_source(&first)?)?;
        assert_eq!(decoded.document.maps, first.document.maps);
        let map = decoded.document.maps[0];
        assert_ne!(map.id, second.document.maps[0].id);
        assert_ne!(map.starting_zoo, second.document.maps[0].starting_zoo);
        assert_ne!(map.id, AssetId::from_key("grassland"));
        assert_eq!(
            map.environment,
            AssetId::from_virtual_path("world/environments/saved_environment.xml")
        );
        assert_eq!(
            map.supported_game_modes,
            WorldMapSupportedGameModeFlags::CHALLENGE
        );
        assert_eq!(decoded.terrains[0].id, map.terrain);
        assert_eq!(&*decoded.terrains[0].path, terrain);
        assert_eq!(decoded.textures[0].id, map.thumbnail);
        assert_eq!(&*decoded.textures[0].path, thumbnail);
        assert_eq!(second.document.maps[0].thumbnail, AssetId::default());
        assert!(second.textures.is_empty());
        assert!(decoded.document.starting_zoos.is_empty());
        Ok(())
    }
}
