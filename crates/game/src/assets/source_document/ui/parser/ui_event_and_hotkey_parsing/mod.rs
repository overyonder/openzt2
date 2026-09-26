//! Authored UI event, named-event-list, and hotkey parsing.

use crate::assets::source_document::{
    ordered_source_document_types::OrderedSourceDocumentNode, path::AssetPath,
};

use super::{
    super::model::{
        SourceUiEvent, SourceUiEventBlock, SourceUiEventTrigger, SourceUiHotkey,
        SourceUiHotkeyTrigger, SourceUiNamedEventList, SourceUiPayloadNode, SourceUiXmlObjectEvent,
    },
    source_scalar_and_attribute_reading::{
        copy_unknown_source_ui_attributes, optional_nonempty_source_attribute, parse_source_bool,
    },
    ui_aspect_and_visual_source_parsing::parse_source_ui_color as parse_color,
    SourceUiDiagnostic,
};

pub(super) fn parse_authored_ui_event_block(
    node: &OrderedSourceDocumentNode,
) -> Option<SourceUiEventBlock> {
    Some(SourceUiEventBlock {
        trigger: parse_authored_ui_event_trigger(&node.name)?,
        events: node
            .element_children()
            .filter(|node| node.name == "event")
            .map(parse_authored_ui_event)
            .collect(),
    })
}

pub(super) fn parse_named_authored_ui_event_lists(
    root: &OrderedSourceDocumentNode,
    diagnostics: &mut Vec<SourceUiDiagnostic>,
) -> Vec<SourceUiNamedEventList> {
    let Some(lists) = root
        .element_children()
        .find(|node| node.name == "namedEventLists")
    else {
        return Vec::new();
    };
    lists
        .element_children()
        .map(|list| {
            diagnostics.extend(list.element_children().filter_map(|node| {
                (node.name != "event").then(|| SourceUiDiagnostic::UnknownElement {
                    widget: format!("namedEventList({})", list.name),
                    element: node.name.to_string(),
                    span: node.span,
                })
            }));
            SourceUiNamedEventList {
                name: list.name.to_string(),
                events: list
                    .element_children()
                    .filter(|node| node.name == "event")
                    .map(parse_authored_ui_event)
                    .collect(),
            }
        })
        .collect()
}

pub(super) fn parse_authored_ui_event_trigger(tag: &str) -> Option<SourceUiEventTrigger> {
    Some(match tag {
        "enter" => SourceUiEventTrigger::Enter,
        "leave" => SourceUiEventTrigger::Leave,
        "activate" => SourceUiEventTrigger::Activate,
        "doubleclick" => SourceUiEventTrigger::DoubleClick,
        "animationcompleted" => SourceUiEventTrigger::AnimationCompleted,
        "show" => SourceUiEventTrigger::Show,
        "hide" => SourceUiEventTrigger::Hide,
        "on" => SourceUiEventTrigger::On,
        "off" => SourceUiEventTrigger::Off,
        "textchanged" => SourceUiEventTrigger::TextChanged,
        _ => return None,
    })
}

pub(super) fn parse_authored_ui_event(node: &OrderedSourceDocumentNode) -> SourceUiEvent {
    let child = node
        .element_children()
        .find(|node| matches!(node.name.as_str(), "child" | "event"));
    let xml = node
        .element_children()
        .find(|node| node.name == "BFXMLObjEvent");
    let xml_key_value = xml.and_then(|node| {
        node.element_children()
            .find(|node| node.name == "BFKeyValObj")
    });
    SourceUiEvent {
        message: node.attribute("msg").unwrap_or_default().to_owned(),
        data: optional_nonempty_source_attribute(node, "data"),
        string: optional_nonempty_source_attribute(node, "string"),
        value: optional_nonempty_source_attribute(node, "value"),
        key: optional_nonempty_source_attribute(node, "key"),
        val: optional_nonempty_source_attribute(node, "val"),
        event_type: optional_nonempty_source_attribute(node, "type"),
        target_child: optional_nonempty_source_attribute(node, "name"),
        color: node
            .element_children()
            .find(|node| matches!(node.name.as_str(), "BFColor" | "UIColorObj"))
            .and_then(parse_color),
        rect: [
            optional_nonempty_source_attribute(node, "x"),
            optional_nonempty_source_attribute(node, "y"),
            optional_nonempty_source_attribute(node, "w"),
            optional_nonempty_source_attribute(node, "h"),
        ],
        xml_object: xml.map(|xml| SourceUiXmlObjectEvent {
            message_type: optional_nonempty_source_attribute(xml, "msgtype"),
            key: xml_key_value.and_then(|node| optional_nonempty_source_attribute(node, "key")),
            value: xml_key_value.and_then(|node| optional_nonempty_source_attribute(node, "val")),
            payload: SourceUiPayloadNode::from_data_node(xml),
        }),
        child: child.map(parse_authored_ui_event).map(Box::new),
        payload: node
            .element_children()
            .filter(|node| {
                !matches!(
                    node.name.as_str(),
                    "child" | "event" | "BFXMLObjEvent" | "BFColor" | "UIColorObj"
                )
            })
            .map(SourceUiPayloadNode::from_data_node)
            .collect(),
        unknown_attributes: copy_unknown_source_ui_attributes(
            &node.attributes,
            &[
                "msg",
                "data",
                "string",
                "value",
                "key",
                "val",
                "type",
                "name",
                "x",
                "y",
                "w",
                "h",
                "code",
                "char",
                "ctrlState",
                "allowRepeat",
                "locid",
                "file",
                "node",
                "time",
            ],
        ),
    }
}

pub(super) fn parse_authored_ui_hotkeys(node: &OrderedSourceDocumentNode) -> Vec<SourceUiHotkey> {
    node.element_children()
        .filter(|child| child.name != "file")
        .map(|child| {
            let mut event = parse_authored_ui_event(child);
            if child.name == "char" {
                event.key = None;
            }
            SourceUiHotkey {
                trigger: match child.name.as_str() {
                    "down" | "char" => SourceUiHotkeyTrigger::Down,
                    "up" => SourceUiHotkeyTrigger::Up,
                    other => SourceUiHotkeyTrigger::Unknown(other.to_owned()),
                },
                localization_id: optional_nonempty_source_attribute(child, "locid")
                    .or_else(|| optional_nonempty_source_attribute(node, "locid")),
                character: optional_nonempty_source_attribute(child, "char")
                    .or_else(|| optional_nonempty_source_attribute(child, "key"))
                    .or_else(|| optional_nonempty_source_attribute(node, "char"))
                    .or_else(|| optional_nonempty_source_attribute(node, "key")),
                code: child.attribute("code").and_then(|value| value.parse().ok()),
                control_state: optional_nonempty_source_attribute(child, "ctrlState")
                    .or_else(|| optional_nonempty_source_attribute(node, "ctrlState")),
                allow_repeat: child
                    .attribute("allowRepeat")
                    .or_else(|| node.attribute("allowRepeat"))
                    .and_then(parse_source_bool),
                file: child
                    .attribute("file")
                    .or_else(|| node.attribute("file"))
                    .filter(|value| !value.is_empty())
                    .map(AssetPath::new),
                node: optional_nonempty_source_attribute(child, "node")
                    .or_else(|| optional_nonempty_source_attribute(node, "node")),
                event,
            }
        })
        .collect()
}
