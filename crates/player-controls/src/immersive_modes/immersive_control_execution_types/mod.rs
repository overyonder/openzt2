use bevy::prelude::*;

macro_rules! define_immersive_mode_control_markers {
    ($($control_marker_name:ident),+ $(,)?) => {
        $(
            #[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
            pub(crate) struct $control_marker_name;
        )+
    };
}

define_immersive_mode_control_markers!(
    GuestViewControl,
    FirstPersonControl,
    FossilSearchControl,
    FossilAssemblyControl,
    ShowEditControl,
);

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct GuestViewTileCount(pub(crate) u32);

/// Entity-local view angles applied to the subject-relative Bevy camera. The
/// subject transform remains the locomotion authority.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct FirstPersonViewAngles {
    pub(crate) yaw: f32,
    pub(crate) pitch: f32,
}

use openzt2_interface_state::immersive_modes::immersive_mode_state_types::ImmersiveMode;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PendingImmersiveEntry {
    pub(crate) mode: ImmersiveMode,
    pub(crate) subject: Option<Entity>,
    pub(crate) failure: Option<
        openzt2_interface_state::immersive_modes::immersive_mode_message_types::ModeEntryFailure,
    >,
}
