use bevy::prelude::*;

/// One row of the source `ui/options/settings.xml` custom detail list, in
/// authored order. Derived fields such as `ambient_effects` follow their
/// parent row, as the source `<setting>` children do.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum GraphicsDetailSetting {
    EnvironmentDetail,
    LightingDetail,
    ShadowDetail,
    SpecularEffects,
    TerrainResolution,
    TerrainTextureSize,
    WaterDetail,
    DetailObjects,
    ObjectDetail,
    ObjectTextureDetail,
    AnimationDetail,
    ViewDistance,
    MaxOverheadZoom,
    ScreenDetail,
    UiTextShadows,
    UiTextTextures,
    ShaderDetail,
}

impl GraphicsDetailSetting {
    pub(super) const ALL: [Self; 17] = [
        Self::EnvironmentDetail,
        Self::LightingDetail,
        Self::ShadowDetail,
        Self::SpecularEffects,
        Self::TerrainResolution,
        Self::TerrainTextureSize,
        Self::WaterDetail,
        Self::DetailObjects,
        Self::ObjectDetail,
        Self::ObjectTextureDetail,
        Self::AnimationDetail,
        Self::ViewDistance,
        Self::MaxOverheadZoom,
        Self::ScreenDetail,
        Self::UiTextShadows,
        Self::UiTextTextures,
        Self::ShaderDetail,
    ];

    /// The source `labelLocID`.
    pub(super) const fn label_localization_key(self) -> &'static str {
        match self {
            Self::EnvironmentDetail => "options:environmentdetail",
            Self::LightingDetail => "options:lightingdetail",
            Self::ShadowDetail => "options:enableshadows",
            Self::SpecularEffects => "options:speculareffects",
            Self::TerrainResolution => "options:terrainresolution",
            Self::TerrainTextureSize => "options:terraintexturesize",
            Self::WaterDetail => "options:waterdetail",
            Self::DetailObjects => "options:enabledetailobjects",
            Self::ObjectDetail => "options:objectdetail",
            Self::ObjectTextureDetail => "options:objecttexturedetail",
            Self::AnimationDetail => "options:animationdetail",
            Self::ViewDistance => "options:viewdistance",
            Self::MaxOverheadZoom => "options:overheadcameradistance",
            Self::ScreenDetail => "options:screendetail",
            Self::UiTextShadows => "options:fontshadows",
            Self::UiTextTextures => "options:fonttexture",
            Self::ShaderDetail => "options:shaderdetail",
        }
    }

    /// The source `<setting locid>` of each slider position.
    pub(super) const fn level_localization_keys(self) -> &'static [&'static str] {
        const LOW_MEDIUM_HIGH: &[&str] = &["options:low", "options:medium", "options:high"];
        const OFF_MEDIUM_HIGH: &[&str] = &["options:off", "options:medium", "options:high"];
        const DISABLE_ENABLE: &[&str] = &["options:disable", "options:enable"];
        match self {
            Self::ShadowDetail => &[
                "options:off",
                "options:medium",
                "options:high",
                "options:ultra",
            ],
            Self::WaterDetail => &[
                "options:low",
                "options:medium",
                "options:high",
                "options:ultra",
            ],
            Self::DetailObjects => OFF_MEDIUM_HIGH,
            Self::SpecularEffects
            | Self::ScreenDetail
            | Self::UiTextShadows
            | Self::UiTextTextures => DISABLE_ENABLE,
            Self::EnvironmentDetail
            | Self::LightingDetail
            | Self::TerrainResolution
            | Self::TerrainTextureSize
            | Self::ObjectDetail
            | Self::ObjectTextureDetail
            | Self::AnimationDetail
            | Self::ViewDistance
            | Self::MaxOverheadZoom
            | Self::ShaderDetail => LOW_MEDIUM_HIGH,
        }
    }
}
