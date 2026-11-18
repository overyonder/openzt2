use std::io;

use bevy::{
    asset::{AssetLoader, LoadContext, io::Reader},
    prelude::*,
};

use crate::{
    graphics_settings_types::{
        EnvironmentalDetailObjectDensity, FourLevelGraphicsDetail, GraphicsEffectQuality,
        GraphicsSettings, ShadowDetail, ThreeLevelGraphicsDetail,
    },
    source_document::{
        blue_fang_source_document_parsing::parse_blue_fang_source_document,
        ordered_source_document_types::OrderedSourceDocumentNode, path::AssetPath,
        ui::parser::source_scalar_and_attribute_reading::parse_source_bool,
    },
};

const SETTINGS_SOURCE_PATH: &str = "ui/options/settings.xml";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GraphicsDetailPreset {
    Medium,
    High,
    Highest,
}

#[derive(Asset, TypePath)]
pub struct GraphicsSettingsPresets([GraphicsSettings; 3]);

impl GraphicsSettingsPresets {
    pub fn settings(
        &self,
        preset: GraphicsDetailPreset,
        multisample_count: u8,
    ) -> GraphicsSettings {
        GraphicsSettings {
            multisample_count,
            ..self.0[preset as usize]
        }
    }
}

#[derive(Resource)]
pub struct GraphicsSettingsPresetSource(Handle<GraphicsSettingsPresets>);

impl GraphicsSettingsPresetSource {
    pub fn get<'a>(
        &self,
        assets: &'a Assets<GraphicsSettingsPresets>,
    ) -> Option<&'a GraphicsSettingsPresets> {
        assets.get(&self.0)
    }
}

#[derive(Default, TypePath)]
struct GraphicsSettingsPresetLoader;

impl AssetLoader for GraphicsSettingsPresetLoader {
    type Asset = GraphicsSettingsPresets;
    type Settings = ();
    type Error = io::Error;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _: &Self::Settings,
        _: &mut LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        let document =
            parse_blue_fang_source_document(AssetPath::new(SETTINGS_SOURCE_PATH), &bytes)
                .map_err(io::Error::other)?;
        let settings = document
            .root
            .element_children()
            .find(|child| child.name.eq_ignore_ascii_case("settings"))
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "graphics settings table has no settings rows",
                )
            })?;
        Ok(GraphicsSettingsPresets([
            lower_graphics_settings_preset(settings, "low")?,
            lower_graphics_settings_preset(settings, "medium")?,
            lower_graphics_settings_preset(settings, "high")?,
        ]))
    }

    fn extensions(&self) -> &[&str] {
        &["xml"]
    }
}

fn lower_graphics_settings_preset(
    settings: &OrderedSourceDocumentNode,
    column: &str,
) -> io::Result<GraphicsSettings> {
    use ThreeLevelGraphicsDetail::{High, Low, Medium};
    let mut result = GraphicsSettings {
        water_reflections: false,
        ..GraphicsSettings::default()
    };
    for row in settings.element_children() {
        let Some(index) = row.attribute_named_any(&[column]) else {
            continue;
        };
        let name = row.name.to_ascii_lowercase();
        let invalid = || {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "invalid graphics preset setting {}={index:?} for {column}",
                    row.name
                ),
            )
        };
        let index = index.parse::<usize>().map_err(|_| invalid())?;
        let detail = || [Low, Medium, High].get(index).copied().ok_or_else(invalid);
        let four_levels = || {
            [
                FourLevelGraphicsDetail::Low,
                FourLevelGraphicsDetail::Medium,
                FourLevelGraphicsDetail::High,
                FourLevelGraphicsDetail::Ultra,
            ]
            .get(index)
            .copied()
            .ok_or_else(invalid)
        };
        let flag = || [false, true].get(index).copied().ok_or_else(invalid);
        match name.as_str() {
            "environmentdetail" => result.environment_detail = detail()?,
            "lightingdetail" => result.lighting_detail = detail()?,
            "shadowdetail" => {
                result.shadow_detail = [
                    ShadowDetail::Off,
                    ShadowDetail::Medium,
                    ShadowDetail::High,
                    ShadowDetail::Ultra,
                ]
                .get(index)
                .copied()
                .ok_or_else(invalid)?
            }
            "speculareffects" => result.specular_effects = flag()?,
            "terrainresolution" => result.terrain_resolution = detail()?,
            "terraintexturesize" => result.terrain_texture_size = detail()?,
            "waterdetail" => result.water_detail = four_levels()?,
            "detailobjects" => {
                result.detail_objects = [
                    EnvironmentalDetailObjectDensity::Off,
                    EnvironmentalDetailObjectDensity::Medium,
                    EnvironmentalDetailObjectDensity::High,
                ]
                .get(index)
                .copied()
                .ok_or_else(invalid)?
            }
            "objectdetail" => result.object_detail = detail()?,
            "objecttexturedetail" => result.object_texture_detail = detail()?,
            "animationdetail" => result.animation_detail = detail()?,
            "viewdistance" => result.view_distance = detail()?,
            "maxoverheadzoom" => result.max_overhead_zoom = detail()?,
            "screendetail" => result.screen_detail = flag()?,
            "uitextshadows" => result.ui_text_shadows = flag()?,
            "uitexttextures" => result.ui_text_textures = flag()?,
            "shaderdetail" => result.shader_detail = detail()?,
            "waterreflections" => result.water_reflections = flag()?,
            _ => {}
        }
        if let Some(setting) = row
            .element_children()
            .filter(|child| child.name.eq_ignore_ascii_case("setting"))
            .nth(index)
        {
            for field in setting.element_children() {
                let Some(value) = field.attribute_named_any(&["value"]) else {
                    continue;
                };
                let boolean = || parse_source_bool(value).ok_or_else(invalid);
                match field.name.to_ascii_lowercase().as_str() {
                    "ambientson" => result.ambient_effects = boolean()?,
                    "skyboxquality" => {
                        result.skybox_quality = value.parse().map_err(|_| invalid())?
                    }
                    "physanimon" => result.physical_animations = boolean()?,
                    "propattachon" => result.prop_attachments = boolean()?,
                    "texturereplaceon" => result.texture_replacement = boolean()?,
                    "waterreflectionson" => result.water_reflections = boolean()?,
                    _ => {}
                }
            }
        }
    }
    result.effects = if result.environment_detail == Low {
        GraphicsEffectQuality::Low
    } else {
        GraphicsEffectQuality::High
    };
    Ok(result)
}

pub(super) struct GraphicsSettingsPresetAssetPlugin;

impl Plugin for GraphicsSettingsPresetAssetPlugin {
    fn build(&self, application: &mut App) {
        application
            .init_asset::<GraphicsSettingsPresets>()
            .init_asset_loader::<GraphicsSettingsPresetLoader>()
            .add_systems(Startup, request_graphics_settings_presets);
    }
}

fn request_graphics_settings_presets(mut commands: Commands, server: Res<AssetServer>) {
    commands.insert_resource(GraphicsSettingsPresetSource(
        server.load(SETTINGS_SOURCE_PATH),
    ));
}
