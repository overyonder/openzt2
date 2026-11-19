use bevy::prelude::*;
use openzt2_game_data::world_definitions::immersive_mode_policy::ImmersiveModeKind;
use openzt2_game_data::world_definitions::immersive_mode_policy::ImmersiveModePolicy;
use openzt2_game_data::AssetId;

use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;

use super::immersive_mode_policy_types::AllowedInteractionActions;
use super::immersive_mode_policy_types::InteractionCursor;
use super::immersive_mode_policy_types::InteractionPrefab;
use super::immersive_mode_state_types::ImmersiveMode;
use crate::immersive_modes::immersive_control_execution_types::FirstPersonViewAngles;
use openzt2_interface_state::immersive_modes::immersive_mode_state_types::ImmersiveHudSuppression;
use openzt2_world_state::simulation_time::simulation_control_types::ImmersiveSimulationPauseClaim;

pub(crate) fn calculate_initial_first_person_view_angles(
    definitions: WorldDefinitionsView<'_>,
    camera_definition: AssetId,
) -> FirstPersonViewAngles {
    definitions.find_camera(camera_definition).map_or_else(
        FirstPersonViewAngles::default,
        |camera| FirstPersonViewAngles {
            yaw: (camera.yaw_radians[0] + camera.yaw_radians[1]) * 0.5,
            pitch: camera.initial_pitch_radians,
        },
    )
}

pub(super) fn find_authored_immersive_mode_policy<'a>(
    definitions: WorldDefinitionsView<'a>,
    mode: ImmersiveMode,
) -> Option<&'a ImmersiveModePolicy> {
    definitions
        .immersive_mode_policies()
        .find(|policy| authored_mode_kind_matches_immersive_mode(&policy.mode, mode))
}

pub(crate) fn attach_authored_immersive_mode_consumer_policies(
    definitions: WorldDefinitionsView<'_>,
    mode: ImmersiveMode,
    controller: Entity,
    commands: &mut Commands,
) {
    let Some(policy) = find_authored_immersive_mode_policy(definitions, mode) else {
        return;
    };
    let mut entity = commands.entity(controller);
    entity.insert(AllowedInteractionActions {
        allowed_actions: policy.allowed_actions,
    });
    if let Some(image) = definitions.texture_image(AssetId(policy.interaction_cursor.0)) {
        entity.insert(InteractionCursor { image });
    }
    let definition = AssetId(policy.interaction_prefab.0);
    if let Some(handle) = definitions.scene(definition) {
        entity.insert(InteractionPrefab { definition, handle });
    }
    if policy.pause_simulation {
        entity.insert(ImmersiveSimulationPauseClaim);
    }
    if policy.hide_hud {
        entity.insert(ImmersiveHudSuppression);
    }
}

fn authored_mode_kind_matches_immersive_mode(
    authored_mode_kind: &ImmersiveModeKind,
    immersive_mode: ImmersiveMode,
) -> bool {
    matches!(
        (authored_mode_kind, immersive_mode),
        (ImmersiveModeKind::GuestView, ImmersiveMode::GuestView)
            | (ImmersiveModeKind::FirstPerson, ImmersiveMode::FirstPerson)
            | (ImmersiveModeKind::FossilSearch, ImmersiveMode::FossilSearch)
            | (
                ImmersiveModeKind::FossilAssembly,
                ImmersiveMode::FossilAssembly
            )
            | (ImmersiveModeKind::Photo, ImmersiveMode::Photo)
            | (ImmersiveModeKind::ShowEdit, ImmersiveMode::ShowEdit)
            | (ImmersiveModeKind::SuperStaff, ImmersiveMode::SuperStaff)
    )
}
