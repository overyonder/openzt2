use bevy::prelude::*;
use openzt2_game_data::world_definitions::staff_management::StaffJobKind;
use openzt2_game_data::AssetId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum StaffRequestControllerSample {
    NumberQ16(i32),
    #[allow(
        dead_code,
        reason = "native boolean thresholds are retained, but live boolean attribute sampling is not connected yet"
    )]
    Boolean(bool),
    NoValue,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct StaffRequestRowIdentity {
    pub(super) definition: AssetId,
    pub(super) binder: Option<AssetId>,
    pub(super) attribute_key: Option<AssetId>,
    pub(super) token: Option<AssetId>,
    pub(super) ordinal: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct StaffRequestRuntimeState {
    pub(super) row: StaffRequestRowIdentity,
    pub(super) previous: Option<StaffRequestControllerSample>,
}

#[derive(Component, Debug, Default)]
pub(super) struct StaffRequestRuntimeBinding {
    pub(super) revision: u64,
    pub(super) rows: Vec<StaffRequestRuntimeState>,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct StaffRequestBindingPending;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct StaffRequestJobKey {
    pub(super) kind: StaffJobKind,
    pub(super) target: Entity,
    pub(super) token: AssetId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum StaffRequestJobEvent {
    Raised {
        key: StaffRequestJobKey,
        urgency: u16,
    },
    Cancelled {
        key: StaffRequestJobKey,
    },
}
