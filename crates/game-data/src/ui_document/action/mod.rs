//! UI triggers and actions.

use serde::{Deserialize, Serialize};

pub mod animal_health;
pub mod animal_shows;
pub mod animals;
pub mod audio_settings;
pub mod camera;
pub mod catalogue_type_list;
pub mod construction;
pub mod economy;
pub mod immersive_mode;
pub mod information;
pub mod persistence;
pub mod photography;
pub mod presentation;
pub mod research;
pub mod scenarios;
pub mod shell_navigation;
pub mod simulation_time;
pub mod staff_management;
pub mod transportation;

use animal_health::UiAnimalHealthActionRecord;
use animal_shows::UiShowActionRecord;
use animals::UiAnimalActionRecord;
use audio_settings::UiAudioSettingActionRecord;
use camera::UiCameraActionRecord;
use catalogue_type_list::UiPopulateCatalogueTypeListActionRecord;
use construction::UiConstructionActionRecord;
use economy::UiEconomyActionRecord;
use immersive_mode::UiEnterImmersiveModeActionRecord;
use information::UiInformationActionRecord;
use persistence::UiPersistenceActionRecord;
use photography::UiPhotoActionRecord;
use presentation::UiPresentationActionRecord;
use research::UiStartResearchForSelectedCatalogueItemActionRecord;
use scenarios::UiScenarioActionRecord;
use shell_navigation::UiShellActionRecord;
use simulation_time::UiSimulationActionRecord;
use staff_management::UiStaffActionRecord;
use transportation::UiTransportActionRecord;

#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum UiTrigger {
    Press,
    Change,
    Submit,
    Cancel,
    Focus,
    Blur,
    Enter,
    Leave,
    AnimationCompleted,
    Show,
    Hide,
    On,
    Off,
}

/// An action in a node event list or hotkey. Node lists preserve authored order.
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub enum UiActionRecord {
    Presentation(UiPresentationActionRecord),
    Shell(UiShellActionRecord),
    PopulateCatalogueTypeList(UiPopulateCatalogueTypeListActionRecord),
    Construction(UiConstructionActionRecord),
    Camera(UiCameraActionRecord),
    Photo(UiPhotoActionRecord),
    StartResearchForSelectedCatalogueItem(UiStartResearchForSelectedCatalogueItemActionRecord),
    AnimalHealth(UiAnimalHealthActionRecord),
    Show(UiShowActionRecord),
    Transport(UiTransportActionRecord),
    Scenario(UiScenarioActionRecord),
    AudioSetting(UiAudioSettingActionRecord),
    Information(UiInformationActionRecord),
    EnterImmersiveMode(UiEnterImmersiveModeActionRecord),
    Staff(UiStaffActionRecord),
    Economy(UiEconomyActionRecord),
    Animal(UiAnimalActionRecord),
    Persistence(UiPersistenceActionRecord),
    Simulation(UiSimulationActionRecord),
}
