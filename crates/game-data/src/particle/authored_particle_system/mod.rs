//! Particle-effect data loaded from a PSYS document.

use crate::AssetId;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthoredParticleSystemDocument {
    pub effect_id: AssetId,
    pub emitters: Vec<AuthoredParticleSystemEmitter>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthoredParticleSystemEmitter {
    pub emitter_id: AssetId,
    pub capacity: u32,
    pub spawn: AuthoredParticleSpawnPolicy,
    pub clock: ParticleEffectSimulationClock,
    pub shape: AuthoredParticleEmitterShape,
    pub material: String,
    pub modifiers: Vec<AuthoredParticleSystemModifier>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AuthoredParticleSpawnPolicy {
    Rate { per_second: f32 },
    Burst { count: u32 },
    Once,
    Curve { looped: bool },
    Manual,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ParticleEffectSimulationClock {
    Presentation,
    ZooSimulation,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum AuthoredParticleEmitterShape {
    Point,
    Sphere {
        radius: f32,
    },
    Box {
        half_extents: [f32; 3],
    },
    Circle {
        radius: f32,
    },
    Cone {
        origin: [f32; 3],
        direction: [f32; 3],
        up: [f32; 3],
        out: [f32; 3],
        min_radius: f32,
        max_radius: f32,
    },
    Decal,
    Proxy,
    Mesh,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AuthoredParticleRendererKind {
    Billboard,
    VelocityAligned,
    Decal,
    Mesh,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AuthoredParticleOverflowPolicy {
    DropNewest,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct AuthoredParticleCurvePoint {
    pub time: f32,
    pub value: [f32; 4],
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum AuthoredParticleSystemModifier {
    InitialPosition {
        lifetime_seconds: f32,
    },
    InitialVelocity {
        velocity: [f32; 3],
    },
    Acceleration {
        acceleration: [f32; 3],
    },
    Drag {
        coefficient: f32,
    },
    ColorOverLifetime {
        points: Vec<AuthoredParticleCurvePoint>,
    },
    SizeOverLifetime {
        points: Vec<AuthoredParticleCurvePoint>,
    },
    KillSphere {
        center: [f32; 3],
        radius: f32,
        kill_inside: bool,
    },
    OrientToVelocity,
    BirthRateCurve {
        points: Vec<AuthoredParticleCurvePoint>,
        looped: bool,
    },
    LifetimeRange {
        min_seconds: f32,
        max_seconds: f32,
    },
    InitialVelocityCone {
        base: [f32; 3],
        direction: [f32; 3],
        vertical_direction: f32,
        vertical_angle: f32,
        horizontal_direction: f32,
        horizontal_angle: f32,
        min_speed: f32,
        max_speed: f32,
    },
    InitialColor {
        rgba: [f32; 4],
    },
    InitialSizeRange {
        min: f32,
        max: f32,
    },
    DirectionalGravity {
        direction: [f32; 3],
        acceleration: f32,
    },
    PointGravity {
        position: [f32; 3],
        force: f32,
        decay: f32,
    },
    Spin {
        min_rate: f32,
        max_rate: f32,
    },
    AngularDrag {
        coefficient: f32,
    },
    FlutterVelocity {
        acceleration: f32,
    },
    FlutterPosition {
        acceleration: f32,
    },
    Drift {
        velocity: [f32; 3],
    },
    KillPlane {
        normal: [f32; 3],
        distance: f32,
    },
    SpawnOnPlane {
        normal: [f32; 3],
        distance: f32,
        probability: f32,
        min_count: u32,
        max_count: u32,
        target_emitter: AssetId,
        radius: f32,
        speed: f32,
        velocity_out: [f32; 3],
        velocity_up: [f32; 3],
        velocity_direction: [f32; 3],
        velocity_scale: [f32; 3],
        offset: f32,
    },
    TileRandomizer {
        columns: u16,
        rows: u16,
    },
    DecalUv {
        minimum: f32,
    },
    EmitWindow {
        start_seconds: f32,
        stop_seconds: f32,
        frequency: f32,
        phase: f32,
    },
    Renderer {
        kind: AuthoredParticleRendererKind,
        width: f32,
        height: f32,
        columns: u16,
        rows: u16,
        sort_by_distance: bool,
        minimum_height: f32,
    },
    DieWhenEmpty {
        enabled: bool,
    },
    Overflow {
        policy: AuthoredParticleOverflowPolicy,
    },
}
