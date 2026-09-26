use bevy::prelude::*;
use openzt2_game_data::ui_document::{action::UiActionRecord, node::UiNodeDefinition};

use crate::plugins::ui::authored_ui_action_projection_components::{
    UiActionSource, UiAnimalActions, UiAnimalHealthActions, UiAudioSettingActions, UiCameraActions,
    UiConstructionActions, UiEconomyActions, UiImmersiveModeActions, UiInformationActions,
    UiPersistenceActions, UiPhotoActions, UiPopulateCatalogueTypeListActions,
    UiPresentationActions, UiResearchActions, UiScenarioActions, UiShellActions, UiShowActions,
    UiSimulationActions, UiStaffActions, UiTransportActions,
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
    insert_component_when_node_has_action_variant!(AudioSetting, UiAudioSettingActions);
    insert_component_when_node_has_action_variant!(Information, UiInformationActions);
    insert_component_when_node_has_action_variant!(EnterImmersiveMode, UiImmersiveModeActions);
    insert_component_when_node_has_action_variant!(Staff, UiStaffActions);
    insert_component_when_node_has_action_variant!(Economy, UiEconomyActions);
    insert_component_when_node_has_action_variant!(Animal, UiAnimalActions);
    insert_component_when_node_has_action_variant!(Persistence, UiPersistenceActions);
    insert_component_when_node_has_action_variant!(Simulation, UiSimulationActions);
}
