use bevy::prelude::*;

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreeLevelGraphicsDetail {
    Low,
    Medium,
    High,
}

impl ThreeLevelGraphicsDetail {
    pub(crate) const VALUES: [Self; 3] = [Self::Low, Self::Medium, Self::High];
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum FourLevelGraphicsDetail {
    Low,
    Medium,
    High,
    Ultra,
}

impl FourLevelGraphicsDetail {
    pub(crate) const VALUES: [Self; 4] = [Self::Low, Self::Medium, Self::High, Self::Ultra];
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShadowDetail {
    Off,
    Medium,
    High,
    Ultra,
}

impl ShadowDetail {
    pub(crate) const VALUES: [Self; 4] = [Self::Off, Self::Medium, Self::High, Self::Ultra];
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnvironmentalDetailObjectDensity {
    Off,
    Medium,
    High,
}

impl EnvironmentalDetailObjectDensity {
    pub(crate) const VALUES: [Self; 3] = [Self::Off, Self::Medium, Self::High];
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphicsEffectQuality {
    Low,
    High,
}

#[derive(Resource, serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub struct GraphicsSettings {
    /// Portable Bevy multisampling policy. Source presets use one sample.
    #[serde(rename = "msaa_samples")]
    pub multisample_count: u8,
    pub environment_detail: ThreeLevelGraphicsDetail,
    pub ambient_effects: bool,
    pub skybox_quality: u8,
    pub lighting_detail: ThreeLevelGraphicsDetail,
    pub shadow_detail: ShadowDetail,
    pub specular_effects: bool,
    pub terrain_resolution: ThreeLevelGraphicsDetail,
    pub terrain_texture_size: ThreeLevelGraphicsDetail,
    pub water_detail: FourLevelGraphicsDetail,
    pub detail_objects: EnvironmentalDetailObjectDensity,
    pub object_detail: ThreeLevelGraphicsDetail,
    #[serde(skip, default = "default_object_lod_distance_scales")]
    pub object_lod_distance_scales: [u32; 4],
    pub physical_animations: bool,
    pub prop_attachments: bool,
    pub object_texture_detail: ThreeLevelGraphicsDetail,
    pub texture_replacement: bool,
    pub animation_detail: ThreeLevelGraphicsDetail,
    pub view_distance: ThreeLevelGraphicsDetail,
    pub max_overhead_zoom: ThreeLevelGraphicsDetail,
    pub screen_detail: bool,
    pub ui_text_shadows: bool,
    pub ui_text_textures: bool,
    pub shader_detail: ThreeLevelGraphicsDetail,
    /// Selects the prepared reflection path; it never creates a render target
    /// or compiles a pipeline from settings code.
    pub water_reflections: bool,
    /// Selects the prepared effect capacity/LOD policy.
    pub effects: GraphicsEffectQuality,
    /// Selects the active particle limit from the renderer configuration.
    #[serde(default)]
    pub particle_budget_tier: Option<ThreeLevelGraphicsDetail>,
}

const fn default_object_lod_distance_scales() -> [u32; 4] {
    [1.0_f32.to_bits(); 4]
}

impl GraphicsSettings {
    /// The default model LOD distance group uses lane two.
    #[must_use]
    pub fn object_lod_distance_scale(&self) -> f32 {
        f32::from_bits(self.object_lod_distance_scales[2])
    }

    #[must_use]
    pub fn object_lod_distance_scale_for_group(&self, group: usize) -> f32 {
        self.object_lod_distance_scales
            .get(group)
            .copied()
            .map_or_else(|| self.object_lod_distance_scale(), f32::from_bits)
    }
}

impl Default for GraphicsSettings {
    fn default() -> Self {
        Self {
            multisample_count: 4,
            environment_detail: ThreeLevelGraphicsDetail::High,
            ambient_effects: true,
            skybox_quality: 2,
            lighting_detail: ThreeLevelGraphicsDetail::High,
            shadow_detail: ShadowDetail::Ultra,
            specular_effects: true,
            terrain_resolution: ThreeLevelGraphicsDetail::High,
            terrain_texture_size: ThreeLevelGraphicsDetail::High,
            water_detail: FourLevelGraphicsDetail::Ultra,
            detail_objects: EnvironmentalDetailObjectDensity::High,
            object_detail: ThreeLevelGraphicsDetail::High,
            object_lod_distance_scales: default_object_lod_distance_scales(),
            physical_animations: true,
            prop_attachments: true,
            object_texture_detail: ThreeLevelGraphicsDetail::High,
            texture_replacement: true,
            animation_detail: ThreeLevelGraphicsDetail::High,
            view_distance: ThreeLevelGraphicsDetail::High,
            max_overhead_zoom: ThreeLevelGraphicsDetail::High,
            screen_detail: true,
            ui_text_shadows: true,
            ui_text_textures: true,
            shader_detail: ThreeLevelGraphicsDetail::High,
            water_reflections: true,
            effects: GraphicsEffectQuality::High,
            particle_budget_tier: None,
        }
    }
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReplaceGraphicsSettingsRequest(pub GraphicsSettings);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphicsSettingsRejectionReason {
    UnsupportedMultisampleCount,
}
