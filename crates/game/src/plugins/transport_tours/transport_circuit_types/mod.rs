use bevy::prelude::*;
use openzt2_game_data::AssetId;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TransportCircuit {
    pub(crate) definition: AssetId,
    pub(crate) closed: bool,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CircuitRunning(pub(crate) bool);

impl Default for CircuitRunning {
    fn default() -> Self {
        Self(true)
    }
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum CircuitDirection {
    #[default]
    Forward,
    Reverse,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CircuitMember(pub(crate) Entity);

/// The selected vehicle definition for one circuit, resolved before the UI
/// action is activated. Generation never searches a source catalogue.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CircuitVehicleDefinition(pub(crate) AssetId);

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct OpenCircuitRequest {
    pub(crate) circuit: Entity,
    pub(crate) open: bool,
}
