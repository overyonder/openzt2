use bevy::ecs::schedule::SystemCondition;
use openzt2_game_data::{ui_document::node::UiNodeHelpDefinition, AssetId};
use std::{io, path::Path};

use bevy::{
    asset::{
        io::{AssetReaderError, Reader},
        AssetLoader, LoadContext, ReadAssetBytesError,
    },
    prelude::*,
};

use crate::{
    asset_source::{
        archive_entry_asset_path::{archive_entry_asset_path, ARCHIVE_ENTRY_ASSET_EXTENSION},
        AssetArchives,
    },
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
const RENDERER_SOURCE_PATH: &str = "config/graphics.xml";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GraphicsDetailPreset {
    Medium,
    High,
    Highest,
}

#[derive(Asset, TypePath)]
pub struct GraphicsSettingsPresets {
    settings: [GraphicsSettings; 3],
    detail_rows: Vec<GraphicsDetailRow>,
    particle_budgets: [Option<u32>; 3],
    renderer_budget_tier: ThreeLevelGraphicsDetail,
}

impl GraphicsSettingsPresets {
    #[cfg(feature = "testing")]
    pub fn from_test_source(bytes: &[u8]) -> io::Result<Self> {
        parse_graphics_settings_presets(bytes)
    }

    pub fn detail_rows(&self) -> &[GraphicsDetailRow] {
        &self.detail_rows
    }

    pub fn settings(
        &self,
        preset: GraphicsDetailPreset,
        multisample_count: u8,
    ) -> GraphicsSettings {
        GraphicsSettings {
            multisample_count,
            ..self.settings[preset as usize]
        }
    }

    pub fn particle_budget(&self, tier: Option<ThreeLevelGraphicsDetail>) -> Option<u32> {
        self.particle_budgets[tier.unwrap_or(self.renderer_budget_tier) as usize]
    }
}

#[derive(Resource, Default)]
pub struct GraphicsSettingsPresetSource(Option<Handle<GraphicsSettingsPresets>>);

impl GraphicsSettingsPresetSource {
    pub fn get<'a>(
        &self,
        assets: &'a Assets<GraphicsSettingsPresets>,
    ) -> Option<&'a GraphicsSettingsPresets> {
        assets.get(self.0.as_ref()?)
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
        load_context: &mut LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        let mut presets = parse_graphics_settings_presets(&bytes)?;
        match load_context.read_asset_bytes(RENDERER_SOURCE_PATH).await {
            Ok(renderer) => apply_particle_budgets(&mut presets, &renderer)?,
            Err(ReadAssetBytesError::AssetReaderError(AssetReaderError::NotFound(_))) => (),
            Err(error) => return Err(io::Error::other(error)),
        }
        Ok(presets)
    }

    fn extensions(&self) -> &[&str] {
        &["xml", ARCHIVE_ENTRY_ASSET_EXTENSION]
    }
}

fn parse_graphics_settings_presets(bytes: &[u8]) -> io::Result<GraphicsSettingsPresets> {
    let document = parse_blue_fang_source_document(AssetPath::new(SETTINGS_SOURCE_PATH), bytes)
        .map_err(io::Error::other)?;
    lower_graphics_settings_presets(&document.root)
}

fn apply_particle_budgets(presets: &mut GraphicsSettingsPresets, bytes: &[u8]) -> io::Result<()> {
    let document = parse_blue_fang_source_document(AssetPath::new(RENDERER_SOURCE_PATH), bytes)
        .map_err(io::Error::other)?;
    if let Some(tier) = document.root.attribute_named_any(&["graphicsDetail"]) {
        if tier.eq_ignore_ascii_case("low") {
            presets.renderer_budget_tier = ThreeLevelGraphicsDetail::Low;
        } else if tier.eq_ignore_ascii_case("medium") {
            presets.renderer_budget_tier = ThreeLevelGraphicsDetail::Medium;
        }
    }
    for (budget, field) in presets.particle_budgets.iter_mut().zip([
        "maxParticlesLow",
        "maxParticlesMed",
        "maxParticlesHigh",
    ]) {
        *budget = document
            .root
            .attribute_named_any(&[field])
            .map(|value| {
                value.trim().parse::<u32>().map_err(|_| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("invalid {field}={value:?}"),
                    )
                })
            })
            .transpose()?;
    }
    Ok(())
}

