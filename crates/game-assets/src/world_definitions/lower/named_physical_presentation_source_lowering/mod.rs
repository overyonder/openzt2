use super::source_element_tree_search::{
    authored_type_family_elements_named, find_descendant, find_presentation_component,
};
use crate::source_document::{
    blue_fang_source_numeric_lexeme::parse_blue_fang_source_numeric_lexeme,
    resolved_source_record_index::{BindError, RecordView},
    source_document_semantic_name::{
        canonicalize_source_document_record_key, source_document_names_are_semantically_equal,
    },
};
use openzt2_game_data::{
    world_definitions::world_objects::WorldObjectNamedPhysicalPresentationDefinition, AssetId,
};
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn lower_named_physical_presentations(
    record: &RecordView<'_, '_>,
    scene_paths: &BTreeMap<String, String>,
) -> Result<Vec<WorldObjectNamedPhysicalPresentationDefinition>, BindError> {
    let mut presentations = Vec::new();
    let mut seen = BTreeSet::new();
    for binder in authored_type_family_elements_named(record, "BFNamedBinder") {
        let Some(name) = binder.attribute_named_any(&["binderName"]) else {
            continue;
        };
        let Some(component) = find_presentation_component(binder) else {
            continue;
        };
        if !seen.insert(canonicalize_source_document_record_key(name)) {
            continue;
        }
        let required = match binder
            .attribute_named_any(&["required"])
            .map(|value| value.trim().to_ascii_lowercase())
            .as_deref()
        {
            None | Some("" | "true" | "1") => true,
            Some("false" | "0") => false,
            _ => {
                return Err(BindError::record(
                    record,
                    format!("physical binder {name:?} has an unsupported required value"),
                ))
            }
        };
        let reference = component
            .attribute_named_any(&["modelfile", "actorfile"])
            .expect("presentation lookup requires a reference");
        let scene = scene_paths.get(&canonicalize_source_document_record_key(reference));
        if scene.is_none() && find_descendant(binder, "BFPhysObj").is_none() {
            if !required {
                continue;
            }
            return Err(BindError::record(
                record,
                format!("physical binder {name:?} references unresolved scene {reference:?}"),
            ));
        }
        let scale = component
            .attribute_named_any(&["scale"])
            .map(|value| {
                parse_blue_fang_source_numeric_lexeme::<f32>(value)
                    .filter(|scale| scale.is_finite() && *scale > 0.0)
                    .ok_or_else(|| {
                        BindError::record(record, "physical binder scale must be positive")
                    })
            })
            .transpose()?
            .unwrap_or(1.0);
        presentations.push(WorldObjectNamedPhysicalPresentationDefinition {
            name: AssetId::from_key(&canonicalize_source_document_record_key(name)),
            prefab: scene.map_or_else(AssetId::default, |scene| AssetId::from_virtual_path(scene)),
            scale,
            required,
            // The original engine names this binder: event markers show
            // `unfinishedEventObj` until their event completes.
            initially_visible: !source_document_names_are_semantically_equal(
                name,
                "finishedEventObj",
            ),
        });
    }
    Ok(presentations)
}

#[cfg(test)]
mod tests {
    use crate::source_document::blue_fang_actor_manifest_model_and_scene_resolution_index::BlueFangActorManifestModelAndSceneResolutionIndex;
    use crate::source_document::blue_fang_source_document_parsing::parse_blue_fang_source_document;
    use crate::source_document::path::AssetPath;
    use crate::world_definitions::world_definition_source_document_lowering::lower_resolved_world_definition_source_document_closure_to_canonical_document;
    use openzt2_game_data::AssetId;

    #[test]
    fn event_marker_shows_only_its_unfinished_binder_initially(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let marker = parse_blue_fang_source_document(
            AssetPath::new("event_marker.xml"),
            br#"<BFTypedBinder binderType="gesture_event_marker">
            <types><entity><gesture_event_marker/></entity></types>
            <binder>
                <BFNamedBinder binderName="unfinishedEventObj" required="1"><instance><BFPhysObj><BFSceneGraphComponent modelfile="unfinished.nif"/></BFPhysObj></instance></BFNamedBinder>
                <BFNamedBinder binderName="finishedEventObj" required="1"><instance><BFPhysObj><BFSceneGraphComponent modelfile="finished.nif"/></BFPhysObj></instance></BFNamedBinder>
            </binder>
        </BFTypedBinder>"#,
        )
        .map_err(|error| std::io::Error::other(format!("{error:?}")))?;
        let mut scenes = BlueFangActorManifestModelAndSceneResolutionIndex::default();
        scenes.register_archive_resolved_native_model_scene_asset_path(
            "unfinished.nif",
            "unfinished.nif#scene",
        );
        scenes.register_archive_resolved_native_model_scene_asset_path(
            "finished.nif",
            "finished.nif#scene",
        );
        let document =
            lower_resolved_world_definition_source_document_closure_to_canonical_document(
                &[marker],
                &scenes,
                "event_marker.xml",
                [0, 0],
            )?
            .ok_or_else(|| std::io::Error::other("marker source produced no document"))?;
        let object = document
            .objects
            .first()
            .ok_or_else(|| std::io::Error::other("marker object is absent"))?;
        let visibility = object
            .named_physical_presentations
            .iter()
            .map(|presentation| (presentation.name, presentation.initially_visible))
            .collect::<Vec<_>>();
        assert_eq!(
            visibility,
            [
                (AssetId::from_key("unfinishedeventobj"), true),
                (AssetId::from_key("finishedeventobj"), false),
            ]
        );
        Ok(())
    }
}
