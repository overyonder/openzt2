use bevy::prelude::*;
use openzt2_game_data::ui_document::{
    action::{
        animal_health::UiAnimalHealthActionRecord, animal_shows::UiShowActionRecord,
        animals::UiAnimalActionRecord, audio_settings::UiAudioSettingActionRecord,
        camera::UiCameraActionRecord, catalogue_type_list::UiPopulateCatalogueTypeListActionRecord,
        construction::UiConstructionActionRecord, economy::UiEconomyActionRecord,
        immersive_mode::UiEnterImmersiveModeActionRecord, information::UiInformationActionRecord,
        persistence::UiPersistenceActionRecord, photography::UiPhotoActionRecord,
        presentation::UiPresentationActionRecord,
        research::UiStartResearchForSelectedCatalogueItemActionRecord,
        scenarios::UiScenarioActionRecord, shell_navigation::UiShellActionRecord,
        simulation_time::UiSimulationActionRecord, staff_management::UiStaffActionRecord,
        transportation::UiTransportActionRecord, UiActionRecord,
    },
    widget::UiWidgetRecord,
};

use crate::assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UiActionSource {
    Node(u32),
    Hotkey { node: u32, hotkey: u32 },
    TimedEvent { node: u32, event: u32 },
}

macro_rules! define_ui_action_projection_component {
    ($component:ident, $record:ty, $variant:ident) => {
        #[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
        pub(crate) struct $component(pub(crate) UiActionSource);

        impl $component {
            pub(crate) fn authored_action_records<'a>(
                &self,
                document: &'a UiDocumentAsset,
            ) -> impl Iterator<Item = &'a $record> {
                canonical_ui_action_records_for_projection_source(self.0, document)
                    .iter()
                    .filter_map(|action| match action {
                        UiActionRecord::$variant(record) => Some(record),
                        _ => None,
                    })
            }
        }
    };
}

fn canonical_ui_action_records_for_projection_source(
    source: UiActionSource,
    document: &UiDocumentAsset,
) -> &[UiActionRecord] {
    let canonical_ui_document = document.canonical_ui_document();
    match source {
        UiActionSource::Node(node) => canonical_ui_document
            .nodes
            .get(node as usize)
            .map_or(&[], |node| node.actions.as_slice()),
        UiActionSource::Hotkey { node, hotkey } => canonical_ui_document
            .nodes
            .get(node as usize)
            .and_then(|node| node.hotkeys.get(hotkey as usize))
            .map_or(&[], |hotkey| std::slice::from_ref(&hotkey.action)),
        UiActionSource::TimedEvent { .. } => &[],
    }
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct UiPresentationActions(pub(crate) UiActionSource);

impl UiPresentationActions {
    pub(crate) fn authored_action_records<'a>(
        &self,
        document: &'a UiDocumentAsset,
    ) -> impl Iterator<Item = &'a UiPresentationActionRecord> {
        let timed_event = match self.0 {
            UiActionSource::TimedEvent { node, event } => document
                .canonical_ui_document()
                .nodes
                .get(node as usize)
                .and_then(|node| {
                    let UiWidgetRecord::TimedSequence { events } = &node.widget else {
                        return None;
                    };
                    events.get(event as usize).map(|event| &event.action)
                }),
            UiActionSource::Node(_) | UiActionSource::Hotkey { .. } => None,
        };
        canonical_ui_action_records_for_projection_source(self.0, document)
            .iter()
            .filter_map(|action| match action {
                UiActionRecord::Presentation(record) => Some(record),
                _ => None,
            })
            .chain(timed_event)
    }
}

define_ui_action_projection_component!(UiShellActions, UiShellActionRecord, Shell);
define_ui_action_projection_component!(
    UiPopulateCatalogueTypeListActions,
    UiPopulateCatalogueTypeListActionRecord,
    PopulateCatalogueTypeList
);
define_ui_action_projection_component!(
    UiConstructionActions,
    UiConstructionActionRecord,
    Construction
);
define_ui_action_projection_component!(UiCameraActions, UiCameraActionRecord, Camera);
define_ui_action_projection_component!(UiPhotoActions, UiPhotoActionRecord, Photo);
define_ui_action_projection_component!(
    UiResearchActions,
    UiStartResearchForSelectedCatalogueItemActionRecord,
    StartResearchForSelectedCatalogueItem
);
define_ui_action_projection_component!(UiShowActions, UiShowActionRecord, Show);
define_ui_action_projection_component!(
    UiAnimalHealthActions,
    UiAnimalHealthActionRecord,
    AnimalHealth
);
define_ui_action_projection_component!(UiTransportActions, UiTransportActionRecord, Transport);
define_ui_action_projection_component!(UiScenarioActions, UiScenarioActionRecord, Scenario);
define_ui_action_projection_component!(
    UiAudioSettingActions,
    UiAudioSettingActionRecord,
    AudioSetting
);
define_ui_action_projection_component!(
    UiInformationActions,
    UiInformationActionRecord,
    Information
);
define_ui_action_projection_component!(
    UiImmersiveModeActions,
    UiEnterImmersiveModeActionRecord,
    EnterImmersiveMode
);
define_ui_action_projection_component!(UiStaffActions, UiStaffActionRecord, Staff);
define_ui_action_projection_component!(UiEconomyActions, UiEconomyActionRecord, Economy);
define_ui_action_projection_component!(UiAnimalActions, UiAnimalActionRecord, Animal);
define_ui_action_projection_component!(
    UiPersistenceActions,
    UiPersistenceActionRecord,
    Persistence
);
define_ui_action_projection_component!(UiSimulationActions, UiSimulationActionRecord, Simulation);