fn lower_graphics_settings_presets(
    root: &OrderedSourceDocumentNode,
) -> io::Result<GraphicsSettingsPresets> {
    let settings = root
        .element_children()
        .find(|child| child.name.eq_ignore_ascii_case("settings"))
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "graphics settings table has no settings rows",
            )
        })?;
    let mut presets = ThreeLevelGraphicsDetail::VALUES.map(|tier| GraphicsSettings {
        water_reflections: false,
        particle_budget_tier: Some(tier),
        ..GraphicsSettings::default()
    });
    let mut detail_rows = Vec::new();
    for source in settings.element_children() {
        let Some(row) = lower_graphics_detail_row(source)? else {
            continue;
        };
        for (preset, column) in presets.iter_mut().zip(["low", "medium", "high"]) {
            if let Some(value) = source.attribute_named_any(&[column, "disabled"]) {
                let value = value.parse().map_err(|_| invalid_setting(source, column))?;
                row.apply_value(value, preset);
            }
        }
        if row.label != AssetId::default() {
            detail_rows.push(row);
        }
    }
    Ok(GraphicsSettingsPresets {
        settings: presets,
        detail_rows,
        particle_budgets: [None; 3],
        renderer_budget_tier: ThreeLevelGraphicsDetail::High,
    })
}

pub struct GraphicsDetailRow {
    pub setting_widget: String,
    pub label: AssetId,
    pub thumb_help: UiNodeHelpDefinition,
    pub up_help: UiNodeHelpDefinition,
    pub down_help: UiNodeHelpDefinition,
    pub minimum: i64,
    pub maximum: i64,
    pub increment: f32,
    access: GraphicsField,
    choices: Vec<GraphicsDetailChoice>,
}

struct GraphicsDetailChoice {
    label: AssetId,
    writes: Vec<GraphicsDependentWrite>,
}

#[derive(Clone, Copy)]
struct GraphicsField {
    read: fn(&GraphicsSettings) -> i64,
    write: fn(&mut GraphicsSettings, i64),
    maximum: i64,
}

impl GraphicsDetailRow {
    pub fn selected_value(&self, settings: &GraphicsSettings) -> i64 {
        (self.access.read)(settings).clamp(self.minimum, self.maximum)
    }

    pub fn value_label(&self, value: i64) -> AssetId {
        self.choices
            .get(value.clamp(self.minimum, self.maximum) as usize)
            .map_or(AssetId::default(), |choice| choice.label)
    }

    pub fn apply_value(&self, value: i64, settings: &mut GraphicsSettings) {
        let value = value.clamp(self.minimum, self.maximum);
        (self.access.write)(settings, value);
        if let Some(choice) = self.choices.get(value as usize) {
            for &(write, value) in &choice.writes {
                write(settings, value);
            }
        }
        settings.effects = if settings.environment_detail == ThreeLevelGraphicsDetail::Low {
            GraphicsEffectQuality::Low
        } else {
            GraphicsEffectQuality::High
        };
    }
}

type GraphicsDependentWrite = (fn(&mut GraphicsSettings, i64), i64);

fn graphics_field(name: &str) -> Option<GraphicsField> {
    macro_rules! field {
        ($field:ident, bool) => {
            GraphicsField {
                read: |settings| i64::from(settings.$field),
                write: |settings, value| settings.$field = value != 0,
                maximum: 1,
            }
        };
        ($field:ident, $ty:ident) => {
            GraphicsField {
                read: |settings| settings.$field as i64,
                write: |settings, value| settings.$field = $ty::VALUES[value as usize],
                maximum: ($ty::VALUES.len() - 1) as i64,
            }
        };
    }
    use ThreeLevelGraphicsDetail as Detail;
    Some(match name {
        "environmentdetail" => field!(environment_detail, Detail),
        "lightingdetail" => field!(lighting_detail, Detail),
        "shadowdetail" => field!(shadow_detail, ShadowDetail),
        "speculareffects" => field!(specular_effects, bool),
        "terrainresolution" => field!(terrain_resolution, Detail),
        "terraintexturesize" => field!(terrain_texture_size, Detail),
        "waterdetail" => field!(water_detail, FourLevelGraphicsDetail),
        "detailobjects" => field!(detail_objects, EnvironmentalDetailObjectDensity),
        "objectdetail" => field!(object_detail, Detail),
        "objecttexturedetail" => field!(object_texture_detail, Detail),
        "animationdetail" => field!(animation_detail, Detail),
        "viewdistance" => field!(view_distance, Detail),
        "maxoverheadzoom" => field!(max_overhead_zoom, Detail),
        "screendetail" => field!(screen_detail, bool),
        "uitextshadows" => field!(ui_text_shadows, bool),
        "uitexttextures" => field!(ui_text_textures, bool),
        "shaderdetail" => field!(shader_detail, Detail),
        "waterreflections" => field!(water_reflections, bool),
        _ => return None,
    })
}

