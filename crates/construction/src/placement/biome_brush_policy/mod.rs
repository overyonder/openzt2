use std::io;

use bevy::{
    asset::{io::Reader, AssetLoader, LoadContext},
    ecs::system::SystemParam,
    prelude::*,
};
use openzt2_game_assets::{
    graphics_settings_presets::{
        GraphicsDetailPreset, GraphicsSettingsPresetSource, GraphicsSettingsPresets,
    },
    graphics_settings_types::GraphicsSettings,
    source_document::{
        blue_fang_source_document_parsing::parse_blue_fang_source_document, path::AssetPath,
    },
};

#[derive(Asset, TypePath)]
pub(super) struct BiomeBrushPolicy {
    pub(super) random_mask_start: bool,
    pub(super) delete_biome_entities: bool,
    weights: [u32; 4],
}

#[derive(Default, TypePath)]
pub(super) struct BiomeBrushPolicyLoader;

impl AssetLoader for BiomeBrushPolicyLoader {
    type Asset = BiomeBrushPolicy;
    type Settings = ();
    type Error = io::Error;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _: &(),
        context: &mut LoadContext<'_>,
    ) -> io::Result<Self::Asset> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        let path = AssetPath::new(context.path().path().to_string_lossy());
        let data = parse_blue_fang_source_document(path, &bytes).map_err(io::Error::other)?;
        let options = &data.root;
        let read_flag = |name| {
            options
                .attribute_named_any(&[name])
                .is_some_and(|value| matches!(value, "1" | "true"))
        };
        let weights = options
            .attribute_named_any(&["graphicsLevelNothingFactor"])
            .and_then(|values| {
                values
                    .split_ascii_whitespace()
                    .map(str::parse::<u32>)
                    .collect::<Result<Vec<_>, _>>()
                    .ok()?
                    .try_into()
                    .ok()
            })
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, "invalid biome nothing weights")
            })?;
        Ok(BiomeBrushPolicy {
            random_mask_start: read_flag("randomMaskStart"),
            delete_biome_entities: read_flag("deleteBiomeEntities"),
            weights,
        })
    }

    fn extensions(&self) -> &[&str] {
        &["xml"]
    }
}

#[derive(Resource)]
pub(super) struct BiomeBrushPolicySource(Handle<BiomeBrushPolicy>);

pub(super) fn load_biome_brush_policy(mut commands: Commands, assets: Res<AssetServer>) {
    commands.insert_resource(BiomeBrushPolicySource(assets.load("biomes/biome.xml")));
}

#[derive(SystemParam)]
pub(super) struct BiomeBrushRules<'w> {
    source: Option<Res<'w, BiomeBrushPolicySource>>,
    policies: Res<'w, Assets<BiomeBrushPolicy>>,
    settings: Res<'w, GraphicsSettings>,
    presets: Res<'w, Assets<GraphicsSettingsPresets>>,
    preset_source: Res<'w, GraphicsSettingsPresetSource>,
}

impl BiomeBrushRules<'_> {
    pub(super) fn policy(&self) -> Option<&BiomeBrushPolicy> {
        self.policies.get(&self.source.as_ref()?.0)
    }

    pub(super) fn nothing_weight_percent(&self, policy: &BiomeBrushPolicy) -> u32 {
        const PRESETS: [GraphicsDetailPreset; 3] = [
            GraphicsDetailPreset::Medium,
            GraphicsDetailPreset::High,
            GraphicsDetailPreset::Highest,
        ];
        let index = self
            .preset_source
            .get(&self.presets)
            .and_then(|table| {
                PRESETS.iter().position(|preset| {
                    table.settings(*preset, self.settings.multisample_count) == *self.settings
                })
            })
            .map_or(0, |index| index + 1);
        policy.weights[index]
    }
}
