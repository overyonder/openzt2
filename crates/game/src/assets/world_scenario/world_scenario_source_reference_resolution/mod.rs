//! Resolution of scenario source references to winning live asset paths.

use std::{io, path::Path};

use openzt2_game_data::AssetId;

use crate::{
    asset_source::AssetArchives,
    assets::source_document::{
        blue_fang_actor_manifest_model_and_scene_resolution_index::BlueFangActorManifestModelAndSceneResolutionIndex,
        blue_fang_source_dependency_reference_discovery::{
            collect_blue_fang_native_model_source_references,
            collect_blue_fang_static_asset_source_references,
        },
        ordered_source_document_types::OrderedSourceDocument,
    },
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SourceReferenceKind {
    Terrain,
    Scene,
    Texture,
}

#[derive(Clone, Debug)]
pub(crate) struct SourceReference {
    pub(super) id: AssetId,
    pub(super) kind: SourceReferenceKind,
    pub(super) path: String,
}

pub(super) fn resolve_world_scenario_source_references(
    source: &OrderedSourceDocument,
    source_documents: &[OrderedSourceDocument],
    actor_manifest_resolution_index: &BlueFangActorManifestModelAndSceneResolutionIndex,
    archives: &AssetArchives,
) -> io::Result<Vec<SourceReference>> {
    let from = Path::new(source.path.as_str());
    let default_map_index = source.path.key().eq_ignore_ascii_case("maps/index.xml");
    let generated = source
        .root
        .element_children()
        .flat_map(|node| {
            let id = node.name.rsplit(':').next().unwrap_or(&node.name);
            let id = if default_map_index {
                format!("default/{id}")
            } else {
                id.to_owned()
            };
            [
                (format!("{id}.dat"), SourceReferenceKind::Terrain),
                (format!("{id}.jpg"), SourceReferenceKind::Texture),
            ]
        })
        .filter_map(|(reference, kind)| {
            let path = archives.resolve_source_reference(from, &reference)?;
            let path = path.to_string_lossy().replace('\\', "/");
            Some(SourceReference {
                id: AssetId::from_virtual_path(&path),
                kind,
                path,
            })
        })
        .collect::<Vec<_>>();
    let mut references = generated;
    let models = collect_blue_fang_native_model_source_references(
        source_documents,
        actor_manifest_resolution_index,
    );
    for (declaring_source, model, prefer_blue_fang_bfb) in models {
        let model_from = Path::new(&declaring_source);
        let resolved = archives
            .resolve_model_reference_with_blue_fang_bfb_preference(
                model_from,
                &model,
                prefer_blue_fang_bfb,
            )
            .or_else(|| {
                (!model.ends_with(".nif") && !model.ends_with(".bfb"))
                    .then(|| [format!("{model}.nif"), format!("{model}.bfb")])
                    .into_iter()
                    .flatten()
                    .find_map(|candidate| archives.resolve_model_reference(model_from, &candidate))
            });
        let resolved =
            resolved.ok_or_else(|| unresolved_reference(&declaring_source, "model", &model))?;
        let path =
            crate::assets::model_source::native_model_source_lowering::
                native_model_scene_labelled_asset_path(&resolved.to_string_lossy());
        references.push(SourceReference {
            id: AssetId::from_virtual_path(&path),
            kind: SourceReferenceKind::Scene,
            path,
        });
    }
    for model in
        actor_manifest_resolution_index.native_model_source_paths_in_actor_manifest_key_order()
    {
        let resolved = archives.resolve_model_reference(from, model).or_else(|| {
            (!model.ends_with(".nif") && !model.ends_with(".bfb"))
                .then(|| [format!("{model}.nif"), format!("{model}.bfb")])
                .into_iter()
                .flatten()
                .find_map(|candidate| archives.resolve_model_reference(from, &candidate))
        });
        let resolved = resolved
            .ok_or_else(|| unresolved_reference(&from.to_string_lossy(), "actor model", model))?;
        let path =
            crate::assets::model_source::native_model_source_lowering::
                native_model_scene_labelled_asset_path(&resolved.to_string_lossy());
        references.push(SourceReference {
            id: AssetId::from_virtual_path(&path),
            kind: SourceReferenceKind::Scene,
            path,
        });
    }
    for (declaring_source, reference) in
        collect_blue_fang_static_asset_source_references(source_documents)
    {
        let Some(resolved) =
            archives.resolve_source_reference(Path::new(&declaring_source), &reference)
        else {
            // Generic static presentation references include stale optional
            // entries. Required scenario/model/script dependencies are
            // resolved through their typed paths above and remain strict.
            continue;
        };
        let extension = resolved
            .extension()
            .and_then(|value| value.to_str())
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("resolved asset {reference} has no extension"),
                )
            })?;
        let (path, kind) = if extension.eq_ignore_ascii_case("nif")
            || extension.eq_ignore_ascii_case("bfb")
        {
            (
                    crate::assets::model_source::native_model_source_lowering::
                        native_model_scene_labelled_asset_path(&resolved.to_string_lossy()),
                    SourceReferenceKind::Scene,
                )
        } else if ["dds", "png", "jpg", "jpeg", "bmp", "tga"]
            .iter()
            .any(|candidate| extension.eq_ignore_ascii_case(candidate))
        {
            let source = resolved.to_string_lossy().replace('\\', "/");
            (
                    crate::assets::texture::source_image_asset_path_selection::select_bevy_image_asset_path_for_blue_fang_source_image(&source),
                    SourceReferenceKind::Texture,
                )
        } else {
            continue;
        };
        let id_path = path.strip_suffix("#Image").unwrap_or(&path);
        references.push(SourceReference {
            id: AssetId::from_virtual_path(id_path),
            kind,
            path,
        });
    }
    references.sort_unstable_by(|left, right| left.path.cmp(&right.path));
    references.dedup_by(|left, right| left.kind == right.kind && left.id == right.id);
    Ok(references)
}

pub(super) fn unresolved_reference(from: &str, kind: &str, reference: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::NotFound,
        format!("{from} references missing {kind} {reference}"),
    )
}