fn lower_graphics_detail_row(
    source: &OrderedSourceDocumentNode,
) -> io::Result<Option<GraphicsDetailRow>> {
    let Some(access) = graphics_field(&source.name.to_ascii_lowercase()) else {
        return Ok(None);
    };
    let text = |name| {
        source
            .attribute_named_any(&[name])
            .unwrap_or_default()
            .trim()
    };
    let bound = |name, default| -> io::Result<i64> {
        if text(name).is_empty() {
            return Ok(default);
        }
        text(name)
            .parse::<i64>()
            .map(|value| value.clamp(0, access.maximum))
            .map_err(|_| invalid_setting(source, name))
    };
    let minimum = bound("min", 0)?;
    let maximum = bound("max", access.maximum)?.max(minimum);
    let increment = if text("increment").is_empty() {
        1.0
    } else {
        text("increment")
            .parse::<f32>()
            .ok()
            .filter(|value| value.is_finite() && *value > 0.0)
            .ok_or_else(|| invalid_setting(source, "increment"))?
    };
    let mut choices = Vec::new();
    for choice in source
        .element_children()
        .filter(|child| child.name.eq_ignore_ascii_case("setting"))
    {
        let mut writes = Vec::new();
        for field in choice.element_children() {
            let Some(value) = field.attribute_named_any(&["value"]) else {
                continue;
            };
            let boolean = || {
                parse_source_bool(value)
                    .map(i64::from)
                    .ok_or_else(|| invalid_setting(field, "value"))
            };
            let write: GraphicsDependentWrite = match field.name.to_ascii_lowercase().as_str() {
                "ambientson" => (
                    |settings, value| settings.ambient_effects = value != 0,
                    boolean()?,
                ),
                "skyboxquality" => (
                    |settings, value| settings.skybox_quality = value as u8,
                    i64::from(
                        value
                            .parse::<u8>()
                            .map_err(|_| invalid_setting(field, "value"))?,
                    ),
                ),
                "physanimon" => (
                    |settings, value| settings.physical_animations = value != 0,
                    boolean()?,
                ),
                "propattachon" => (
                    |settings, value| settings.prop_attachments = value != 0,
                    boolean()?,
                ),
                "texturereplaceon" => (
                    |settings, value| settings.texture_replacement = value != 0,
                    boolean()?,
                ),
                "waterreflectionson" => (
                    |settings, value| settings.water_reflections = value != 0,
                    boolean()?,
                ),
                _ => continue,
            };
            writes.push(write);
        }
        choices.push(GraphicsDetailChoice {
            label: localization_key(choice.attribute_named_any(&["locid"]).unwrap_or_default()),
            writes,
        });
    }
    Ok(Some(GraphicsDetailRow {
        setting_widget: text("settingWidget").to_owned(),
        label: localization_key(text("labelLocID")),
        thumb_help: detail_help(text("thumbids")),
        up_help: detail_help(text("upids")),
        down_help: detail_help(text("downids")),
        minimum,
        maximum,
        increment,
        access,
        choices,
    }))
}

fn localization_key(text: &str) -> AssetId {
    if text.is_empty() {
        AssetId::default()
    } else {
        AssetId::from_key(&text.to_ascii_lowercase())
    }
}

fn detail_help(prefix: &str) -> UiNodeHelpDefinition {
    if prefix.is_empty() {
        return UiNodeHelpDefinition::default();
    }
    UiNodeHelpDefinition {
        name: localization_key(&format!("{prefix}_name")),
        short: localization_key(&format!("{prefix}_short")),
        long: localization_key(&format!("{prefix}_long")),
        help: localization_key(&format!("{prefix}_help")),
        lower: localization_key(&format!("{prefix}_lower")),
    }
}

