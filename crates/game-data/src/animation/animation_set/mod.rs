/// One actor/model-specific authored animation library.
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct AuthoredAnimationSetDocument {
    pub model_asset_path: String,
    pub skeleton_asset_path: String,
    pub animation_clips: Vec<AuthoredAnimationSetClipReference>,
    pub animation_graph_nodes: Vec<AuthoredAnimationGraphNode>,
    pub animation_graph_order: Vec<String>,
    pub animation_graph_edges: Vec<AuthoredAnimationGraphEdge>,
    pub animation_graph_metadata: Option<AuthoredAnimationGraphMetadata>,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct AuthoredAnimationSetClipReference {
    /// Actor-local authored animation name used by behavior and graph nodes.
    pub animation_clip_asset_key: String,
    /// Reusable clip-source asset, independent of this actor's graph.
    pub animation_clip_asset_path: String,
    pub playback_policy: AuthoredAnimationPlaybackPolicy,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct AuthoredAnimationGraphEdge {
    pub source_animation_graph_node_asset_key: String,
    pub target_animation_graph_node_asset_key: String,
    pub transition_animation_clip_asset_keys: Vec<String>,
    pub authored_animation_attributes: Vec<AuthoredAnimationAttribute>,
}

#[derive(Clone, Debug, Default, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct AuthoredAnimationPlaybackPolicy {
    pub playback_rate: Option<f32>,
    pub load_during_animation_set_load: Option<bool>,
    pub explicit_use_only: Option<bool>,
    pub resolve_unit_collisions: Option<bool>,
    pub ground_fit_rotation_is_enabled: Option<bool>,
    pub spine_bending_is_allowed: Option<bool>,
    pub angular_speed_by_axis: [Option<f32>; 3],
    pub authored_animation_attributes: Vec<AuthoredAnimationAttribute>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct AuthoredAnimationGraphNode {
    pub animation_graph_node_asset_key: String,
    pub animation_clip_asset_keys: Vec<String>,
    pub authored_animation_attributes: Vec<AuthoredAnimationAttribute>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct AuthoredAnimationGraphMetadata {
    pub authored_graph_name: Option<String>,
    pub authored_graph_version: Option<u32>,
    pub authored_animation_attributes: Vec<AuthoredAnimationAttribute>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct AuthoredAnimationAttribute {
    pub attribute_name: String,
    pub attribute_value: String,
}
