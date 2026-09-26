use crate::asset_source::AssetArchives;
use crate::assets::source_document::ordered_source_document_types::{
    OrderedSourceDocument, OrderedSourceDocumentAttribute, OrderedSourceDocumentNode,
};
#[cfg(test)]
use crate::assets::source_document::{
    blue_fang_source_document_parsing::parse_blue_fang_source_document, path::AssetPath,
};
pub(super) fn ordered_source_document_contains_element_named(
    root: &OrderedSourceDocumentNode,
    wanted_name: &str,
) -> bool {
    fn beneath(node: &OrderedSourceDocumentNode, wanted_name: &str) -> bool {
        node.name.eq_ignore_ascii_case(wanted_name)
            || node
                .element_children()
                .any(|child| beneath(child, wanted_name))
    }

    root.name.eq_ignore_ascii_case(wanted_name)
        || root
            .element_children()
            .any(|child| beneath(child, wanted_name))
}

/// Finds expansion autoplacement files for the biome.
pub(super) fn authored_biome_automatic_placement_supplement_source_paths(
    archives: &AssetArchives,
    primary_document: &OrderedSourceDocument,
) -> Vec<std::path::PathBuf> {
    if !primary_document.root.name.eq_ignore_ascii_case("BFGBiome") {
        return Vec::new();
    }
    let Some(biome_name) = primary_document.root.attribute("name") else {
        return Vec::new();
    };
    let directory = format!(
        "biomes/autoplacement/{}/",
        biome_name.trim().to_ascii_lowercase()
    );
    let mut paths = archives
        .resolved_paths()
        .1
        .iter()
        .filter(|candidate| {
            let candidate = candidate
                .to_string_lossy()
                .replace('\\', "/")
                .to_ascii_lowercase();
            candidate.starts_with(&directory)
                && candidate
                    .strip_prefix(&directory)
                    .is_some_and(|suffix| !suffix.contains('/'))
                && candidate.ends_with(".xml")
        })
        .cloned()
        .collect::<Vec<_>>();
    paths.sort_unstable_by_key(|candidate| {
        candidate
            .to_string_lossy()
            .replace('\\', "/")
            .to_ascii_lowercase()
    });
    paths
}

pub(super) fn authored_hireable_generic_binder_family_variant_source_paths(
    archives: &AssetArchives,
    primary_path: &str,
    primary_document: &OrderedSourceDocument,
) -> Vec<std::path::PathBuf> {
    let root = &primary_document.root;
    let abstract_binder = root.name.eq_ignore_ascii_case("BFTypedBinder")
        && root
            .attributes
            .iter()
            .find(|attribute| attribute.name().eq_ignore_ascii_case("abstract"))
            .is_some_and(|attribute| {
                matches!(
                    attribute.value().trim().to_ascii_lowercase().as_str(),
                    "true" | "1"
                )
            });
    let authored_staff_type = root
        .element_children()
        .find(|source_node| source_node.name.eq_ignore_ascii_case("types"))
        .is_some_and(source_node_contains_staff_type);
    if !abstract_binder
        || !authored_staff_type
        || !root
            .element_children()
            .any(source_node_contains_staff_placement_action)
    {
        return Vec::new();
    }
    let normalized_primary_path = primary_path.replace('\\', "/").to_ascii_lowercase();
    let Some((directory, file_name)) = normalized_primary_path.rsplit_once('/') else {
        return Vec::new();
    };
    let Some((stem, _)) = file_name.rsplit_once('.') else {
        return Vec::new();
    };
    let family_prefix = format!("{directory}/{stem}_");
    let mut paths = archives
        .resolved_paths()
        .1
        .iter()
        .filter(|candidate| {
            let candidate = candidate
                .to_string_lossy()
                .replace('\\', "/")
                .to_ascii_lowercase();
            candidate.starts_with(&family_prefix)
                && candidate
                    .strip_prefix(&family_prefix)
                    .is_some_and(|suffix| !suffix.contains('/'))
                && candidate.ends_with(".xml")
        })
        .cloned()
        .collect::<Vec<_>>();
    paths.sort_unstable_by_key(|candidate| {
        candidate
            .to_string_lossy()
            .replace('\\', "/")
            .to_ascii_lowercase()
    });
    paths
}

fn source_node_contains_staff_type(source_node: &OrderedSourceDocumentNode) -> bool {
    source_node.name.eq_ignore_ascii_case("Staff")
        || source_node
            .element_children()
            .any(source_node_contains_staff_type)
}

fn source_node_contains_staff_placement_action(source_node: &OrderedSourceDocumentNode) -> bool {
    (source_node.name.eq_ignore_ascii_case("event")
        && source_node.attributes.iter().any(|attribute| {
            attribute.name().eq_ignore_ascii_case("msg")
                && attribute
                    .value()
                    .eq_ignore_ascii_case("ZT_SETPLACEMENTOBJECT")
        }))
        || source_node
            .element_children()
            .any(source_node_contains_staff_placement_action)
}

