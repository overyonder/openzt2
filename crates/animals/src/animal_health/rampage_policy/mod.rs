use std::io;

use bevy::{
    asset::{AssetLoader, LoadContext, io::Reader},
    prelude::*,
};

use openzt2_game_assets::source_document::{
    blue_fang_source_document_parsing::parse_blue_fang_source_document,
    blue_fang_source_numeric_lexeme::parse_blue_fang_source_numeric_lexeme, path::AssetPath,
};

const RAMPAGE_POLICY_PATH: &str = "config/rampage.xml";

#[derive(Asset, TypePath)]
pub(super) struct RampagePolicy {
    pub(super) check_interval: f32,
    pub(super) good_point_limit: f32,
    pub(super) bad_point_limit: f32,
    pub(super) base_chance: f32,
    pub(super) maximum_chance: f32,
    pub(super) population_factor: f32,
    pub(super) duration: f32,
    pub(super) variance: f32,
    pub(super) cooldown: f32,
    pub(super) music: openzt2_game_data::AssetId,
    pub(super) end_music: openzt2_game_data::AssetId,
}

#[derive(Resource)]
pub(super) struct RampagePolicySource(pub(super) Handle<RampagePolicy>);

#[derive(Default, TypePath)]
struct RampagePolicyLoader;

impl AssetLoader for RampagePolicyLoader {
    type Asset = RampagePolicy;
    type Settings = ();
    type Error = io::Error;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _: &Self::Settings,
        _: &mut LoadContext<'_>,
    ) -> io::Result<RampagePolicy> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        parse_rampage_policy(&bytes)
    }

    fn extensions(&self) -> &[&str] {
        &["xml"]
    }
}

pub(super) fn parse_rampage_policy(bytes: &[u8]) -> io::Result<RampagePolicy> {
    let document = parse_blue_fang_source_document(AssetPath::new(RAMPAGE_POLICY_PATH), bytes)
        .map_err(io::Error::other)?;
    let root = document.root;
    if !root.name.eq_ignore_ascii_case("BFGRampageMgr") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "expected BFGRampageMgr",
        ));
    }
    let number = |name| {
        root.attribute_named_any(&[name])
            .and_then(parse_blue_fang_source_numeric_lexeme::<f32>)
            .filter(|value| value.is_finite() && *value >= 0.0)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("invalid rampage field {name}"),
                )
            })
    };
    let cue = |name| {
        root.attribute_named_any(&[name])
            .map_or_else(openzt2_game_data::AssetId::default, |name| {
                openzt2_game_data::AssetId::from_key(&name.trim().to_ascii_lowercase())
            })
    };
    Ok(RampagePolicy {
        check_interval: number("updateInterval")?,
        good_point_limit: number("maxGoodPoints")?,
        bad_point_limit: number("maxBadPoints")?,
        base_chance: number("baseRampageChance")?,
        maximum_chance: number("maxRampageChance")?,
        population_factor: number("rampagingAnimalFactor")?,
        duration: number("rampageTimeout")?,
        variance: number("rampageVariance")?,
        cooldown: number("rampageInterval")?,
        music: cue("rampageMusic"),
        end_music: cue("rampageEndMusic"),
    })
}

pub(super) fn register_rampage_policy(app: &mut App) {
    app.init_asset::<RampagePolicy>()
        .init_asset_loader::<RampagePolicyLoader>()
        .add_systems(
            Startup,
            |server: Res<AssetServer>, mut commands: Commands| {
                commands.insert_resource(RampagePolicySource(server.load(RAMPAGE_POLICY_PATH)));
            },
        );
}
