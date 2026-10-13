use bevy::prelude::*;
use openzt2_game_data::AssetId;

use openzt2_interface_state::input::input_types::PrimaryPointerInputState;
use openzt2_interface_state::ui::pointer_capture_types::UiPointerCapture;
use openzt2_zoo_state::construction::construction_interaction_types::ConstructionCursor;

use openzt2_zoo_state::topology::topology_graph_types::TopologyGrid;

/// The latest initial fence anchor press, installed only by runtime verification.
#[derive(Resource, Default)]
pub struct InitialFenceAnchorPressDiagnosticState {
    pending: Option<InitialFenceAnchorPress>,
}

impl InitialFenceAnchorPressDiagnosticState {
    pub(super) const fn record_press(&mut self, press: InitialFenceAnchorPress) {
        self.pending = Some(press);
    }

    pub const fn take_pending_press(&mut self) -> Option<InitialFenceAnchorPress> {
        self.pending.take()
    }
}

/// The inputs topology consumed when an ordinary press began a fence preview.
#[derive(Clone, Copy)]
pub struct InitialFenceAnchorPress {
    pub root: Entity,
    pub definition: AssetId,
    pub grid: TopologyGrid,
    pub pointer: PrimaryPointerInputState,
    pub cursor: ConstructionCursor,
    pub ui_capture: UiPointerCapture,
    pub cell: IVec3,
}
