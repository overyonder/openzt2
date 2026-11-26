use bevy::prelude::*;
use openzt2_game_data::ui_document::action::{simulation_time::UiSimulationAction, UiActionRecord};
use openzt2_game_data::ui_document::node::UiNodeDefinition;
use openzt2_world_state::simulation_time::simulation_control_types::AuthoredUiTemporarySimulationPause;

use openzt2_interface_state::ui::authored_ui_action_projection_components::UiActionSource;
use openzt2_interface_state::ui::authored_ui_action_projection_components::UiAnimalActions;
use openzt2_interface_state::ui::authored_ui_action_projection_components::UiAnimalHealthActions;
use openzt2_interface_state::ui::authored_ui_action_projection_components::UiCameraActions;
use openzt2_interface_state::ui::authored_ui_action_projection_components::UiConstructionActions;
use openzt2_interface_state::ui::authored_ui_action_projection_components::UiEconomyActions;
use openzt2_interface_state::ui::authored_ui_action_projection_components::UiImmersiveModeActions;
use openzt2_interface_state::ui::authored_ui_action_projection_components::UiInformationActions;
use openzt2_interface_state::ui::authored_ui_action_projection_components::UiPersistenceActions;
use openzt2_interface_state::ui::authored_ui_action_projection_components::UiPhotoActions;
use openzt2_interface_state::ui::authored_ui_action_projection_components::UiPopulateCatalogueTypeListActions;
use openzt2_interface_state::ui::authored_ui_action_projection_components::UiPresentationActions;
use openzt2_interface_state::ui::authored_ui_action_projection_components::UiResearchActions;
use openzt2_interface_state::ui::authored_ui_action_projection_components::UiScenarioActions;
use openzt2_interface_state::ui::authored_ui_action_projection_components::UiShellActions;
use openzt2_interface_state::ui::authored_ui_action_projection_components::UiSimulationActions;
use openzt2_interface_state::ui::authored_ui_action_projection_components::UiStaffActions;
use openzt2_interface_state::ui::authored_ui_action_projection_components::UiTransportActions;
use openzt2_interface_state::ui::authored_ui_action_projection_components::{
    UiAudioSettingActions, UiAudioVolumeControl,
};
use openzt2_interface_state::ui::authored_ui_action_projection_components::{
    UiScriptActions, UiShowActions,
};

pub(super) fn insert_authored_node_action_routing_components(
    commands: &mut Commands,
    entity: Entity,
    node_index: u32,
    record: &UiNodeDefinition,
) {
    macro_rules! insert_component_when_node_has_action_variant {
        ($variant:ident, $component:ident) => {
            if record
                .actions
                .iter()
                .any(|action| matches!(action, UiActionRecord::$variant(_)))
            {
                commands
                    .entity(entity)
                    .insert($component(UiActionSource::Node(node_index)));
            }
        };
    }

    insert_component_when_node_has_action_variant!(Presentation, UiPresentationActions);
    insert_component_when_node_has_action_variant!(Shell, UiShellActions);
    insert_component_when_node_has_action_variant!(
        PopulateCatalogueTypeList,
        UiPopulateCatalogueTypeListActions
    );
    insert_component_when_node_has_action_variant!(Construction, UiConstructionActions);
    insert_component_when_node_has_action_variant!(Camera, UiCameraActions);
    insert_component_when_node_has_action_variant!(Photo, UiPhotoActions);
    insert_component_when_node_has_action_variant!(
        StartResearchForSelectedCatalogueItem,
        UiResearchActions
    );
    insert_component_when_node_has_action_variant!(AnimalHealth, UiAnimalHealthActions);
    insert_component_when_node_has_action_variant!(Show, UiShowActions);
    insert_component_when_node_has_action_variant!(Transport, UiTransportActions);
    insert_component_when_node_has_action_variant!(Scenario, UiScenarioActions);
    insert_component_when_node_has_action_variant!(Script, UiScriptActions);
    if let Some(channel) = record.actions.iter().find_map(|action| match action {
        UiActionRecord::AudioSetting(record) => Some(record.action),
        _ => None,
    }) {
        let mut node = commands.entity(entity);
        node.insert(UiAudioSettingActions(UiActionSource::Node(node_index)));
        if !matches!(channel, openzt2_game_data::ui_document::action::audio_settings::UiAudioSettingAction::SetEightBitSound(_)) {
            node.insert(UiAudioVolumeControl(channel));
        }
    }
    insert_component_when_node_has_action_variant!(Information, UiInformationActions);
    insert_component_when_node_has_action_variant!(EnterImmersiveMode, UiImmersiveModeActions);
    insert_component_when_node_has_action_variant!(Staff, UiStaffActions);
    insert_component_when_node_has_action_variant!(Economy, UiEconomyActions);
    insert_component_when_node_has_action_variant!(Animal, UiAnimalActions);
    insert_component_when_node_has_action_variant!(Persistence, UiPersistenceActions);
    insert_component_when_node_has_action_variant!(Simulation, UiSimulationActions);
    if record.actions.iter().any(|action| matches!(action,
        UiActionRecord::Simulation(record)
            if matches!(record.action, UiSimulationAction::SetTemporarySimulationPauseHeld { .. })
    )) {
        commands.entity(entity).insert(AuthoredUiTemporarySimulationPause(false));
    }
}

