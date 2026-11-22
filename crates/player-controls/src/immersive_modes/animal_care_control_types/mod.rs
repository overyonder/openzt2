use bevy::prelude::*;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DiseaseTreatmentControl;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TranquilizerControl;

#[derive(Component, Clone, Copy, Default, PartialEq)]
pub(super) struct TranquilizerAimSample {
    pub(super) camera: Option<Entity>,
    pub(super) target: Option<Entity>,
    pub(super) distance_m: f32,
    pub(super) in_range: bool,
}

#[derive(Component, Default)]
pub(super) struct TranquilizerMisfireTimer(pub(super) Option<Timer>);

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
