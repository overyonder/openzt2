use bevy::prelude::*;
use openzt2_game_data::ui_document::node_property_binding::UiBooleanPropertyBindingSource;

use openzt2_interface_state::camera::camera_runtime_state_types::CameraMode;
use openzt2_interface_state::camera::camera_runtime_state_types::ZooCamera;
use openzt2_interface_state::immersive_modes::immersive_mode_state_types::ActiveImmersiveMode;
use openzt2_interface_state::immersive_modes::immersive_mode_state_types::{
    ImmersiveCamera, ImmersiveMode,
};
use openzt2_interface_state::persistence::save_slot_types::WorldSnapshotLoadedFromSlot;

use super::authored_ui_selection_binding::UiSelectedBinding;
use super::authored_ui_selection_state::UiSelected;

pub(super) fn reflect_accepted_camera_mode_in_main_toolbar(
    cameras: Query<(Entity, &CameraMode), With<ZooCamera>>,
    immersive_modes: Query<(&ActiveImmersiveMode, &ImmersiveCamera)>,
    mut buttons: Query<(&UiSelectedBinding, &mut UiSelected)>,
) {
    let camera = cameras.single().ok();
    let active = camera.and_then(|(camera, _)| {
        immersive_modes
            .iter()
            .find_map(|(active, link)| (link.0 == camera).then_some(active))
    });
    let photo = active.is_some_and(|active| active.mode == ImmersiveMode::Photo)
        && camera.is_some_and(|(_, mode)| *mode == CameraMode::Free);
    for (binding, mut selected) in &mut buttons {
        let current = match &binding.0 {
            UiBooleanPropertyBindingSource::OverheadViewSelected => {
                camera.is_some_and(|(_, mode)| {
                    // Guest view and ordinary follow-camera use remain within
                    // the authored overhead branch.
                    *mode == CameraMode::Overhead
                        || *mode == CameraMode::Follow
                            && active.is_none_or(|active| active.mode == ImmersiveMode::GuestView)
                })
            }
            UiBooleanPropertyBindingSource::FirstPersonViewSelected => {
                camera.is_some_and(|(_, mode)| *mode == CameraMode::FirstPerson)
            }
            UiBooleanPropertyBindingSource::PhotoViewSelected => photo,
            _ => continue,
        };
        if selected.0 != current {
            selected.0 = current;
            // Reflection must not re-run a mode's authored On/Off commands.
        }
    }
}

#[derive(Component)]
pub(super) struct ToolbarToggleInitialSelection(pub(super) bool);

pub(super) fn restore_main_toolbar_toggle_selection(
    mut completed_loads: MessageReader<WorldSnapshotLoadedFromSlot>,
    mut toggles: Query<
        (Ref<ToolbarToggleInitialSelection>, &mut UiSelected),
        Without<UiSelectedBinding>,
    >,
) {
    let world_loaded = completed_loads.read().last().is_some();
    for (initial, mut selection) in &mut toggles {
        if world_loaded || initial.is_added() {
            selection.set_if_neq(UiSelected(initial.0));
        }
    }
}
