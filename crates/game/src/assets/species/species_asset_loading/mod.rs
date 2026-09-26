use std::{io, path::Path};

use bevy::{
    asset::{io::Reader, AssetLoader, LoadContext},
    prelude::*,
};

use crate::{
    asset_source::AssetArchives,
    assets::{
        source_document::{
            blue_fang_actor_manifest_model_and_scene_resolution_index::BlueFangActorManifestModelAndSceneResolutionIndex,
            blue_fang_source_dependency_reference_discovery::collect_blue_fang_actor_manifest_source_references,
            blue_fang_source_document_parsing::parse_blue_fang_source_document,
            ordered_source_document_collection_precedence::reorder_ordered_source_documents_by_requested_normalized_path_keys_and_last_definition_precedence,
            ordered_source_document_types::OrderedSourceDocument, path::AssetPath,
        },
        species::source::{
            collect_authored_species_type_source_references,
            lower_resolved_species_source_document_closure_to_canonical_document,
        },
    },
};

use super::species_asset_types::{SpeciesAnimationSetDependencyPath, SpeciesAsset};

#[derive(TypePath)]
pub(super) struct SpeciesAssetLoader {
    archives: AssetArchives,
}

impl FromWorld for SpeciesAssetLoader {
    fn from_world(world: &mut World) -> Self {
        Self {
            archives: world.resource::<AssetArchives>().clone(),
        }
    }
}

impl AssetLoader for SpeciesAssetLoader {
    type Asset = SpeciesAsset;
    type Settings = ();
    type Error = io::Error;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _: &(),
        context: &mut LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        let path = context.path().path().to_string_lossy().replace('\\', "/");
        let mut documents = Vec::<OrderedSourceDocument>::new();
        let mut actor_manifest_resolution_index =
            BlueFangActorManifestModelAndSceneResolutionIndex::default();
        let primary = parse_blue_fang_source_document(AssetPath::new(&path), &bytes)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        let primary_path = primary.path.key();
        let mut type_references = collect_authored_species_type_source_references(&primary)
            .into_iter()
            .map(|reference| (primary.path.key(), reference))
            .collect::<std::collections::BTreeSet<_>>();
        documents.push(primary);
        let mut loaded = std::collections::BTreeSet::from([primary_path.clone()]);
        while let Some((from, reference)) = type_references.pop_first() {
            let Some(dependency) = self
                .archives
                .resolve_source_reference(Path::new(&from), &reference)
            else {
                // Type vocabulary also contains ordinary symbolic bases such
                // as `entity`; only values that resolve to authored source
                // documents form loader dependencies.
                continue;
            };
            let dependency_path = dependency.to_string_lossy().replace('\\', "/");
            if !loaded.insert(dependency_path.clone()) {
                continue;
            }
            let dependency_bytes = context
                .read_asset_bytes(dependency)
                .await
                .map_err(io::Error::other)?;
            let dependency = parse_blue_fang_source_document(
                AssetPath::new(&dependency_path),
                &dependency_bytes,
            )
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
            type_references.extend(
                collect_authored_species_type_source_references(&dependency)
                    .into_iter()
                    .map(|reference| (dependency.path.key(), reference)),
            );
            documents.push(dependency);
        }
        let mut order = loaded
            .into_iter()
            .filter(|loaded| loaded != &primary_path)
            .collect::<Vec<_>>();
        order.push(primary_path.clone());
        reorder_ordered_source_documents_by_requested_normalized_path_keys_and_last_definition_precedence(
            &mut documents,
            &order,
        );

        let mut manifests = collect_blue_fang_actor_manifest_source_references(&documents);
        while let Some(manifest) = manifests.pop_first() {
            let resolved = self
                .archives
                .resolve_source_reference(Path::new(&primary_path), &manifest)
                .ok_or_else(|| unresolved_reference(&primary_path, "actor manifest", &manifest))?;
            let manifest_path = resolved.to_string_lossy().replace('\\', "/");
            let bytes = context
                .read_asset_bytes(resolved)
                .await
                .map_err(io::Error::other)?;
            let actor_manifest =
                parse_blue_fang_source_document(AssetPath::new(&manifest_path), &bytes)
                    .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
            actor_manifest_resolution_index
                .insert_parsed_blue_fang_actor_model_manifest(&actor_manifest);
        }

        let document = lower_resolved_species_source_document_closure_to_canonical_document(
            &documents,
            &actor_manifest_resolution_index,
            &primary_path,
        )
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        let mut animation_sets = actor_manifest_resolution_index
            .animation_set_asset_paths_by_actor_manifest_key()
            .iter()
            .map(
                |(authored_path, resolved_path)| SpeciesAnimationSetDependencyPath {
                    id: openzt2_game_data::AssetId::from_virtual_path(authored_path),
                    path: resolved_path.clone().into_boxed_str(),
                },
            )
            .collect::<Vec<_>>();
        animation_sets.sort_unstable_by_key(|dependency| dependency.id);
        animation_sets.dedup_by_key(|dependency| dependency.id);
        Ok(SpeciesAsset {
            document,
            animation_sets: animation_sets.into_boxed_slice(),
        })
    }

    fn extensions(&self) -> &[&str] {
        &["xml"]
    }
}

fn unresolved_reference(from: &str, kind: &str, reference: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::NotFound,
        format!("{from} references missing {kind} {reference}"),
    )
}
