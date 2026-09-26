//! Live lowering of the authored world-definition document closure.

use std::io;

use crate::assets::source_document::{
    blue_fang_actor_manifest_model_and_scene_resolution_index::BlueFangActorManifestModelAndSceneResolutionIndex,
    document_semantics::{classify_source_document, SourceDocumentKind},
    ordered_source_document_types::{OrderedSourceDocument, OrderedSourceDocumentNode},
};
use openzt2_game_data::world_definitions::document::WorldDefinitionDocument;

use super::{
    behavior_auxiliary_source_document_lowering::lower_behavior_auxiliary_source_document, lower,
};

#[cfg(test)]
mod tests;

pub(super) fn lower_resolved_world_definition_source_document_closure_to_canonical_document(
    source_documents: &[OrderedSourceDocument],
    actor_manifest_resolution_index: &BlueFangActorManifestModelAndSceneResolutionIndex,
    primary_path: &str,
    authored_type_registry_source_order: [u64; 2],
) -> io::Result<Option<WorldDefinitionDocument>> {
    if source_documents.is_empty() {
        return Ok(None);
    }
    let primary_path = primary_path.to_ascii_lowercase();
    let primary_is_abstract_binder = source_documents.iter().any(|document| {
        document.path.key() == primary_path
            && document.root.name.eq_ignore_ascii_case("BFTypedBinder")
            && document.root.attributes.iter().any(|attribute| {
                attribute.name().eq_ignore_ascii_case("abstract")
                    && matches!(
                        attribute.value().to_ascii_lowercase().as_str(),
                        "true" | "1"
                    )
            })
    });
    let actor_scene_paths =
        actor_manifest_resolution_index.actor_scene_asset_paths_by_authored_actor_key();
    // Dependencies supply inherited values and reference targets. Only the
    // primary document owns declarations in this Bevy asset.
    let auxiliary_document = source_documents
        .iter()
        .filter(|document| document.path.key() == primary_path)
        .find(|document| {
            is_auxiliary(document.root.name.as_str())
                || document
                    .root
                    .name
                    .eq_ignore_ascii_case("ZTFossilFindingMode")
                || document
                    .root
                    .element_children()
                    .any(|child| contains(child, "ZTFossilFindingMode"))
        });
    let fossil_mode_document = source_documents
        .iter()
        .find(|document| document.path.key() == "ui/modes/modes.xml");
    let auxiliary = lower_behavior_auxiliary_source_document(
        auxiliary_document,
        fossil_mode_document,
        |path| Some(openzt2_game_data::AssetId::from_virtual_path(path)),
    )
    .map_err(|diagnostics| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            diagnostics
                .into_iter()
                .map(|diagnostic| format!("{}: {:?}", diagnostic.virtual_path, diagnostic.kind))
                .collect::<Vec<_>>()
                .join("; "),
        )
    })?;
    let documents = source_documents
        .iter()
        .filter(|document| {
            (document.path.key() == primary_path
                && (document.root.name.eq_ignore_ascii_case("ZTPhotoMode")
                    || document
                        .root
                        .element_children()
                        .any(|child| contains(child, "ZTPhotoMode"))))
                || (document.path.key() == primary_path
                    && primary_path.starts_with("locations/")
                    && primary_path.ends_with("initiallocs.xml"))
                || matches!(
                    classify_source_document(document),
                    SourceDocumentKind::SpeciesOrEntity
                        | SourceDocumentKind::WorldDefinitions
                        | SourceDocumentKind::Manager
                        | SourceDocumentKind::Catalogue
                )
        })
        .cloned()
        .collect::<Vec<_>>();
    lower::bind_resolved_world_definition_source_records_to_canonical_document(
        &documents,
        &primary_path,
        Some(&auxiliary),
        actor_scene_paths,
        actor_scene_paths,
        authored_type_registry_source_order,
    )
    .map(|document| {
        // Abstract binders supply inherited data without requiring a standalone definition.
        (document != WorldDefinitionDocument::default() || primary_is_abstract_binder)
            .then_some(document)
    })
    .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.to_string()))
}

fn is_auxiliary(root: &str) -> bool {
    matches!(
        root.to_ascii_lowercase().as_str(),
        "bfparticledictionary"
            | "particledictionary"
            | "ztgesturemgr"
            | "ztpuzzlemgr"
            | "gestures"
            | "tricks"
    )
}

fn contains(node: &OrderedSourceDocumentNode, wanted: &str) -> bool {
    node.name.eq_ignore_ascii_case(wanted)
        || node.element_children().any(|child| contains(child, wanted))
}
