#[derive(Clone, Copy, Debug, Default, serde::Deserialize, serde::Serialize)]
pub struct BehaviorSpawnPosePolicy {
    pub random_position_offset_m: f32,
    pub random_rotation_offset_radians: f32,
}
