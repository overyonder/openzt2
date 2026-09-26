use bevy::prelude::*;
use openzt2_game_data::{
    species::{LifeStage, Sex},
    AssetId,
};

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Animal;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SpeciesHandle {
    pub(crate) species: AssetId,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AnimalVariant(pub AssetId);

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AnimalSex(pub Sex);

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AnimalLifeStage(pub LifeStage);

/// Authored life-stage boundaries used by lifecycle systems.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub(crate) struct AuthoredAnimalLifeStageStartTicks {
    pub(crate) stage_start_ticks: [u64; 4],
}

/// Species reproduction defaults. Current pregnancy remains separately owned.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ReproductionTraits {
    pub(crate) gestation_ticks: u64,
    pub(crate) litter: [u16; 2],
}

/// Authored waste production policy; emitted waste is owned by maintenance.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AnimalWasteCycle {
    pub(crate) definition: AssetId,
    pub(crate) interval_ticks: u32,
    pub(crate) units: u16,
}

/// Selected render facts for this animal variant. The selected scale itself is
/// projected into the entity's ordinary [`Transform`].
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SelectedAnimalPresentationModel {
    pub(crate) model: AssetId,
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Swims;

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Predator;

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Prey;

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Adoptable;

/// Scenario-authored restriction on the ordinary release action. Absence means
/// the action is available; this marker is queried directly by UI consumers.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ReleaseRestricted;

/// Scenario-authored restriction on the ordinary crate action.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct CratingRestricted;

/// Conservation classification projected from the authored species catalogue.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct EndangeredSpecies;

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Sterile;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Age {
    pub(crate) born_tick: u64,
}

impl Age {
    pub(crate) const fn ticks_at(self, tick: u64) -> u64 {
        tick.saturating_sub(self.born_tick)
    }
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Pregnancy {
    pub(crate) father: Entity,
    pub(crate) due_tick: u64,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Parents {
    pub(crate) mother: Option<Entity>,
    pub(crate) father: Option<Entity>,
}

impl Parents {
    pub(crate) const NONE: Self = Self {
        mother: None,
        father: None,
    };
}
