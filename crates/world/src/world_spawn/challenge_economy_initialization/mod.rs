use std::io;

use bevy::asset::{io::Reader, AssetLoader, LoadContext};
use bevy::prelude::*;
use openzt2_game_assets::source_document::{
    blue_fang_source_document_parsing::parse_blue_fang_source_document,
    blue_fang_source_numeric_lexeme::parse_blue_fang_source_numeric_lexeme, path::AssetPath,
};

pub(super) const CHALLENGE_MODE_DOCUMENT: &str = "config/challenge_mode.xml";

#[derive(Asset, TypePath)]
pub struct ChallengeEconomyInitializer {
    pub(super) starting_cash_cents: i64,
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
        _: &mut LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        let document =
            parse_blue_fang_source_document(AssetPath::new(CHALLENGE_MODE_DOCUMENT), &bytes)
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        let starting_cash_cents = document
            .root
            .element_children()
            .find(|node| node.name.eq_ignore_ascii_case("ZTEconomyMgr"))
            .and_then(|economy| economy.attribute_named_any(&["initialCash"]))
            .and_then(parse_blue_fang_source_numeric_lexeme::<f64>)
            .map(|amount| (100.0 * amount).round())
            .filter(|amount| amount.is_finite() && amount.abs() < i64::MAX as f64)
            .map(|amount| amount as i64)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "invalid challenge starting cash",
                )
            })?;
        Ok(ChallengeEconomyInitializer {
            starting_cash_cents,
        })
    }
}
