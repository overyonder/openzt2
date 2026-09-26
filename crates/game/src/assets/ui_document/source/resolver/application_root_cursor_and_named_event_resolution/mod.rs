//! Application-root cursor-directory and named-event resolution.

use std::collections::BTreeMap;

use crate::assets::source_document::{
    ordered_source_document_types::OrderedSourceDocumentSpan,
    ui::{
        model::{SourceUiEvent, SourceUiNode, SourceUiWidgetData},
        parser::SourceUiDocument,
    },
};

use super::super::ui_source_document_gap::{
    UiSourceDocumentFamily, UiSourceDocumentGap, UiSourceDocumentGapKind,
};

pub(super) fn resolve_application_root_cursor_and_named_event_semantics(
    sources: &mut BTreeMap<String, SourceUiDocument>,
) -> Result<(), UiSourceDocumentGap> {
    let Some(application) = sources.get("ztapp4.xml") else {
        return Ok(());
    };
    let span = application.root.span;
    let cursor_directory = application
        .cursor_directory
        .as_deref()
        .map(normalize_authored_cursor_directory)
        .transpose()
        .map_err(|message| application_root_gap(span, message))?;
    let mut named_events = BTreeMap::new();
    for list in &application.named_event_lists {
        if list.events.is_empty() {
            return Err(application_root_gap(
                span,
                format!("named UI event list {:?} is empty", list.name),
            ));
        }
        if named_events
            .insert(list.name.clone(), list.events.clone())
            .is_some()
        {
            return Err(application_root_gap(
                span,
                format!("named UI event list {:?} is duplicated", list.name),
            ));
        }
    }

    for source in sources.values_mut() {
        if let Some(directory) = cursor_directory.as_deref() {
            resolve_relative_cursor_paths_in_ui_source_node(&mut source.root, directory);
        }
        expand_named_events_in_ui_source_node(&mut source.root, &named_events, &mut Vec::new())
            .map_err(|message| application_root_gap(span, message))?;
        source.cursor_directory = None;
        source.named_event_lists.clear();
    }
    Ok(())
}

fn normalize_authored_cursor_directory(value: &str) -> Result<String, String> {
    let replaced = value.trim().replace('\\', "/");
    let relative = replaced
        .strip_prefix("../data/")
        .or_else(|| replaced.strip_prefix("data/"))
        .unwrap_or(&replaced)
        .trim_matches('/');
    let parts = relative.split('/').collect::<Vec<_>>();
    if parts.is_empty()
        || parts
            .iter()
            .any(|part| part.is_empty() || matches!(*part, "." | ".."))
    {
        return Err(format!("invalid authored cursor directory {value:?}"));
    }
    Ok(parts.join("/"))
}

fn resolve_relative_cursor_paths_in_ui_source_node(node: &mut SourceUiNode, directory: &str) {
    let resolve = |value: &mut String| {
        if !value.contains('/') && !value.contains('\\') {
            *value = format!("{directory}/{value}");
        }
    };
    if let Some(cursor) = &mut node.cursor {
        resolve(cursor);
    }
    if let SourceUiWidgetData::Globe(globe) = &mut node.widget {
        if let Some(cursor) = &mut globe.dot_highlight_cursor {
            resolve(cursor);
        }
    }
    for event in node.events.iter_mut().flat_map(|block| &mut block.events) {
        resolve_relative_cursor_path_in_ui_source_event(event, directory);
    }
    node.children.iter_mut().for_each(|child| {
        resolve_relative_cursor_paths_in_ui_source_node(child, directory);
    });
}

fn resolve_relative_cursor_path_in_ui_source_event(event: &mut SourceUiEvent, directory: &str) {
    if event.message == "UI_SETCURSOR" {
        if let Some(cursor) = &mut event.string {
            if !cursor.contains('/') && !cursor.contains('\\') {
                *cursor = format!("{directory}/{cursor}");
            }
        }
    }
    if let Some(child) = &mut event.child {
        resolve_relative_cursor_path_in_ui_source_event(child, directory);
    }
}

fn expand_named_events_in_ui_source_node(
    node: &mut SourceUiNode,
    registry: &BTreeMap<String, Vec<SourceUiEvent>>,
    stack: &mut Vec<String>,
) -> Result<(), String> {
    for block in &mut node.events {
        block.events = block
            .events
            .drain(..)
            .map(|event| expand_named_ui_source_event(event, registry, stack))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .flatten()
            .collect();
    }
    node.children
        .iter_mut()
        .try_for_each(|child| expand_named_events_in_ui_source_node(child, registry, stack))
}

fn expand_named_ui_source_event(
    mut event: SourceUiEvent,
    registry: &BTreeMap<String, Vec<SourceUiEvent>>,
    stack: &mut Vec<String>,
) -> Result<Vec<SourceUiEvent>, String> {
    if event.message == "UI_SEND_NAMED_EVENTS" {
        let name = event
            .string
            .as_deref()
            .ok_or_else(|| "UI_SEND_NAMED_EVENTS has no authored list name".to_owned())?;
        if stack.iter().any(|active| active == name) {
            return Err(format!("named UI event list cycle includes {name:?}"));
        }
        let definitions = registry
            .get(name)
            .ok_or_else(|| format!("UI_SEND_NAMED_EVENTS refers to absent list {name:?}"))?
            .clone();
        stack.push(name.to_owned());
        let expanded = definitions
            .into_iter()
            .map(|definition| expand_named_ui_source_event(definition, registry, stack))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .flatten()
            .collect();
        stack.pop();
        return Ok(expanded);
    }
    if let Some(child) = event.child.take() {
        let mut expanded = expand_named_ui_source_event(*child, registry, stack)?;
        if expanded.len() != 1 {
            return Err("a nested named UI event expanded to more than one child event".to_owned());
        }
        event.child = expanded.pop().map(Box::new);
    }
    Ok(vec![event])
}

fn application_root_gap(span: OrderedSourceDocumentSpan, message: String) -> UiSourceDocumentGap {
    UiSourceDocumentGap {
        family: UiSourceDocumentFamily::Ui,
        kind: UiSourceDocumentGapKind::UnsupportedVocabulary,
        virtual_path: "ztapp4.xml".into(),
        span,
        message,
    }
}
