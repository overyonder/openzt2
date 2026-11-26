use bevy::{ecs::system::SystemParam, prelude::*};
use openzt2_world::world_spawn::challenge_economy_initialization::{
    ChallengeEconomyInitializer, CHALLENGE_MODE_DOCUMENT,
};

#[derive(Resource)]
pub(super) struct MainModeCashAsset(Handle<ChallengeEconomyInitializer>);

pub(super) fn load_main_mode_cash(mut commands: Commands, server: Res<AssetServer>) {
    commands.insert_resource(MainModeCashAsset(server.load(CHALLENGE_MODE_DOCUMENT)));
}

#[derive(SystemParam)]
pub(super) struct MainModeStartingCash<'w, 's> {
    handle: Option<Res<'w, MainModeCashAsset>>,
    assets: Option<Res<'w, Assets<ChallengeEconomyInitializer>>>,
    events: Option<MessageReader<'w, 's, AssetEvent<ChallengeEconomyInitializer>>>,
}

impl MainModeStartingCash<'_, '_> {
    pub(super) fn authored_cents(&self) -> Option<i64> {
        self.assets
            .as_ref()?
            .get(&self.handle.as_ref()?.0)?
            .starting_cash_cents()
    }

    pub(super) fn changed(&mut self) -> bool {
        self.events
            .as_mut()
            .is_some_and(|events| events.read().count() != 0)
    }
}
