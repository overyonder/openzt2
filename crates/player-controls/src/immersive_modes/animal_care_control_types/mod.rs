use bevy::prelude::*;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DiseaseTreatmentControl;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TranquilizerControl;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AnimalCareTarget {
    pub(crate) animal_entity: Option<Entity>,
}

#[derive(Component, Debug, Clone, Copy, Default)]
pub(crate) struct TranquilizerAudioFeedback {
    pub(crate) targeted_animal_entity: Option<Entity>,
    pub(crate) charge_is_ready: bool,
    pub(crate) trigger_is_held: bool,
}