pub(super) fn insert_authored_action_proxy_route(
    commands: &mut Commands,
    entity: Entity,
    source: UiActionSource,
    action: &UiActionRecord,
) {
    match action {
        UiActionRecord::Presentation(_) => {
            commands
                .entity(entity)
                .insert(UiPresentationActions(source));
        }
        UiActionRecord::Shell(_) => {
            commands.entity(entity).insert(UiShellActions(source));
        }
        UiActionRecord::PopulateCatalogueTypeList(_) => {
            commands
                .entity(entity)
                .insert(UiPopulateCatalogueTypeListActions(source));
        }
        UiActionRecord::Construction(_) => {
            commands
                .entity(entity)
                .insert(UiConstructionActions(source));
        }
        UiActionRecord::Camera(_) => {
            commands.entity(entity).insert(UiCameraActions(source));
        }
        UiActionRecord::Photo(_) => {
            commands.entity(entity).insert(UiPhotoActions(source));
        }
        UiActionRecord::StartResearchForSelectedCatalogueItem(_) => {
            commands.entity(entity).insert(UiResearchActions(source));
        }
        UiActionRecord::AnimalHealth(_) => {
            commands
                .entity(entity)
                .insert(UiAnimalHealthActions(source));
        }
        UiActionRecord::Show(_) => {
            commands.entity(entity).insert(UiShowActions(source));
        }
        UiActionRecord::Transport(_) => {
            commands.entity(entity).insert(UiTransportActions(source));
        }
        UiActionRecord::Scenario(_) => {
            commands.entity(entity).insert(UiScenarioActions(source));
        }
        UiActionRecord::Script(_) => {
            commands.entity(entity).insert(UiScriptActions(source));
        }
        UiActionRecord::AudioSetting(_) => {
            commands
                .entity(entity)
                .insert(UiAudioSettingActions(source));
        }
        UiActionRecord::Information(_) => {
            commands.entity(entity).insert(UiInformationActions(source));
        }
        UiActionRecord::EnterImmersiveMode(_) => {
            commands
                .entity(entity)
                .insert(UiImmersiveModeActions(source));
        }
        UiActionRecord::Staff(_) => {
            commands.entity(entity).insert(UiStaffActions(source));
        }
        UiActionRecord::Economy(_) => {
            commands.entity(entity).insert(UiEconomyActions(source));
        }
        UiActionRecord::Animal(_) => {
            commands.entity(entity).insert(UiAnimalActions(source));
        }
        UiActionRecord::Persistence(_) => {
            commands.entity(entity).insert(UiPersistenceActions(source));
        }
        UiActionRecord::Simulation(record) => {
            commands.entity(entity).insert(UiSimulationActions(source));
            if matches!(record.action, openzt2_game_data::ui_document::action::simulation_time::UiSimulationAction::SetTemporarySimulationPauseHeld { .. }) {
                        commands.entity(entity).insert((
                            Visibility::Inherited,
                            openzt2_world_state::simulation_time::simulation_control_types::AuthoredUiTemporarySimulationPause(false),
                        ));
                    }
        }
    };
}
