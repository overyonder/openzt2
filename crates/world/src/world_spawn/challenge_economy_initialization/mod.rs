use std::io;

use bevy::asset::{io::Reader, AssetLoader, LoadContext};
use bevy::prelude::*;
use openzt2_game_assets::source_document::{
    blue_fang_source_document_parsing::parse_blue_fang_source_document,
    blue_fang_source_numeric_lexeme::parse_blue_fang_source_numeric_lexeme, path::AssetPath,
};

pub const CHALLENGE_MODE_DOCUMENT: &str = "config/challenge_mode.xml";

#[derive(Asset, TypePath)]
pub struct ChallengeEconomyInitializer {
    starting_cash_cents: Option<i64>,
}

impl ChallengeEconomyInitializer {
    pub fn starting_cash_cents(&self) -> Option<i64> {
        self.starting_cash_cents
    }
}

#[derive(Default, TypePath)]
pub(super) struct ChallengeEconomyInitializerLoader;

impl AssetLoader for ChallengeEconomyInitializerLoader {
    type Asset = ChallengeEconomyInitializer;
    type Settings = ();
    type Error = io::Error;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _: &(),
        context: &mut LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        let document =
            parse_blue_fang_source_document(AssetPath::new(CHALLENGE_MODE_DOCUMENT), &bytes)
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        let economy = document
            .root
            .element_children()
            .find(|node| node.name.eq_ignore_ascii_case("ZTEconomyMgr"))
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, "missing Challenge economy")
            })?;
        let config_data = if let Some(path) = economy.attribute_named_any(&["configData"]) {
            let bytes = context
                .read_asset_bytes(path.to_owned())
                .await
                .map_err(io::Error::other)?;
            Some(
                parse_blue_fang_source_document(AssetPath::new(path), &bytes)
                    .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?,
            )
        } else {
            None
        };
        let starting_cash_cents = economy
            .attribute_named_any(&["initialCash"])
            .or_else(|| {
                config_data
                    .as_ref()?
                    .root
                    .attribute_named_any(&["initialCash"])
            })
            .map(|value| {
                parse_blue_fang_source_numeric_lexeme::<f64>(value)
                    .map(|amount| (100.0 * amount).round())
                    .filter(|amount| amount.is_finite() && amount.abs() < i64::MAX as f64)
                    .map(|amount| amount as i64)
                    .ok_or_else(|| {
                        io::Error::new(
                            io::ErrorKind::InvalidData,
                            "invalid Challenge starting cash",
                        )
                    })
            })
            .transpose()?;
        Ok(ChallengeEconomyInitializer {
            starting_cash_cents,
        })
    }
}
