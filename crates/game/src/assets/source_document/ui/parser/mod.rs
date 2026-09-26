//! Typed parsing of Blue Fang UI documents.

use crate::assets::source_document::{
    ordered_source_document_types::{OrderedSourceDocument, OrderedSourceDocumentSpan},
    path::AssetPath,
};

use super::model::*;

mod focused_widget_property_parsing;
mod globe_source_extraction;
mod source_payload_node_copying;
mod source_scalar_and_attribute_reading;
mod ui_aspect_and_visual_source_parsing;
mod ui_event_and_hotkey_parsing;
mod ui_region_and_state_parsing;
mod ui_skin_parsing;
mod ui_widget_kind_dispatch;
mod ui_widget_tree_parsing;

use ui_event_and_hotkey_parsing::{
    parse_authored_ui_hotkeys as parse_hotkeys,
    parse_named_authored_ui_event_lists as parse_named_event_lists,
};
use ui_skin_parsing::parse_authored_ui_skins;
use ui_widget_tree_parsing::{parse_authored_ui_node, parse_authored_ui_root};

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum SourceUiDiagnostic {
    UnknownWidget {
        tag: String,
        span: OrderedSourceDocumentSpan,
    },
    UnknownAttribute {
        widget: String,
        attribute: SourceUiAttribute,
        span: OrderedSourceDocumentSpan,
    },
    UnknownElement {
        widget: String,
        element: String,
        span: OrderedSourceDocumentSpan,
    },
    InvalidValue {
        widget: String,
        field: String,
        value: String,
        span: OrderedSourceDocumentSpan,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SourceUiDocument {
    pub(crate) path: AssetPath,
    pub(crate) root: SourceUiNode,
    pub(crate) skins: Vec<SourceUiSkin>,
    pub(crate) cursor_directory: Option<String>,
    pub(crate) named_event_lists: Vec<SourceUiNamedEventList>,
    pub(crate) diagnostics: Vec<SourceUiDiagnostic>,
}

impl SourceUiDocument {
    pub(crate) fn parse(document: &OrderedSourceDocument) -> Self {
        let skins = parse_authored_ui_skins(&document.root);
        let mut diagnostics = Vec::new();
        let root = parse_authored_ui_root(&document.root, &mut diagnostics);
        let named_event_lists = parse_named_event_lists(&document.root, &mut diagnostics);
        Self {
            path: document.path.clone(),
            root,
            skins,
            cursor_directory: document
                .root
                .attribute("cursorDirectory")
                .map(str::to_owned),
            named_event_lists,
            diagnostics,
        }
    }

    /// Parse the shipped mode-global hotkey registry into transient UI nodes.
    ///
    /// Each direct child is a named source mode rather than a widget. Giving
    /// it a container-shaped transient node lets live lowering compose its complete
    /// bindings into consumer roles without retaining the registry at runtime.
    pub(crate) fn parse_hotkey_modes(document: &OrderedSourceDocument) -> Option<Self> {
        if !document.root.name.eq_ignore_ascii_case("hotkeys") {
            return None;
        }
        let mut diagnostics = Vec::new();
        let mut root = parse_authored_ui_root(&document.root, &mut diagnostics);
        root.kind = SourceUiWidgetKind::Root;
        root.name = Some("hotkeys".into());
        root.hotkeys.clear();
        root.events.clear();
        root.unknown_attributes.clear();
        root.unknown_elements.clear();
        root.children = document
            .root
            .element_children()
            .map(|mode| {
                let mut mode_node = parse_authored_ui_node(mode, &mut diagnostics);
                mode_node.kind = SourceUiWidgetKind::Container;
                mode_node.name = Some(mode.name.to_string());
                mode_node.hotkeys = parse_hotkeys(mode);
                mode_node.events.clear();
                mode_node.children.clear();
                mode_node.unknown_attributes.clear();
                mode_node.unknown_elements.clear();
                mode_node
            })
            .collect();
        Some(Self {
            path: document.path.clone(),
            root,
            skins: Vec::new(),
            cursor_directory: None,
            named_event_lists: Vec::new(),
            diagnostics,
        })
    }
}
