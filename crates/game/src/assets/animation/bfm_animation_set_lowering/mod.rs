use std::io;

use openzt2_game_data::animation::animation_set::{
    AuthoredAnimationAttribute, AuthoredAnimationGraphEdge, AuthoredAnimationGraphMetadata,
    AuthoredAnimationGraphNode, AuthoredAnimationPlaybackPolicy, AuthoredAnimationSetClipReference,
    AuthoredAnimationSetDocument,
};

use crate::assets::source_document::{
    blue_fang_source_document_parsing::parse_blue_fang_source_document,
    ordered_source_document_types::OrderedSourceDocumentNode, path::AssetPath,
};

pub(super) fn lower_bfm_source_to_animation_set_document(
    animation_set_asset_path: &str,
    bfm_source_bytes: &[u8],
) -> io::Result<AuthoredAnimationSetDocument> {
    let bfm_source_document =
        parse_blue_fang_source_document(AssetPath::new(animation_set_asset_path), bfm_source_bytes)
            .map_err(|source_error| io::Error::new(io::ErrorKind::InvalidData, source_error))?;
    let bfm_root = bfm_source_document
        .root
        .element_children()
        .find(|source_node| source_node.name.eq_ignore_ascii_case("BFM"))
        .ok_or_else(missing_bfm_model_animation_set)?;
    let model_asset_path = bfm_root
        .attribute("modelname")
        .map(|authored_model_asset_path| AssetPath::new(authored_model_asset_path).key())
        .ok_or_else(missing_bfm_model_animation_set)?;
    let animation_clips = bfm_root
        .element_children()
        .filter(|source_node| source_node.name.eq_ignore_ascii_case("animation"))
        .filter_map(|animation_source_node| {
            Some(AuthoredAnimationSetClipReference {
                animation_clip_asset_key: animation_source_node.attribute("animName")?.to_owned(),
                animation_clip_asset_path: AssetPath::new(animation_source_node.attribute("anim")?)
                    .key(),
                playback_policy: lower_bfm_animation_playback_policy(animation_source_node),
            })
        })
        .collect::<Vec<_>>();

    let authored_graph_source = (!animation_clips.is_empty())
        .then(|| {
            bfm_source_document
                .root
                .element_children()
                .find(|source_node| source_node.name.eq_ignore_ascii_case("Graph"))
        })
        .flatten();
    let animation_graph_order = authored_graph_source
        .into_iter()
        .flat_map(OrderedSourceDocumentNode::element_children)
        .filter(|source_node| source_node.name.eq_ignore_ascii_case("node"))
        .filter_map(|source_node| source_node.attribute("name").map(str::to_owned))
        .collect();
    let animation_graph_nodes = authored_graph_source
        .into_iter()
        .flat_map(OrderedSourceDocumentNode::element_children)
        .filter(|source_node| source_node.name.eq_ignore_ascii_case("node"))
        .filter_map(lower_bfm_animation_graph_node)
        .collect();
    let animation_graph_edges = authored_graph_source
        .into_iter()
        .flat_map(OrderedSourceDocumentNode::element_children)
        .filter(|source_node| source_node.name.eq_ignore_ascii_case("node"))
        .flat_map(lower_bfm_animation_graph_node_edges)
        .collect();
    let animation_graph_metadata =
        authored_graph_source.map(|graph_source_node| AuthoredAnimationGraphMetadata {
            authored_graph_name: graph_source_node.attribute("name").map(str::to_owned),
            authored_graph_version: graph_source_node
                .attribute("version")
                .and_then(|authored_version| authored_version.parse().ok()),
            authored_animation_attributes: collect_unrecognized_bfm_animation_attributes(
                graph_source_node,
                &["name", "version"],
            ),
        });

    Ok(AuthoredAnimationSetDocument {
        model_asset_path: model_asset_path.clone(),
        skeleton_asset_path: model_asset_path,
        animation_clips,
        animation_graph_nodes,
        animation_graph_order,
        animation_graph_edges,
        animation_graph_metadata,
    })
}

fn missing_bfm_model_animation_set() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "BFM has no model animation set")
}

fn lower_bfm_animation_graph_node(
    graph_node_source: &OrderedSourceDocumentNode,
) -> Option<AuthoredAnimationGraphNode> {
    let animation_graph_node_asset_key = graph_node_source.attribute("name")?.to_owned();
    Some(AuthoredAnimationGraphNode {
        animation_graph_node_asset_key,
        animation_clip_asset_keys: collect_bfm_animation_graph_table_members(graph_node_source),
        authored_animation_attributes: collect_unrecognized_bfm_animation_attributes(
            graph_node_source,
            &["name"],
        ),
    })
}