fn invalid_setting(source: &OrderedSourceDocumentNode, field: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!("invalid graphics setting {} {field}", source.name),
    )
}

pub(super) struct GraphicsSettingsPresetAssetPlugin;

impl Plugin for GraphicsSettingsPresetAssetPlugin {
    fn build(&self, application: &mut App) {
        application
            .init_asset::<GraphicsSettingsPresets>()
            .init_asset_loader::<GraphicsSettingsPresetLoader>()
            .init_resource::<GraphicsSettingsPresetSource>()
            .add_systems(
                PreUpdate,
                resolve_graphics_settings_preset_source.run_if(
                    resource_added::<AssetArchives>
                        .or_eager(on_message::<crate::asset_source::ArchiveOverlayChanged>),
                ),
            );
    }
}

fn resolve_graphics_settings_preset_source(
    archives: Res<AssetArchives>,
    server: Res<AssetServer>,
    mut changes: MessageReader<crate::asset_source::ArchiveOverlayChanged>,
    mut source: ResMut<GraphicsSettingsPresetSource>,
) {
    let path = Path::new(SETTINGS_SOURCE_PATH);
    let authored_inputs_changed = changes
        .read()
        .flat_map(|change| &change.changes)
        .any(|change| {
            change.normalized_asset_path == path
                || change.normalized_asset_path == Path::new(RENDERER_SOURCE_PATH)
        });
    changes.clear();
    let selected_path = archives
        .resolved_winning_entry(path)
        .map_or_else(|| path.to_owned(), |entry| archive_entry_asset_path(&entry));
    if source
        .0
        .as_ref()
        .and_then(Handle::path)
        .is_some_and(|path| path.path() == selected_path)
    {
        if authored_inputs_changed {
            server.reload(selected_path);
        }
        return;
    }
    source.0 = Some(server.load(selected_path));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn particle_budgets_follow_preset_columns_independently() -> io::Result<()> {
        let mut presets =
            parse_graphics_settings_presets(b"<UserSettings><settings/></UserSettings>")?;
        apply_particle_budgets(
            &mut presets,
            br#"<BFDX9Renderer graphicsDetail="low" maxParticlesLow="0" maxParticlesHigh="137"/>"#,
        )?;
        assert_eq!(
            presets
                .settings
                .map(|preset| presets.particle_budget(preset.particle_budget_tier)),
            [Some(0), None, Some(137)]
        );
        assert_eq!(presets.particle_budget(None), Some(0));
        Ok(())
    }

    #[test]
    fn preset_columns_use_authored_indexes_and_nested_values() -> io::Result<()> {
        let source = parse_blue_fang_source_document(
            AssetPath::new(SETTINGS_SOURCE_PATH),
            br#"<UserSettings><settings>
                <environmentDetail low="2" medium="0" high="1">
                    <setting><skyboxQuality value="4"/></setting>
                    <setting><skyboxQuality value="5"/></setting>
                    <setting><skyboxQuality value="6"/></setting>
                </environmentDetail>
                <specularEffects low="1" medium="0" high="0"/>
                <detailObjects low="0" medium="2" high="1"/>
                <screenDetail low="1" medium="0" high="0"/>
                <waterReflections low="0" medium="0" high="1"/>
            </settings></UserSettings>"#,
        )
        .map_err(io::Error::other)?;
        let presets = lower_graphics_settings_presets(&source.root)?;
        let highest = presets.settings(GraphicsDetailPreset::Highest, 4);
        assert_eq!(highest.environment_detail, ThreeLevelGraphicsDetail::Medium);
        assert_eq!(highest.skybox_quality, 5);
        assert!(!highest.specular_effects);
        assert_eq!(
            highest.detail_objects,
            EnvironmentalDetailObjectDensity::Medium
        );
        assert!(!highest.screen_detail);
        assert!(highest.water_reflections);
        assert_eq!(highest.multisample_count, 4);
        assert_eq!(
            presets
                .settings(GraphicsDetailPreset::High, 1)
                .skybox_quality,
            4
        );
        assert_eq!(
            presets
                .settings(GraphicsDetailPreset::Medium, 1)
                .skybox_quality,
            6
        );
        Ok(())
    }
}
