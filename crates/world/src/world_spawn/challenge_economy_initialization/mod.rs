use std::io;

use bevy::asset::{io::Reader, AssetLoader, LoadContext};
use bevy::prelude::*;
use openzt2_game_assets::source_document::{
    blue_fang_source_document_parsing::parse_blue_fang_source_document, path::AssetPath,
};

pub const CHALLENGE_MODE_DOCUMENT: &str = "ui/modes/modes.xml";

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
        _context: &mut LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        let document =
            parse_blue_fang_source_document(AssetPath::new(CHALLENGE_MODE_DOCUMENT), &bytes)
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        let mode = if document.root.name.eq_ignore_ascii_case("ZTMainMode") {
            &document.root
        } else {
            document
                .root
                .element_children()
                .find(|node| node.name.eq_ignore_ascii_case("ZTMainMode"))
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing ZTMainMode"))?
        };
        let starting_cash_cents = mode
            .attribute("startingCash")
            .map(|amount| {
                amount
                    .parse::<i64>()
                    .ok()
                    .and_then(|amount| amount.checked_mul(100))
                    .ok_or_else(|| {
                        io::Error::new(io::ErrorKind::InvalidData, "invalid startingCash")
                    })
            })
            .transpose()?;
        Ok(ChallengeEconomyInitializer {
            starting_cash_cents,
        })
    }
}
