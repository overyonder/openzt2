#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct AuthoredAnimationTextKey {
    pub authored_frame_number: Option<f32>,
    pub authored_time_seconds: Option<f32>,
    pub authored_playback_percentage: Option<f32>,
    pub authored_text: String,
    pub animation_text_commands: Vec<AuthoredAnimationTextCommand>,
}

#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct AuthoredAnimationTextCommand {
    pub target_skeleton_joint_asset_key: Option<String>,
    pub animation_text_action: AuthoredAnimationTextAction,
}

#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum AuthoredAnimationTextAction {
    PlaySound {
        audio_asset_key: String,
        use_terrain_surface_sound: bool,
        playback_is_looped: bool,
        update_sound_position_during_playback: bool,
    },
    RunParticleSystem {
        particle_system_asset_key: String,
        use_target_z_axis_rotation: bool,
        particle_scale: Option<f32>,
    },
    TriggerSurfaceEffect {
        surface_effect_kind: AuthoredAnimationSurfaceEffectKind,
        effect_strength: Option<f32>,
        named_surface_effect_asset_key: Option<String>,
    },
    AttachPendingObject,
    AttachNamedObject {
        object_asset_key: String,
    },
    DetachObject,
    SpawnObject,
    CreateObject {
        object_asset_key: String,
    },
    DestroyObject {
        object_asset_key: String,
    },
    PlayAnimationGraphNode {
        animation_graph_node_asset_key: String,
        blend_duration_seconds: Option<f32>,
        advance_current_animation_time: bool,
        enter_immediately: bool,
    },
    SetAnimationGraphEnabled(bool),
    ExitAnimation,
    SetGroundFit(i32),
    SetDockControllerEnabled(bool),
    GoToDockControllerNode(i32),
    SendArtificialIntelligenceCommand(String),
    SetAnimationPlaybackSpeed {
        playback_speed_percent: i32,
        authored_speed_mode: i32,
    },
    ApplyWhapImpulse {
        impulse_angle_degrees: i32,
        impulse_duration_frames: i32,
    },
    RunPresentationEffect {
        presentation_effect_asset_key: String,
        attach_to_target: bool,
        stop_attached_effects: bool,
        disable_distance_culling: bool,
        water_height_offset: Option<f32>,
    },
    AttachParticleSystem {
        particle_system_asset_key: String,
        use_target_rotation: bool,
    },
    DetachParticleSystem {
        particle_system_asset_key: String,
    },
    KillAnimationSubject,
    RequestTraversalTransition {
        animation_graph_node_asset_key: String,
    },
    CreateObjectOverride {
        object_asset_key: String,
    },
    RequestAnimationGraphNode(String),
    StartAlignedAnimation {
        animation_graph_node_asset_key: String,
        authored_actor_range: String,
        authored_alignment_axes: String,
        authored_front_axis: String,
        playback_is_looped: bool,
    },
    UnrecognizedAuthoredCommand(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub enum AuthoredAnimationSurfaceEffectKind {
    DirectionalVector,
    SurfaceImpact,
    DirectionalVectorAndSurfaceImpact,
    NamedSurfaceEffect,
}
