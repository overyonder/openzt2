use bevy::prelude::*;
use openzt2_game_data::{
    species::{LifeStage, Sex},
    AssetId,
};

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Animal;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpeciesHandle {
    pub species: AssetId,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnimalVariant(pub AssetId);

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnimalSex(pub Sex);

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnimalLifeStage(pub LifeStage);

/// Authored life-stage boundaries used by lifecycle systems.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct AuthoredAnimalLifeStageStartTicks {
    pub stage_start_ticks: [u64; 4],
}

/// Species reproduction defaults. Current pregnancy remains separately owned.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReproductionTraits {
    pub gestation_ticks: u64,
    pub litter: [u16; 2],
}

/// Authored waste production policy; emitted waste is owned by maintenance.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnimalWasteCycle {
    pub definition: AssetId,
    pub interval_ticks: u32,
    pub units: u16,
}

/// Selected render facts for this animal variant. The selected scale itself is
/// projected into the entity's ordinary [`Transform`].
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectedAnimalPresentationModel {
    pub model: AssetId,
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Swims;

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Predator;

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Prey;

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Adoptable;

/// Scenario-authored restriction on the ordinary release action. Absence means
/// the action is available; this marker is queried directly by UI consumers.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReleaseRestricted;

/// Scenario-authored restriction on the ordinary crate action.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CratingRestricted;

/// Conservation classification projected from the authored species catalogue.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EndangeredSpecies;

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Sterile;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Age {
    pub born_tick: u64,
}

impl Age {
    pub const fn ticks_at(self, tick: u64) -> u64 {
        tick.saturating_sub(self.born_tick)
    }
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pregnancy {
    pub father: Entity,
    pub due_tick: u64,
}

/// The animal's `mate` relationship, set by `scripts/setmates.lua` after mating.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mate(pub Entity);

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Parents {
    pub mother: Option<Entity>,
    pub father: Option<Entity>,
}

impl Parents {
    pub const NONE: Self = Self {
        mother: None,
        father: None,
    };
}

#[derive(Component)]
pub struct AnimalPresentationOwner(pub Entity);
