use super::source_element_tree_search::{
    authored_type_family_elements_named, find_descendant, find_presentation_component,
};
use crate::source_document::{
    blue_fang_source_numeric_lexeme::parse_blue_fang_source_numeric_lexeme,
    ordered_source_document_types::OrderedSourceDocumentNode,
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
            attach_node: None,
            inherit_attach_rotation: false,
        });
    }
    Ok(presentations)
}

/// Lowers the binders `physObjParenting` hangs on nodes of an object's main
/// model, such as fires, mist, fans and flags on buildings.
///
/// Presentation-controller states (signs), optional alternatives
/// (`required="0"`) and actors (vendors, fish) are owned elsewhere, and a
/// binder whose scene is unresolved is left out.
pub(super) fn lower_parented_named_physical_presentations(
    record: &RecordView<'_, '_>,
    scene_paths: &BTreeMap<String, String>,
) -> Result<Vec<WorldObjectNamedPhysicalPresentationDefinition>, BindError> {
    let controlled = state_controlled_binder_names(record);
    let binders = authored_type_family_elements_named(record, "BFNamedBinder");
    let mut presentations = Vec::new();
    let mut seen = BTreeSet::new();
    for parenting in authored_type_family_elements_named(record, "parentObj") {
        let (Some(parent), Some(child), Some(node)) = (
            parenting.attribute_named_any(&["parent"]),
            parenting.attribute_named_any(&["child"]),
            parenting
                .attribute_named_any(&["attachnode"])
                .map(str::trim)
                .filter(|node| !node.is_empty()),
        ) else {
            continue;
        };
        let child_key = canonicalize_source_document_record_key(child);
        if !source_document_names_are_semantically_equal(parent, "mainObj")
            || controlled.contains(&child_key)
            || !seen.insert(child_key)
        {
            continue;
        }
        let Some(binder) = binders.iter().copied().find(|binder| {
            binder
                .attribute_named_any(&["binderName"])
                .is_some_and(|name| source_document_names_are_semantically_equal(name, child))
        }) else {
            continue;
        };
        if binder
            .attribute_named_any(&["required"])
            .is_some_and(|value| matches!(value.trim(), "0" | "false"))
        {
            continue;
        }
        let Some(component) = find_presentation_component(binder).filter(|component| {
            !source_document_names_are_semantically_equal(
                component.name.as_str(),
                "BFActorComponent",
            )
        }) else {
            continue;
        };
        let model = component
            .attribute_named_any(&["modelfile", "actorfile"])
            .expect("presentation lookup requires a reference");
        let Some(scene) = scene_paths.get(&canonicalize_source_document_record_key(model)) else {
            bevy::log::warn!(
                definition = record.key,
                binder = child,
                %model,
                "parented binder has no resolved model scene; leaving it out"
            );
            continue;
        };
        let scale = component
            .attribute_named_any(&["scale"])
            .map(|value| {
                parse_blue_fang_source_numeric_lexeme::<f32>(value)
                    .filter(|scale| scale.is_finite() && *scale > 0.0)
                    .ok_or_else(|| {
                        BindError::record(record, "parented binder scale must be positive")
                    })
            })
            .transpose()?
            .unwrap_or(1.0);
        presentations.push(WorldObjectNamedPhysicalPresentationDefinition {
            name: AssetId::from_key(&canonicalize_source_document_record_key(child)),
            prefab: AssetId::from_virtual_path(scene),
            scale,
            required: true,
            initially_visible: true,
            attach_node: Some(node.to_owned()),
            inherit_attach_rotation: parenting
                .attribute_named_any(&["rottype"])
                .is_some_and(|mode| source_document_names_are_semantically_equal(mode, "use")),
        });
    }
    Ok(presentations)
}

/// Binders named as the `child` of a presentation-controller state (any
/// element other than `parentObj` carrying `child`), such as sign states.
fn state_controlled_binder_names(record: &RecordView<'_, '_>) -> BTreeSet<String> {
    fn collect(element: &OrderedSourceDocumentNode, output: &mut BTreeSet<String>) {
        for child in element.element_children() {
            if !source_document_names_are_semantically_equal(child.name.as_str(), "parentObj") {
                if let Some(binder) = child.attribute_named_any(&["child"]) {
                    output.insert(canonicalize_source_document_record_key(binder));
                }
            }
            collect(child, output);
        }
    }
    let mut controlled = BTreeSet::new();
    collect(record.source_document_element(), &mut controlled);
    for family_key in record.type_tokens() {
        if let Some(family) = record.find_resolved_source_record_by_reference(&family_key) {
            collect(family.source_document_element(), &mut controlled);
        }
    }
    controlled
}

