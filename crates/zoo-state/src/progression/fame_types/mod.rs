use bevy::prelude::*;

#[derive(Component, Debug, Clone, Copy, Default, PartialEq)]
pub struct Fame {
    pub half_stars: u8,
    pub current_percent: Option<f32>,
    pub maximum_reached: u8,
    pub maximum_percent_reached: Option<f32>,
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq)]
pub struct ZooMaximumFame(pub Option<f32>);

#[derive(Message, Debug, Clone, Copy)]
pub struct SetScenarioMaximumFame {
    pub percent: f32,
}

impl Fame {
    pub fn half_stars_for_percent(percent: f32) -> u8 {
        (percent / 10.0).floor() as u8
    }
}

/// Authored presentation request to replace the entrance appearance in place.
#[derive(Message, Clone, Copy)]
pub struct FameGateAppearanceRequested {
    pub gate: Entity,
    pub definition: openzt2_game_data::AssetId,
}

#[derive(Component, PartialEq)]
pub struct PendingFameGatePrefab {
    pub definition: openzt2_game_data::AssetId,
    pub prefab: Handle<openzt2_game_assets::scene_prefab::ScenePrefabAsset>,
    pub releasing_root_renderable: bool,
}

/// Accumulated view facts per species from the guest's accepted view events.
#[derive(Component, Default)]
pub struct GuestSpeciesFameViews(
    pub std::collections::BTreeMap<openzt2_game_data::AssetId, GuestSpeciesFameView>,
);

#[derive(Default)]
pub struct GuestSpeciesFameView {
    pub events: u32,
    pub entertainment: f32,
    pub donation: f32,
}

/// Layout values produced by the world's authored fame script.
#[derive(Component, Default, PartialEq)]
pub struct FameAuthoredPresentation {
    pub bars: std::collections::BTreeMap<openzt2_game_data::AssetId, f32>,
    pub gate_icon: Option<openzt2_game_data::AssetId>,
}
