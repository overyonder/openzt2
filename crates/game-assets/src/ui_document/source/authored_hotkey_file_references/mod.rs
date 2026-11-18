use std::collections::BTreeMap;

use crate::source_document::{
    ordered_source_document_types::{
        OrderedSourceDocument, OrderedSourceDocumentChild, OrderedSourceDocumentChildren,
        OrderedSourceDocumentNode,
    },
    path::AssetPath,
};

use super::ui_source_document_gap::{
    UiSourceDocumentFamily, UiSourceDocumentGap, UiSourceDocumentGapKind,
};

pub(super) fn collect_authored_hotkey_file_paths(document: &OrderedSourceDocument) -> Vec<String> {
    let mut paths = Vec::new();
    if document.root.name.eq_ignore_ascii_case("hotkeys") {
        for mode in document.root.element_children() {
            paths.extend(
                mode.element_children()
                    .filter_map(|node| hotkey_file_reference(node).map(|(path, _)| path)),
            );
        }
    }
    collect_ui_node_hotkey_file_paths(&document.root, &mut paths);
    paths
}

fn collect_ui_node_hotkey_file_paths(node: &OrderedSourceDocumentNode, paths: &mut Vec<String>) {
    if node.name == "UIHotKeys" {
        paths.extend(
            node.element_children()
                .filter_map(|node| hotkey_file_reference(node).map(|(path, _)| path)),
        );
    } else {
        for child in node.element_children() {
            collect_ui_node_hotkey_file_paths(child, paths);
        }
    }
}

fn hotkey_file_reference(node: &OrderedSourceDocumentNode) -> Option<(String, &str)> {
    if node.name != "file" {
        return None;
    }
    let path = node.attribute("name")?.trim();
    let mode = node.attribute("node")?.trim();
    (!path.is_empty() && !mode.is_empty()).then(|| (AssetPath::new(path).key(), mode))
}

pub(super) fn expand_authored_hotkey_files(
    document: &OrderedSourceDocument,
    registries: &BTreeMap<String, &OrderedSourceDocument>,
) -> Result<OrderedSourceDocument, UiSourceDocumentGap> {
    let source = document.path.key();
    let root = if document.root.name.eq_ignore_ascii_case("hotkeys") {
        let mut root = document.root.clone();
        let mut modes = root.child_items().to_vec();
        for mode in &mut modes {
            if let OrderedSourceDocumentChild::Element(mode) = mode {
                expand_hotkey_binding_sequence(mode, registries, &source, &mut Vec::new())?;
            }
        }
        root.children =
            OrderedSourceDocumentChildren::from_ordered_source_document_child_items(modes);
        root
    } else {
        expand_ui_node_hotkeys(&document.root, registries, &source)?
    };
    Ok(OrderedSourceDocument {
        path: document.path.clone(),
        format: document.format,
        root,
    })
}

fn expand_ui_node_hotkeys(
    node: &OrderedSourceDocumentNode,
    registries: &BTreeMap<String, &OrderedSourceDocument>,
    source: &str,
) -> Result<OrderedSourceDocumentNode, UiSourceDocumentGap> {
    if node.name == "UIHotKeys" {
        let mut bindings = node.clone();
        expand_hotkey_binding_sequence(&mut bindings, registries, source, &mut Vec::new())?;
        return Ok(bindings);
    }
    let children = node
        .child_items()
        .iter()
        .map(|child| match child {
            OrderedSourceDocumentChild::Element(child) => Ok(OrderedSourceDocumentChild::Element(
                Box::new(expand_ui_node_hotkeys(child, registries, source)?),
            )),
            child => Ok(child.clone()),
        })
        .collect::<Result<Vec<_>, UiSourceDocumentGap>>()?;
    Ok(OrderedSourceDocumentNode {
        name: node.name.clone(),
        attributes: node.attributes.clone(),
        children: OrderedSourceDocumentChildren::from_ordered_source_document_child_items(children),
        span: node.span,
    })
}

