use bevy::ecs::system::SystemParam;
use bevy::{prelude::*, text::EditableText};

use openzt2_interface_state::camera::camera_control_message_types::SetCameraMode;
use openzt2_interface_state::camera::camera_runtime_state_types::CameraDefinition;
use openzt2_interface_state::camera::camera_runtime_state_types::ZooCamera;
use openzt2_zoo_state::animal_health::types::Disease;
use openzt2_zoo_state::animal_health::types::Rampaging;

use openzt2_interface_state::information::entity_selection_types::InformationEntitySource;
use openzt2_interface_state::information::entity_selection_types::Inspectable;
use openzt2_interface_state::information::entity_selection_types::SelectedEntity;
use openzt2_interface_state::information::entity_selection_types::SelectionRequest;

/// Canonical selection state, world identity queries, and selection and camera
/// outputs used by authored information-selection actions.
#[derive(SystemParam)]
pub(super) struct InformationSelectionActionTargets<'w, 's> {
    pub(super) selected_entity: Res<'w, SelectedEntity>,
    pub(super) live_entity_names: Query<'w, 's, &'static mut Name>,
    pub(super) zoo_roots: Query<
        'w,
        's,
        Entity,
        With<openzt2_world_state::world_spawn::world_membership_types::WorldRoot>,
    >,
    pub(super) information_entity_sources: Query<'w, 's, &'static InformationEntitySource>,
    pub(super) entity_parents: Query<'w, 's, &'static ChildOf>,
    pub(super) activated_text_nodes: Query<'w, 's, &'static EditableText>,
    pub(super) inspectable_animal_health: Query<
        'w,
        's,
        (Entity, Option<&'static Disease>, Option<&'static Rampaging>),
        With<Inspectable>,
    >,
    pub(super) selection_requests: MessageWriter<'w, SelectionRequest>,
    pub(super) zoo_camera_definitions: Query<'w, 's, &'static CameraDefinition, With<ZooCamera>>,
    pub(super) camera_mode_requests: MessageWriter<'w, SetCameraMode>,
}
