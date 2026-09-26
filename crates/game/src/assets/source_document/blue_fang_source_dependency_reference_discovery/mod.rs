//! Authored dependency references discovered from transient Blue Fang source documents.

use std::collections::BTreeSet;

use super::{
    blue_fang_actor_manifest_model_and_scene_resolution_index::BlueFangActorManifestModelAndSceneResolutionIndex,
    ordered_source_document_types::{OrderedSourceDocument, OrderedSourceDocumentNode},
    path::AssetPath,
    source_document_semantic_name::source_document_names_are_semantically_equal,
};

fn source_node_is_traversable_animation_path_component(
    source_node: &OrderedSourceDocumentNode,
) -> bool {
    source_node
        .name
        .rsplit(':')
        .next()
        .is_some_and(|name| name.eq_ignore_ascii_case("BFTravAnimPathComponent"))
}

pub(in crate::assets) fn source_scene_component_prefers_available_blue_fang_model(
    source_component_name: &str,
    explicit_bfr_attribute: Option<&str>,
) -> bool {
    [
        "BFSceneGraphComponent",
        "BFSimpleLODComponent",
        "BFRSceneGraphComponent",
    ]
    .iter()
    .any(|name| source_document_names_are_semantically_equal(source_component_name, name))
        || explicit_bfr_attribute
            .is_some_and(|value| value.trim().eq_ignore_ascii_case("true") || value.trim() == "1")
}

pub(in crate::assets) fn collect_blue_fang_actor_manifest_source_references(
    source_documents: &[OrderedSourceDocument],
) -> BTreeSet<String> {
    fn collect_actor_manifest_references_beneath_source_node(
        source_node: &OrderedSourceDocumentNode,
        actor_manifest_references: &mut BTreeSet<String>,
    ) {
        if !source_node_is_traversable_animation_path_component(source_node) {
            source_node
                .attribute("actorfile")
                .filter(|actor_manifest_path| !actor_manifest_path.trim().is_empty())
                .map(|actor_manifest_path| AssetPath::new(actor_manifest_path).key())
                .into_iter()
                .for_each(|actor_manifest_path| {
                    actor_manifest_references.insert(actor_manifest_path);
                });
        }
        source_node.element_children().for_each(|child_node| {
            collect_actor_manifest_references_beneath_source_node(
                child_node,
                actor_manifest_references,
            )
        });
    }

    let mut actor_manifest_references = BTreeSet::new();
    for source_document in source_documents {
        source_document
            .root
            .attribute("actorfile")
            .filter(|actor_manifest_path| !actor_manifest_path.trim().is_empty())
            .map(|actor_manifest_path| AssetPath::new(actor_manifest_path).key())
            .into_iter()
            .for_each(|actor_manifest_path| {
                actor_manifest_references.insert(actor_manifest_path);
            });
        source_document
            .root
            .element_children()
            .for_each(|source_node| {
                collect_actor_manifest_references_beneath_source_node(
                    source_node,
                    &mut actor_manifest_references,
                )
            });
    }
    actor_manifest_references
}

