use std::io;

use bevy::{
    asset::{io::Reader, AssetLoader, LoadContext},
    ecs::system::SystemParam,
    prelude::*,
};
use openzt2_game_assets::source_document::{
    blue_fang_source_document_parsing::parse_blue_fang_source_document, path::AssetPath,
};

#[derive(Asset, TypePath)]
pub(crate) struct MainModeCash {
    starting_cash_cents: Option<i64>,
}

#[derive(TypePath, Default)]
pub(super) struct MainModeCashLoader;

impl AssetLoader for MainModeCashLoader {
    type Asset = MainModeCash;
    type Settings = ();
    type Error = io::Error;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _: &(),
        context: &mut LoadContext<'_>,
    ) -> io::Result<MainModeCash> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        let path = context.path().path().to_string_lossy();
        let document = parse_blue_fang_source_document(AssetPath::new(&path), &bytes)
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
        Ok(MainModeCash {
            starting_cash_cents,
        })
    }
}

#[derive(Resource)]
pub(super) struct MainModeCashAsset(Handle<MainModeCash>);

impl FromWorld for MainModeCashAsset {
    fn from_world(world: &mut World) -> Self {
        Self(world.resource::<AssetServer>().load("ui/modes/modes.xml"))
    }
}

#[derive(SystemParam)]
pub(super) struct MainModeStartingCash<'w, 's> {
    handle: Option<Res<'w, MainModeCashAsset>>,
    assets: Option<Res<'w, Assets<MainModeCash>>>,
    events: Option<MessageReader<'w, 's, AssetEvent<MainModeCash>>>,
}

impl MainModeStartingCash<'_, '_> {
    pub(super) fn authored_cents(&self) -> Option<i64> {
        self.assets
            .as_ref()?
            .get(&self.handle.as_ref()?.0)?
            .starting_cash_cents
    }

    pub(super) fn changed(&mut self) -> bool {
        self.events
            .as_mut()
            .is_some_and(|events| events.read().count() != 0)
    }
}
