use bevy::prelude::*;
use openzt2_game_data::AssetId;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransportCircuit {
    pub definition: AssetId,
    pub closed: bool,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct CircuitRunning(pub bool);

impl Default for CircuitRunning {
    fn default() -> Self {
        Self(true)
    }
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CircuitDirection {
    #[default]
    Forward,
    Reverse,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct CircuitMember(pub Entity);

/// The selected vehicle definition for one circuit, resolved before the UI
/// action is activated. Generation never searches a source catalogue.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct CircuitVehicleDefinition(pub AssetId);

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenCircuitRequest {
    pub circuit: Entity,
    pub open: bool,
}