fn expand_hotkey_binding_sequence(
    bindings: &mut OrderedSourceDocumentNode,
    registries: &BTreeMap<String, &OrderedSourceDocument>,
    source: &str,
    include_stack: &mut Vec<(String, String)>,
) -> Result<(), UiSourceDocumentGap> {
    let mut expanded = Vec::new();
    for item in bindings.child_items() {
        let Some(reference) = item.as_element() else {
            expanded.push(item.clone());
            continue;
        };
        let Some((path, mode)) = hotkey_file_reference(reference) else {
            expanded.push(item.clone());
            continue;
        };
        let gap = |message| UiSourceDocumentGap {
            family: UiSourceDocumentFamily::Ui,
            kind: UiSourceDocumentGapKind::UnsupportedVocabulary,
            virtual_path: source.to_owned(),
            span: reference.span,
            message,
        };
        let registry = registries
            .get(&path)
            .ok_or_else(|| gap(format!("hotkey document {path:?} is unavailable")))?;
        let mut modes = registry
            .root
            .element_children()
            .filter(|node| node.name.eq_ignore_ascii_case(mode));
        let Some(mode_node) = modes.next().filter(|_| modes.next().is_none()) else {
            return Err(gap(format!(
                "hotkey mode {mode:?} in {path:?} must name one mode"
            )));
        };
        let key = (path, mode.to_ascii_lowercase());
        if include_stack.contains(&key) {
            return Err(gap(format!(
                "cyclic hotkey reference to {mode:?} in {:?}",
                key.0
            )));
        }
        include_stack.push(key);
        let mut mode_node = mode_node.clone();
        expand_hotkey_binding_sequence(
            &mut mode_node,
            registries,
            &registry.path.key(),
            include_stack,
        )?;
        include_stack.pop();
        expanded.extend_from_slice(mode_node.child_items());
    }
    bindings.children =
        OrderedSourceDocumentChildren::from_ordered_source_document_child_items(expanded);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source_document::{
        blue_fang_source_document_parsing::parse_blue_fang_source_document,
        ui::parser::SourceUiDocument,
    };

    #[test]
    fn a_dialog_expands_its_named_mode_between_authored_bindings() {
        let registry = parse_blue_fang_source_document(
            AssetPath::new("ui/hotkeys/modals.xml"),
            br#"<hotkeys><esctextcancel>
                <down code="27" msg="UI_CHILD" data="UIChildData" name="TextButton Cancel">
                    <child msg="UI_ACTIVATE"/>
                </down>
            </esctextcancel></hotkeys>"#,
        )
        .unwrap();
        let document = parse_blue_fang_source_document(
            AssetPath::new("ui/layout/load.xml"),
            br#"<UILayout name="Load Game Shell"><UIHotKeys>
                <down code="37" msg="UI_ACTIVATE"/>
                <file name="UI/hotkeys/modals.xml" node="esctextcancel"/>
                <down code="39" msg="UI_ACTIVATE"/>
            </UIHotKeys></UILayout>"#,
        )
        .unwrap();
        let expanded = expand_authored_hotkey_files(
            &document,
            &BTreeMap::from([(registry.path.key(), &registry)]),
        )
        .unwrap();
        let source = SourceUiDocument::parse(&expanded);
        assert_eq!(
            source
                .root
                .hotkeys
                .iter()
                .filter_map(|binding| binding.code)
                .collect::<Vec<_>>(),
            [37, 27, 39]
        );
        assert_eq!(
            source.root.hotkeys[1].event.target_child.as_deref(),
            Some("TextButton Cancel")
        );
    }

    #[test]
    fn an_unavailable_hotkey_mode_is_rejected() {
        let registry = parse_blue_fang_source_document(
            AssetPath::new("ui/hotkeys/hotkeys.xml"),
            b"<hotkeys><escclose><down code=\"27\" msg=\"UI_CLOSE\"/></escclose></hotkeys>",
        )
        .unwrap();
        let document = parse_blue_fang_source_document(
            AssetPath::new("ui/layout/load.xml"),
            b"<UILayout><UIHotKeys><file name=\"ui/hotkeys/hotkeys.xml\" node=\"missing\"/></UIHotKeys></UILayout>",
        ).unwrap();
        assert!(expand_authored_hotkey_files(
            &document,
            &BTreeMap::from([(registry.path.key(), &registry)])
        )
        .is_err());
    }
}
