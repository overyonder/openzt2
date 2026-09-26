use bevy::prelude::*;

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreeLevelGraphicsDetail {
    Low,
    Medium,
    High,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum FourLevelGraphicsDetail {
    Low,
    Medium,
    High,
    Ultra,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShadowDetail {
    Off,
    Medium,
    High,
    Ultra,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnvironmentalDetailObjectDensity {
    Off,
    Medium,
    High,
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
}

impl GraphicsSettings {
    /// Source `detailLevel = 3`, compiled from the `high` column of the final
    /// expansion `ui/options/settings.xml`.
    pub const fn from_source_highest_detail_preset() -> Self {
        Self {
            multisample_count: 1,
            environment_detail: ThreeLevelGraphicsDetail::High,
            ambient_effects: true,
            skybox_quality: 2,
            lighting_detail: ThreeLevelGraphicsDetail::High,
            shadow_detail: ShadowDetail::Ultra,
            specular_effects: false,
            terrain_resolution: ThreeLevelGraphicsDetail::High,
            terrain_texture_size: ThreeLevelGraphicsDetail::High,
            water_detail: FourLevelGraphicsDetail::Ultra,
            detail_objects: EnvironmentalDetailObjectDensity::Medium,
            object_detail: ThreeLevelGraphicsDetail::High,
            physical_animations: true,
            prop_attachments: true,
            object_texture_detail: ThreeLevelGraphicsDetail::High,
            texture_replacement: true,
            animation_detail: ThreeLevelGraphicsDetail::High,
            view_distance: ThreeLevelGraphicsDetail::High,
            max_overhead_zoom: ThreeLevelGraphicsDetail::High,
            screen_detail: false,
            ui_text_shadows: true,
            ui_text_textures: true,
            shader_detail: ThreeLevelGraphicsDetail::High,
            water_reflections: false,
            effects: GraphicsEffectQuality::High,
        }
    }

    /// Source `detailLevel = 2`, compiled from the `medium` column.
    pub const fn from_source_high_detail_preset() -> Self {
        Self {
            multisample_count: 1,
            environment_detail: ThreeLevelGraphicsDetail::Medium,
            ambient_effects: true,
            skybox_quality: 2,
            lighting_detail: ThreeLevelGraphicsDetail::Medium,
            shadow_detail: ShadowDetail::Medium,
            specular_effects: false,
            terrain_resolution: ThreeLevelGraphicsDetail::Medium,
            terrain_texture_size: ThreeLevelGraphicsDetail::Medium,
            water_detail: FourLevelGraphicsDetail::Medium,
            detail_objects: EnvironmentalDetailObjectDensity::Medium,
            object_detail: ThreeLevelGraphicsDetail::Medium,
            physical_animations: true,
            prop_attachments: true,
            object_texture_detail: ThreeLevelGraphicsDetail::Medium,
            texture_replacement: true,
            animation_detail: ThreeLevelGraphicsDetail::Medium,
            view_distance: ThreeLevelGraphicsDetail::Medium,
            max_overhead_zoom: ThreeLevelGraphicsDetail::Medium,
            screen_detail: false,
            ui_text_shadows: false,
            ui_text_textures: false,
            shader_detail: ThreeLevelGraphicsDetail::Medium,
            water_reflections: false,
            effects: GraphicsEffectQuality::High,
        }
    }

    /// Source `detailLevel = 1`, compiled from the `low` column.
    pub const fn from_source_medium_detail_preset() -> Self {
        Self {
            multisample_count: 1,
            environment_detail: ThreeLevelGraphicsDetail::Low,
            ambient_effects: false,
            skybox_quality: 1,
            lighting_detail: ThreeLevelGraphicsDetail::Low,
            shadow_detail: ShadowDetail::Off,
            specular_effects: false,
            terrain_resolution: ThreeLevelGraphicsDetail::Low,
            terrain_texture_size: ThreeLevelGraphicsDetail::Low,
            water_detail: FourLevelGraphicsDetail::Low,
            detail_objects: EnvironmentalDetailObjectDensity::Off,
            object_detail: ThreeLevelGraphicsDetail::Low,
            physical_animations: false,
            prop_attachments: false,
            object_texture_detail: ThreeLevelGraphicsDetail::Low,
            texture_replacement: false,
            animation_detail: ThreeLevelGraphicsDetail::Low,
            view_distance: ThreeLevelGraphicsDetail::Low,
            max_overhead_zoom: ThreeLevelGraphicsDetail::Low,
            screen_detail: false,
            ui_text_shadows: false,
            ui_text_textures: false,
            shader_detail: ThreeLevelGraphicsDetail::Low,
            water_reflections: false,
            effects: GraphicsEffectQuality::Low,
        }
    }
}

impl Default for GraphicsSettings {
    fn default() -> Self {
        // The shipped profile selects `graphicsDetail="med"` with
        // `detailLevel="2"`. Profile persistence may subsequently override
        // this complete typed resource.
        Self::from_source_high_detail_preset()
    }
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReplaceGraphicsSettingsRequest(pub GraphicsSettings);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphicsSettingsRejectionReason {
    UnsupportedMultisampleCount,
}
