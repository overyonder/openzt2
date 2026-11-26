//! Live lowering of the authored world-definition document closure.

use std::io;

use crate::source_document::{
    blue_fang_actor_manifest_model_and_scene_resolution_index::BlueFangActorManifestModelAndSceneResolutionIndex,
    document_semantics::{SourceDocumentKind, classify_source_document},
    ordered_source_document_types::{OrderedSourceDocument, OrderedSourceDocumentNode},
};
use openzt2_game_data::world_definitions::document::WorldDefinitionDocument;

use super::{
    behavior_auxiliary_source_document_lowering::lower_behavior_auxiliary_source_document, lower,
};

#[cfg(test)]
mod tests;

#[derive(Default)]
pub(super) struct WorldDefinitionSourceMetadata {
    pub(super) runtime: super::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAssetMetadata,
    pub(super) tank_material_paths: std::collections::BTreeSet<String>,
}

pub(super) fn lower_resolved_world_definition_source_document_closure_to_canonical_document(
    source_documents: &[OrderedSourceDocument],
    actor_manifest_resolution_index: &BlueFangActorManifestModelAndSceneResolutionIndex,
    primary_path: &str,
    authored_type_registry_source_order: [u64; 2],
    metadata: &mut WorldDefinitionSourceMetadata,
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
    let gameplay_modes = source_documents
        .iter()
        .find(|document| document.path.key() == "ui/modes/modes.xml");
    let auxiliary =
        lower_behavior_auxiliary_source_document(auxiliary_document, gameplay_modes, |path| {
            Some(openzt2_game_data::AssetId::from_virtual_path(path))
        })
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
    .and_then(|(mut document, runtime, tank_material_paths)| {
        *metadata = WorldDefinitionSourceMetadata {
            runtime,
            tank_material_paths,
        };
        if let Some(modes) = gameplay_modes.filter(|_| primary_path == "ui/modes/modes.xml") {
            document.transport_highlight = lower_transport_highlight_policy(modes)?;
        }
        // Abstract binders supply inherited data without requiring a standalone definition.
        Ok(
            (document != WorldDefinitionDocument::default() || primary_is_abstract_binder)
                .then_some(document),
        )
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

fn lower_transport_highlight_policy(
    document: &OrderedSourceDocument,
) -> Result<
    Option<
        openzt2_game_data::world_definitions::transportation_and_tours::TransportHighlightPolicy,
    >,
    crate::source_document::resolved_source_record_index::BindError,
> {
    use openzt2_game_data::world_definitions::transportation_and_tours::{
        TrackHighlightPolicy, TransportDeletionHighlightPolicy, TransportHighlightPolicy,
    };
    let invalid = |node: &OrderedSourceDocumentNode, field: &str| {
        crate::source_document::resolved_source_record_index::BindError {
            virtual_path: document.path.key(),
            span: node.span,
            message: format!("{} has malformed {field}", node.name),
        }
    };
    let interval = |node: &OrderedSourceDocumentNode| {
        node.attribute_named_any(&["colorAnimationCycle"])
            .and_then(|value| value.trim_end_matches(['f', 'F']).parse::<f32>().ok())
            .filter(|value| value.is_finite() && *value > 0.0)
            .ok_or_else(|| invalid(node, "colorAnimationCycle"))
    };
    let color = |owner: &OrderedSourceDocumentNode,
                 name: &str|
     -> Result<
        [u8; 4],
        crate::source_document::resolved_source_record_index::BindError,
    > {
        let node = owner
            .element_children()
            .find(|child| child.name.eq_ignore_ascii_case("colors"))
            .and_then(|colors| {
                colors
                    .element_children()
                    .find(|child| child.name.eq_ignore_ascii_case(name))
            })
            .ok_or_else(|| invalid(owner, name))?;
        let mut channels = [0, 0, 0, 255];
        for (channel, attribute) in channels.iter_mut().zip(["r", "g", "b", "a"]) {
            if let Some(value) = node.attribute_named_any(&[attribute]) {
                *channel = value.parse::<u8>().map_err(|_| invalid(node, attribute))?;
            }
        }
        Ok(channels)
    };
    let mut policy = TransportHighlightPolicy::default();
    fn find_mode<'a>(
        node: &'a OrderedSourceDocumentNode,
        mode: &str,
    ) -> Option<&'a OrderedSourceDocumentNode> {
        if node.name.eq_ignore_ascii_case(mode) {
            return Some(node);
        }
        node.element_children()
            .find_map(|child| find_mode(child, mode))
    }
    if let Some(track) = find_mode(&document.root, "ZTGroundTrackMode")
        .filter(|mode| mode.attribute_named_any(&["colorAnimationCycle"]).is_some())
    {
        policy.track = Some(TrackHighlightPolicy {
            place: [color(track, "place_yes")?, color(track, "place_no")?],
            steep: color(track, "too_steep")?,
            endpoint: [
                color(track, "endpoint_highlight1")?,
                color(track, "endpoint_highlight2")?,
            ],
            station: [
                color(track, "station_highlight1")?,
                color(track, "station_highlight2")?,
            ],
            color_interval_seconds: interval(track)?,
        });
    }
    if let Some(deletion) = find_mode(&document.root, "ZTDeleteMode")
        .filter(|mode| mode.attribute_named_any(&["colorAnimationCycle"]).is_some())
    {
        policy.deletion = Some(TransportDeletionHighlightPolicy {
            colors: [color(deletion, "mouseover")?, color(deletion, "alternate")?],
            color_interval_seconds: interval(deletion)?,
        });
    }
    Ok((policy != TransportHighlightPolicy::default()).then_some(policy))
}
