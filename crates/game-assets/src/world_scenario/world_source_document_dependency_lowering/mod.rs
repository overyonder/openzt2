use super::{
    authored,
    world_scenario_document_asset_and_dependency_handles::DemandLoadedAssetPath,
    world_scenario_document_source::WorldScenarioDocumentSource,
    world_scenario_source_reference_resolution::{
        resolve_world_scenario_source_references, unresolved_reference, SourceReference,
        SourceReferenceKind,
    },
};
use crate::{
    asset_source::AssetArchives,
    source_document::{
        blue_fang_actor_manifest_model_and_scene_resolution_index::BlueFangActorManifestModelAndSceneResolutionIndex,
        blue_fang_source_dependency_reference_discovery::collect_blue_fang_actor_manifest_source_references,
        blue_fang_source_document_parsing::parse_blue_fang_source_document,
        ordered_source_document_types::OrderedSourceDocument, path::AssetPath,
    },
};
use bevy::asset::LoadContext;
use openzt2_game_data::world_scenario::WorldScenarioDocument;
use std::{io, path::Path};

pub(super) async fn lower_world_source_document_and_resolve_dependencies(
    source: &OrderedSourceDocument,
    archives: &AssetArchives,
    context: &mut LoadContext<'_>,
) -> io::Result<(WorldScenarioDocument, Vec<SourceReference>)> {
    let path = source.path.as_str();
    let documents = std::slice::from_ref(source);
    let mut actors = BlueFangActorManifestModelAndSceneResolutionIndex::default();
    for manifest in collect_blue_fang_actor_manifest_source_references(documents) {
        let resolved = archives
            .resolve_source_reference(Path::new(path), &manifest)
            .ok_or_else(|| unresolved_reference(path, "actor manifest", &manifest))?;
        let manifest_path = resolved.to_string_lossy().replace('\\', "/");
        let bytes = context
            .read_asset_bytes(resolved)
            .await
            .map_err(io::Error::other)?;
        let manifest = parse_blue_fang_source_document(AssetPath::new(&manifest_path), &bytes)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        actors.insert_parsed_blue_fang_actor_model_manifest(&manifest);
    }
    let starting_zoo = matches!(
        source.root.name.rsplit(':').next(),
        Some("BFContext" | "saveRoot")
    ) && path.to_ascii_lowercase().ends_with(".zt2");
    let timing = if starting_zoo {
        let timing_path = archives
            .resolve_source_reference(Path::new(""), "config/timekeeper.xml")
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "config/timekeeper.xml"))?;
        Some(context.load_builder().load_value::<crate::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset>(timing_path).await.map_err(io::Error::other)?)
    } else {
        None
    };
    let timing = timing
        .as_ref()
        .map(|asset| {
            asset.get().simulation_timing().ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "timekeeper has no simulation timing",
                )
            })
        })
        .transpose()?;
    let references =
        resolve_world_scenario_source_references(source, documents, &actors, archives)?;
    let document = authored::lower_documents(
        &[source],
        &references,
        actors.actor_scene_asset_paths_by_authored_actor_key(),
        timing,
    )?;
    Ok((document, references))
}

pub(crate) async fn lower_saved_world_starting_document_with_scene_dependencies(
    source: &OrderedSourceDocument,
    archives: &AssetArchives,
    context: &mut LoadContext<'_>,
) -> io::Result<WorldScenarioDocumentSource> {
    let (document, references) =
        lower_world_source_document_and_resolve_dependencies(source, archives, context).await?;
    let scenes = references
        .iter()
        .filter(|reference| {
            reference.kind == SourceReferenceKind::Scene
                && document.starting_zoos.iter().any(|start| {
                    start
                        .entities
                        .iter()
                        .any(|entity| entity.prefab == reference.id)
                })
        })
        .map(|reference| DemandLoadedAssetPath {
            id: reference.id,
            path: reference.path.clone().into_boxed_str(),
        })
        .collect::<Vec<_>>();
    let mut scenes = scenes;
    scenes.sort_unstable_by_key(|dependency| dependency.id);
    scenes.dedup_by_key(|dependency| dependency.id);
    Ok(WorldScenarioDocumentSource {
        document,
        starting_zoos: Box::new([]),
        terrains: Box::new([]),
        scenes: scenes.into_boxed_slice(),
        textures: Box::new([]),
    })
}
