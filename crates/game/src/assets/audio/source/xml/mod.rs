use openzt2_game_data::AssetId;

use crate::assets::source_document::ordered_source_document_types::{
    OrderedSourceDocument, OrderedSourceDocumentNode, OrderedSourceDocumentSpan,
};

use super::AudioDocumentDiagnostic;

pub(super) fn descendants(
    root: &OrderedSourceDocumentNode,
) -> impl Iterator<Item = &OrderedSourceDocumentNode> {
    root.element_children().flat_map(descendants_node)
}

pub(super) fn descendants_node(
    node: &OrderedSourceDocumentNode,
) -> Box<dyn Iterator<Item = &OrderedSourceDocumentNode> + '_> {
    Box::new(std::iter::once(node).chain(node.element_children().flat_map(descendants_node)))
}

pub(super) fn is_factory(node: &OrderedSourceDocumentNode) -> bool {
    matches!(
        node.name.as_str(),
        "BF2DSndFactory" | "BF3DSndFactory" | "BFAmbient2DSndFactory" | "BFAmbient3DSndFactory"
    )
}

pub(super) fn water_stage_ids(
    root: &OrderedSourceDocumentNode,
    kind: &str,
) -> [Option<AssetId>; 3] {
    let Some(group) = root.element_children().find(|node| node.name == kind) else {
        return [None; 3];
    };
    let mut stages = [None; 3];
    for (slot, size) in stages.iter_mut().zip(["small", "medium", "large"]) {
        *slot = group
            .element_children()
            .find(|node| node.name == size)
            .and_then(|node| attr(node, "soundStageName"))
            .map(stage_id);
    }
    stages
}

pub(super) fn attr<'a>(node: &'a OrderedSourceDocumentNode, name: &str) -> Option<&'a str> {
    node.attributes
        .iter()
        .find(|attribute| attribute.name().eq_ignore_ascii_case(name))
        .map(|attribute| attribute.value())
}

pub(super) fn required_attr<'a>(
    document: &OrderedSourceDocument,
    node: &'a OrderedSourceDocumentNode,
    name: &str,
) -> Result<&'a str, AudioDocumentDiagnostic> {
    attr(node, name).ok_or_else(|| diagnostic(document, Some(node.span), format!("missing {name}")))
}

pub(super) fn number<T: std::str::FromStr>(
    node: &OrderedSourceDocumentNode,
    name: &str,
) -> Result<Option<T>, AudioDocumentDiagnostic> {
    let Some(authored) = attr(node, name) else {
        return Ok(None);
    };
    let value = authored.trim().trim_end_matches(['f', 'F']).trim();
    if value.is_empty() {
        return Ok(None);
    }
    value
        .parse()
        .map(Some)
        .map_err(|_| AudioDocumentDiagnostic {
            path: "audio-document".into(),
            span: Some(node.span),
            message: format!("invalid {name}: {authored}"),
        })
}

pub(super) fn optional_number<T: std::str::FromStr>(
    node: Option<&OrderedSourceDocumentNode>,
    name: &str,
) -> Result<Option<T>, AudioDocumentDiagnostic> {
    node.map(|node| number(node, name))
        .transpose()
        .map(Option::flatten)
}

pub(super) fn inherited_number<T: std::str::FromStr>(
    node: &OrderedSourceDocumentNode,
    parent: &OrderedSourceDocumentNode,
    name: &str,
) -> Result<Option<T>, AudioDocumentDiagnostic> {
    number(node, name)?.map_or_else(|| number(parent, name), |value| Ok(Some(value)))
}

pub(super) fn inherited_bool(
    node: &OrderedSourceDocumentNode,
    parent: &OrderedSourceDocumentNode,
    name: &str,
) -> Result<bool, AudioDocumentDiagnostic> {
    if attr(node, name).is_some() {
        bool_attr(node, name)
    } else {
        bool_attr(parent, name)
    }
}

pub(super) fn root_number<T: std::str::FromStr>(
    document: &OrderedSourceDocument,
    name: &str,
) -> Result<Option<T>, AudioDocumentDiagnostic> {
    document
        .root
        .attribute(name)
        .map(|value| {
            value
                .trim()
                .trim_end_matches(['f', 'F'])
                .parse()
                .map_err(|_| {
                    diagnostic(
                        document,
                        Some(document.root.span),
                        format!("invalid {name}: {value}"),
                    )
                })
        })
        .transpose()
}

pub(super) fn bool_attr(
    node: &OrderedSourceDocumentNode,
    name: &str,
) -> Result<bool, AudioDocumentDiagnostic> {
    let Some(value) = attr(node, name) else {
        return Ok(false);
    };
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        _ => Err(AudioDocumentDiagnostic {
            path: "audio-document".into(),
            span: Some(node.span),
            message: format!("invalid {name}: {value}"),
        }),
    }
}

pub(super) fn set_flag(flags: &mut u16, mask: u16, enabled: bool) {
    if enabled {
        *flags |= mask;
    }
}

pub(super) fn ordered(left: f32, right: f32) -> [f32; 2] {
    [left.min(right), left.max(right)]
}

pub(super) fn canonical_path(path: &str) -> String {
    path.trim().replace('\\', "/").to_ascii_lowercase()
}

pub(super) fn canonical_token(token: &str) -> String {
    token.trim().to_ascii_lowercase()
}

pub(super) fn cue_id(name: &str) -> AssetId {
    AssetId::from_key(&format!("audio-cue:{}", canonical_token(name)))
}

pub(super) fn stage_id(name: &str) -> AssetId {
    AssetId::from_key(&format!("audio-stage:{}", canonical_token(name)))
}

pub(super) fn soundscape_name(path: &str) -> String {
    canonical_path(path)
        .rsplit('/')
        .next()
        .unwrap_or("default.xml")
        .trim_end_matches(".xml")
        .trim_end_matches("_default")
        .to_owned()
}

pub(super) fn parent_directory_name(path: &str) -> Option<String> {
    let path = canonical_path(path);
    let mut parts = path.rsplit('/');
    parts.next()?;
    parts.next().map(str::to_owned)
}

fn diagnostic(
    document: &OrderedSourceDocument,
    span: Option<OrderedSourceDocumentSpan>,
    message: String,
) -> AudioDocumentDiagnostic {
    AudioDocumentDiagnostic {
        path: document.path.as_str().to_owned(),
        span,
        message,
    }
}