/// Finds ancestor binders named by the repeated <types> path.
pub(super) fn collect_authored_typed_binder_family_references(
    source_documents: &[OrderedSourceDocument],
) -> std::collections::BTreeSet<(String, String)> {
    fn append_descendant_names(source_node: &OrderedSourceDocumentNode, output: &mut Vec<String>) {
        for child in source_node.element_children() {
            output.push(child.name.to_string());
            append_descendant_names(child, output);
        }
    }

    let mut references = std::collections::BTreeSet::new();
    for source_document in source_documents {
        let root = &source_document.root;
        if !root.name.eq_ignore_ascii_case("BFTypedBinder") {
            continue;
        }
        let Some(types) = root
            .element_children()
            .find(|source_node| source_node.name.eq_ignore_ascii_case("types"))
        else {
            continue;
        };
        let Some(entity) = types
            .element_children()
            .find(|source_node| source_node.name.eq_ignore_ascii_case("entity"))
        else {
            continue;
        };
        let concrete_binder_type = root
            .attributes
            .iter()
            .find(|attribute| attribute.name().eq_ignore_ascii_case("binderType"))
            .map(OrderedSourceDocumentAttribute::value)
            .unwrap_or_default();
        let mut family_names = Vec::new();
        // All declared ancestors can supply inherited defaults.
        append_descendant_names(entity, &mut family_names);
        family_names
            .into_iter()
            .filter(|family_name| !family_name.eq_ignore_ascii_case(concrete_binder_type))
            .for_each(|family_name| {
                references.insert((source_document.path.key(), family_name));
            });
    }
    references
}

#[cfg(test)]
mod typed_binder_family_reference_tests {
    use super::*;

    #[test]
    fn building_type_ancestors_are_loaded_without_loading_the_concrete_type_again() {
        let document = parse_blue_fang_source_document(
            AssetPath::new("entities/objects/buildings/ai/stage.xml"),
            br#"<BFTypedBinder binderType="stage"><types><entity><building><stage/></building></entity></types></BFTypedBinder>"#,
        ).expect("valid source fixture");
        let references = collect_authored_typed_binder_family_references(&[document]);
        assert_eq!(
            references.into_iter().collect::<Vec<_>>(),
            vec![(
                "entities/objects/buildings/ai/stage.xml".to_owned(),
                "building".to_owned(),
            )]
        );
    }
}

pub(super) fn resolved_world_definition_source_documents_declare_viewing_area_type(
    source_documents: &[OrderedSourceDocument],
) -> bool {
    fn source_node_contains_viewing_area_type(source_node: &OrderedSourceDocumentNode) -> bool {
        source_node.name.eq_ignore_ascii_case("ViewingArea")
            || source_node
                .element_children()
                .any(source_node_contains_viewing_area_type)
    }

    source_documents.iter().any(|source_document| {
        source_document
            .root
            .element_children()
            .filter(|source_node| source_node.name.eq_ignore_ascii_case("types"))
            .any(|type_declarations| {
                type_declarations
                    .element_children()
                    .any(source_node_contains_viewing_area_type)
            })
    })
}

pub(super) fn collect_authored_world_definition_source_document_dependency_references(
    source_documents: &[OrderedSourceDocument],
) -> std::collections::BTreeSet<(String, String)> {
    fn collect_source_document_inheritance_attributes(
        source_document_path: &str,
        source_node_name: &str,
        source_attributes: &[OrderedSourceDocumentAttribute],
        source_document_references: &mut std::collections::BTreeSet<(String, String)>,
    ) {
        source_attributes
            .iter()
            .filter(|source_attribute| {
                if source_attribute.name().eq_ignore_ascii_case("template")
                    && source_node_name.rsplit(':').next().is_some_and(|name| {
                        name.get(..2)
                            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("UI"))
                    })
                {
                    return false;
                }
                [
                    "extends",
                    "base",
                    "parentType",
                    "inherit",
                    "template",
                    "s_supportType",
                    "s_curveSupportType",
                ]
                .iter()
                .any(|attribute_name| source_attribute.name().eq_ignore_ascii_case(attribute_name))
            })
            .map(OrderedSourceDocumentAttribute::value)
            .filter(|source_reference| !source_reference.trim().is_empty())
            .for_each(|source_reference| {
                source_document_references.insert((
                    source_document_path.to_owned(),
                    source_reference.trim().to_owned(),
                ));
            });
    }

    fn collect_source_document_references_beneath_node(
        source_document_path: &str,
        source_node: &OrderedSourceDocumentNode,
        source_document_references: &mut std::collections::BTreeSet<(String, String)>,
    ) {
        collect_source_document_inheritance_attributes(
            source_document_path,
            &source_node.name,
            &source_node.attributes,
            source_document_references,
        );
        let source_node_local_name = source_node
            .name
            .rsplit(':')
            .next()
            .unwrap_or(&source_node.name);
        if ["fog", "skylayers"]
            .iter()
            .any(|node_name| source_node_local_name.eq_ignore_ascii_case(node_name))
        {
            source_node
                .attributes
                .iter()
                .find(|source_attribute| source_attribute.name().eq_ignore_ascii_case("file"))
                .map(OrderedSourceDocumentAttribute::value)
                .filter(|source_reference| {
                    source_reference
                        .trim()
                        .to_ascii_lowercase()
                        .ends_with(".xml")
                })
                .into_iter()
                .for_each(|source_reference| {
                    source_document_references.insert((
                        source_document_path.to_owned(),
                        source_reference.trim().to_owned(),
                    ));
                });
        }
        source_node.element_children().for_each(|child_node| {
            collect_source_document_references_beneath_node(
                source_document_path,
                child_node,
                source_document_references,
            )
        });
    }

    let mut source_document_references = std::collections::BTreeSet::new();
    source_documents.iter().for_each(|source_document| {
        let source_document_path = source_document.path.key();
        collect_source_document_inheritance_attributes(
            &source_document_path,
            &source_document.root.name,
            &source_document.root.attributes,
            &mut source_document_references,
        );
        source_document
            .root
            .element_children()
            .for_each(|source_node| {
                collect_source_document_references_beneath_node(
                    &source_document_path,
                    source_node,
                    &mut source_document_references,
                )
            });
    });
    source_document_references
}