pub(in crate::assets) fn collect_blue_fang_native_model_source_references(
    source_documents: &[OrderedSourceDocument],
    actor_manifest_resolution_index: &BlueFangActorManifestModelAndSceneResolutionIndex,
) -> BTreeSet<(String, String, bool)> {
    fn collect_native_model_references_beneath_source_node(
        declaring_source_path: &str,
        source_node: &OrderedSourceDocumentNode,
        inside_environment_sky_layers: bool,
        actor_manifest_resolution_index: &BlueFangActorManifestModelAndSceneResolutionIndex,
        native_model_references: &mut BTreeSet<(String, String, bool)>,
    ) {
        collect_native_model_references_from_source_node(
            declaring_source_path,
            source_node,
            inside_environment_sky_layers,
            actor_manifest_resolution_index,
            native_model_references,
        );
        let inside_environment_sky_layers = inside_environment_sky_layers
            || source_document_names_are_semantically_equal(&source_node.name, "skylayers");
        source_node.element_children().for_each(|child_node| {
            collect_native_model_references_beneath_source_node(
                declaring_source_path,
                child_node,
                inside_environment_sky_layers,
                actor_manifest_resolution_index,
                native_model_references,
            )
        });
    }

    fn collect_native_model_references_from_source_node(
        declaring_source_path: &str,
        source_node: &OrderedSourceDocumentNode,
        inside_environment_sky_layers: bool,
        actor_manifest_resolution_index: &BlueFangActorManifestModelAndSceneResolutionIndex,
        native_model_references: &mut BTreeSet<(String, String, bool)>,
    ) {
        for source_attribute_name in ["modelfile", "actorfile"] {
            if source_attribute_name == "actorfile"
                && source_node_is_traversable_animation_path_component(source_node)
            {
                continue;
            }
            let Some(authored_path) = source_node
                .attribute(source_attribute_name)
                .filter(|authored_path| !authored_path.trim().is_empty())
            else {
                continue;
            };
            let authored_path = AssetPath::new(authored_path).key();
            let native_model_path = actor_manifest_resolution_index
                .native_model_source_path_for_normalized_actor_manifest_key(&authored_path)
                .map_or(authored_path, str::to_owned);
            let prefer_blue_fang_bfb = source_attribute_name == "modelfile"
                && source_scene_component_prefers_available_blue_fang_model(
                    &source_node.name,
                    source_node.attribute("isBFR"),
                );
            if native_model_path.ends_with(".nif")
                || native_model_path.ends_with(".bfb")
                || !native_model_path.contains('.')
                || source_attribute_name == "actorfile"
            {
                native_model_references.insert((
                    declaring_source_path.to_owned(),
                    native_model_path,
                    prefer_blue_fang_bfb,
                ));
            }
        }

        let environment_model_collection = ["interpModels", "sunModels", "skirtModels"]
            .iter()
            .any(|name| source_document_names_are_semantically_equal(&source_node.name, name))
            || (inside_environment_sky_layers
                && source_document_names_are_semantically_equal(&source_node.name, "models"));
        if environment_model_collection {
            source_node
                .attributes
                .iter()
                .map(|attribute| AssetPath::new(attribute.value()).key())
                .filter(|authored_path| !authored_path.is_empty())
                .for_each(|authored_path| {
                    native_model_references.insert((
                        declaring_source_path.to_owned(),
                        authored_path,
                        false,
                    ));
                });
        }
        if source_document_names_are_semantically_equal(&source_node.name, "ZTSunLayer") {
            source_node
                .attribute("sunPositionModel")
                .map(|authored_path| AssetPath::new(authored_path).key())
                .filter(|authored_path| !authored_path.is_empty())
                .into_iter()
                .for_each(|authored_path| {
                    native_model_references.insert((
                        declaring_source_path.to_owned(),
                        authored_path,
                        false,
                    ));
                });
        }
    }

    let mut native_model_references = BTreeSet::new();
    for source_document in source_documents {
        let declaring_source_path = source_document.path.as_str();
        for source_attribute_name in ["modelfile", "actorfile"] {
            let Some(authored_path) = source_document
                .root
                .attribute(source_attribute_name)
                .filter(|authored_path| !authored_path.trim().is_empty())
            else {
                continue;
            };
            let authored_path = AssetPath::new(authored_path).key();
            let native_model_path = actor_manifest_resolution_index
                .native_model_source_path_for_normalized_actor_manifest_key(&authored_path)
                .map_or(authored_path, str::to_owned);
            let prefer_blue_fang_bfb = source_attribute_name == "modelfile"
                && source_scene_component_prefers_available_blue_fang_model(
                    &source_document.root.name,
                    source_document.root.attribute("isBFR"),
                );
            if native_model_path.ends_with(".nif")
                || native_model_path.ends_with(".bfb")
                || !native_model_path.contains('.')
                || source_attribute_name == "actorfile"
            {
                native_model_references.insert((
                    declaring_source_path.to_owned(),
                    native_model_path,
                    prefer_blue_fang_bfb,
                ));
            }
        }
        source_document
            .root
            .element_children()
            .for_each(|source_node| {
                let inside_environment_sky_layers = source_document_names_are_semantically_equal(
                    &source_document.root.name,
                    "skylayers",
                );
                collect_native_model_references_beneath_source_node(
                    declaring_source_path,
                    source_node,
                    inside_environment_sky_layers,
                    actor_manifest_resolution_index,
                    &mut native_model_references,
                )
            });
    }
    native_model_references
}

pub(in crate::assets) fn collect_blue_fang_static_asset_source_references(
    source_documents: &[OrderedSourceDocument],
) -> BTreeSet<(String, String)> {
    fn collect_static_asset_references_from_source_value(
        declaring_source_path: &str,
        source_value: &str,
        static_asset_references: &mut BTreeSet<(String, String)>,
    ) {
        source_value
            .split(|character: char| {
                character.is_whitespace() || matches!(character, ',' | ';' | '\'' | '"' | '(' | ')')
            })
            .map(|source_token| AssetPath::new(source_token).key())
            .filter(|source_token| {
                source_token.rsplit_once('.').is_some_and(|(_, extension)| {
                    [
                        "dds", "png", "jpg", "jpeg", "bmp", "tga", "nif", "bfb", "psys",
                    ]
                    .iter()
                    .any(|candidate| extension.eq_ignore_ascii_case(candidate))
                })
            })
            .for_each(|static_asset_path| {
                static_asset_references
                    .insert((declaring_source_path.to_owned(), static_asset_path));
            });
    }

    fn collect_static_asset_references_beneath_source_node(
        declaring_source_path: &str,
        source_node: &OrderedSourceDocumentNode,
        static_asset_references: &mut BTreeSet<(String, String)>,
    ) {
        source_node.attributes.iter().for_each(|source_attribute| {
            collect_static_asset_references_from_source_value(
                declaring_source_path,
                source_attribute.value(),
                static_asset_references,
            )
        });
        source_node
            .first_text()
            .into_iter()
            .for_each(|source_text| {
                collect_static_asset_references_from_source_value(
                    declaring_source_path,
                    source_text,
                    static_asset_references,
                )
            });
        source_node.element_children().for_each(|child_node| {
            collect_static_asset_references_beneath_source_node(
                declaring_source_path,
                child_node,
                static_asset_references,
            )
        });
    }

    let mut static_asset_references = BTreeSet::new();
    for source_document in source_documents {
        let declaring_source_path = source_document.path.as_str();
        source_document
            .root
            .attributes
            .iter()
            .for_each(|source_attribute| {
                collect_static_asset_references_from_source_value(
                    declaring_source_path,
                    source_attribute.value(),
                    &mut static_asset_references,
                )
            });
        source_document
            .root
            .first_text()
            .into_iter()
            .for_each(|source_text| {
                collect_static_asset_references_from_source_value(
                    declaring_source_path,
                    source_text,
                    &mut static_asset_references,
                )
            });
        source_document
            .root
            .element_children()
            .for_each(|source_node| {
                collect_static_asset_references_beneath_source_node(
                    declaring_source_path,
                    source_node,
                    &mut static_asset_references,
                )
            });
    }
    static_asset_references
}