fn lower_bfm_animation_graph_node_edges(
    graph_node_source: &OrderedSourceDocumentNode,
) -> impl Iterator<Item = AuthoredAnimationGraphEdge> + '_ {
    let source_animation_graph_node_asset_key = graph_node_source.attribute("name");
    graph_node_source
        .element_children()
        .filter(|source_node| source_node.name.eq_ignore_ascii_case("edge"))
        .filter_map(move |edge_source_node| {
            Some(AuthoredAnimationGraphEdge {
                source_animation_graph_node_asset_key: source_animation_graph_node_asset_key?
                    .to_owned(),
                target_animation_graph_node_asset_key: edge_source_node
                    .attribute("name")?
                    .to_owned(),
                transition_animation_clip_asset_keys: collect_bfm_animation_graph_table_members(
                    edge_source_node,
                ),
                authored_animation_attributes: collect_unrecognized_bfm_animation_attributes(
                    edge_source_node,
                    &["name"],
                ),
            })
        })
}

fn collect_bfm_animation_graph_table_members(
    source_node: &OrderedSourceDocumentNode,
) -> Vec<String> {
    source_node
        .element_children()
        .filter(|child_source_node| child_source_node.name.eq_ignore_ascii_case("table"))
        .flat_map(OrderedSourceDocumentNode::element_children)
        .map(|member_source_node| member_source_node.name.to_string())
        .collect()
}

fn lower_bfm_animation_playback_policy(
    animation_source_node: &OrderedSourceDocumentNode,
) -> AuthoredAnimationPlaybackPolicy {
    AuthoredAnimationPlaybackPolicy {
        playback_rate: animation_source_node
            .attribute("animSpeed")
            .and_then(parse_blue_fang_animation_number),
        load_during_animation_set_load: animation_source_node
            .attribute("load")
            .and_then(parse_blue_fang_animation_boolean),
        explicit_use_only: animation_source_node
            .attribute("explicitUseOnly")
            .and_then(parse_blue_fang_animation_boolean),
        resolve_unit_collisions: animation_source_node
            .attribute("resolveUnitCollisions")
            .and_then(parse_blue_fang_animation_boolean),
        ground_fit_rotation_is_enabled: animation_source_node
            .attribute("groundFitRotation")
            .and_then(parse_blue_fang_animation_boolean),
        spine_bending_is_allowed: animation_source_node
            .attribute("allowSpineBending")
            .and_then(parse_blue_fang_animation_boolean),
        angular_speed_by_axis: ["yawSpeed", "pitchSpeed", "rollSpeed"].map(|attribute_name| {
            animation_source_node
                .attribute(attribute_name)
                .and_then(parse_blue_fang_animation_number)
        }),
        authored_animation_attributes: collect_unrecognized_bfm_animation_attributes(
            animation_source_node,
            &[
                "anim",
                "animName",
                "animSpeed",
                "load",
                "explicitUseOnly",
                "resolveUnitCollisions",
                "groundFitRotation",
                "allowSpineBending",
                "yawSpeed",
                "pitchSpeed",
                "rollSpeed",
            ],
        ),
    }
}

fn collect_unrecognized_bfm_animation_attributes(
    source_node: &OrderedSourceDocumentNode,
    recognized_attribute_names: &[&str],
) -> Vec<AuthoredAnimationAttribute> {
    source_node
        .attributes
        .iter()
        .filter(|source_attribute| {
            !recognized_attribute_names.iter().any(|recognized_name| {
                source_attribute
                    .name()
                    .eq_ignore_ascii_case(recognized_name)
            })
        })
        .map(|source_attribute| AuthoredAnimationAttribute {
            attribute_name: source_attribute.name().to_owned(),
            attribute_value: source_attribute.value().to_owned(),
        })
        .collect()
}

fn parse_blue_fang_animation_number(authored_value: &str) -> Option<f32> {
    authored_value
        .trim()
        .trim_end_matches(['f', 'F'])
        .parse()
        .ok()
}

fn parse_blue_fang_animation_boolean(authored_value: &str) -> Option<bool> {
    match authored_value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" => Some(true),
        "0" | "false" | "no" => Some(false),
        _ => None,
    }
}
