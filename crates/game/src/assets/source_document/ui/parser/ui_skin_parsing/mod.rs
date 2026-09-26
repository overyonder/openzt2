//! Authored UI skin, theme, and replacement parsing.

use crate::assets::source_document::{
    ordered_source_document_types::OrderedSourceDocumentNode, path::AssetPath,
};

use super::{
    super::model::{SourceUiSkin, SourceUiSkinReplacement, SourceUiSkinTheme},
    source_scalar_and_attribute_reading::copy_unknown_source_ui_attributes,
};

pub(super) fn parse_authored_ui_skins(root: &OrderedSourceDocumentNode) -> Vec<SourceUiSkin> {
    root.element_children()
        .filter(|node| node.name == "replacement")
        .flat_map(|node| node.element_children())
        .filter(|node| node.name == "skin")
        .map(|skin| SourceUiSkin {
            name: skin.attribute("name").unwrap_or_default().to_owned(),
            replacement_type: skin.attribute("replaceType").map(str::to_owned),
            directory: skin
                .attribute("dir")
                .filter(|value| !value.is_empty())
                .map(AssetPath::new),
            replacements: skin
                .element_children()
                .filter(|node| node.name == "replace")
                .filter_map(parse_authored_ui_skin_replacement)
                .collect(),
            themes: skin
                .element_children()
                .filter(|node| node.name == "theme")
                .map(|theme| SourceUiSkinTheme {
                    name: theme.attribute("name").unwrap_or_default().to_owned(),
                    replacements: theme
                        .element_children()
                        .filter(|node| node.name == "replace")
                        .filter_map(parse_authored_ui_skin_replacement)
                        .collect(),
                    unknown_attributes: copy_unknown_source_ui_attributes(
                        &theme.attributes,
                        &["name"],
                    ),
                })
                .collect(),
            unknown_attributes: copy_unknown_source_ui_attributes(
                &skin.attributes,
                &["name", "replaceType", "dir"],
            ),
        })
        .collect()
}

fn parse_authored_ui_skin_replacement(
    node: &OrderedSourceDocumentNode,
) -> Option<SourceUiSkinReplacement> {
    Some(SourceUiSkinReplacement {
        target_name: node.attribute("name")?.to_owned(),
        image: node.attribute("image").map(AssetPath::new),
        unknown_attributes: copy_unknown_source_ui_attributes(&node.attributes, &["name", "image"]),
    })
}