#[cfg(test)]
mod tests {
    use crate::source_document::blue_fang_actor_manifest_model_and_scene_resolution_index::BlueFangActorManifestModelAndSceneResolutionIndex;
    use crate::source_document::blue_fang_source_document_parsing::parse_blue_fang_source_document;
    use crate::source_document::path::AssetPath;
    use crate::world_definitions::world_definition_source_document_lowering::lower_resolved_world_definition_source_document_closure_to_canonical_document;
    use openzt2_game_data::AssetId;

    #[test]
    fn building_lowers_its_node_parented_model_binders() -> Result<(), Box<dyn std::error::Error>> {
        let building = parse_blue_fang_source_document(
            AssetPath::new("foodstand.xml"),
            br#"<BFTypedBinder binderType="foodstand">
            <types><entity><building><foodstand/></building></entity></types>
            <instance><BFGEntity><physObjParenting>
                <parentObj parent="mainObj" child="fire01" rottype="ignore" postype="use" attachnode="Link_Fire01"/>
                <parentObj parent="mainObj" child="vent" rottype="use" postype="use" attachnode="link_vent"/>
                <parentObj parent="mainObj" child="dead" rottype="use" postype="use" attachnode="autoroot"/>
                <parentObj parent="mainObj" child="vendor" rottype="use" postype="use" attachnode="Link_Vendor"/>
            </physObjParenting></BFGEntity></instance>
            <binder>
                <BFNamedBinder binderName="mainObj"><instance><BFPhysObj><BFSimpleLODComponent modelfile="stand.nif"/></BFPhysObj></instance></BFNamedBinder>
                <BFNamedBinder binderName="fire01"><instance><BFPhysObj><BFSimpleLODComponent modelfile="fire.nif" scale="0.45"/></BFPhysObj></instance></BFNamedBinder>
                <BFNamedBinder binderName="vent"><instance><BFPhysObj><BFSimpleLODComponent modelfile="vent.nif"/></BFPhysObj></instance></BFNamedBinder>
                <BFNamedBinder binderName="dead" required="0"><instance><BFPhysObj><BFSimpleLODComponent modelfile="vent.nif"/></BFPhysObj></instance></BFNamedBinder>
                <BFNamedBinder binderName="vendor"><instance><BFPhysObj><BFActorComponent actorfile="vendor.bfm"/></BFPhysObj></instance></BFNamedBinder>
            </binder>
        </BFTypedBinder>"#,
        )
        .map_err(|error| std::io::Error::other(format!("{error:?}")))?;
        let mut scenes = BlueFangActorManifestModelAndSceneResolutionIndex::default();
        for model in ["stand.nif", "fire.nif", "vent.nif"] {
            scenes.register_archive_resolved_native_model_scene_asset_path(
                model,
                &format!("{model}#scene"),
            );
        }
        let document =
            lower_resolved_world_definition_source_document_closure_to_canonical_document(
                &[building],
                &scenes,
                "foodstand.xml",
                [0, 0],
            )?
            .ok_or_else(|| std::io::Error::other("building source produced no document"))?;
        let object = document
            .objects
            .first()
            .ok_or_else(|| std::io::Error::other("building object is absent"))?;
        let parented = object
            .named_physical_presentations
            .iter()
            .map(|presentation| {
                (
                    presentation.name,
                    presentation.prefab,
                    presentation.attach_node.as_deref(),
                    presentation.inherit_attach_rotation,
                    presentation.scale,
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            parented,
            [
                (
                    AssetId::from_key("fire01"),
                    AssetId::from_virtual_path("fire.nif#scene"),
                    Some("Link_Fire01"),
                    false,
                    0.45,
                ),
                (
                    AssetId::from_key("vent"),
                    AssetId::from_virtual_path("vent.nif#scene"),
                    Some("link_vent"),
                    true,
                    1.0,
                ),
            ],
            "optional alternatives and actors are not parented presentations"
        );
        Ok(())
    }

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
