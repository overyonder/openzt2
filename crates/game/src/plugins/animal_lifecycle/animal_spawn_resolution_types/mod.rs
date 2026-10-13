use openzt2_game_data::{
    species::{LifeStage, Sex, SpeciesFlags, SpeciesVariantFlags},
    AssetId,
};

use super::types::{
    AnimalWasteCycle, AuthoredAnimalLifeStageStartTicks, ReproductionTraits,
    SelectedAnimalPresentationModel,
};

/// Complete typed facts selected from one species and variant for a live spawn.
#[derive(Clone, Copy)]
pub(crate) struct ResolvedAnimalSpawnComponents {
    pub(crate) sex: Sex,
    pub(crate) life_stage: LifeStage,
    pub(crate) variant: AssetId,
    pub(crate) definition: AssetId,
    pub(crate) life_stage_start_ticks: AuthoredAnimalLifeStageStartTicks,
    pub(crate) reproduction: ReproductionTraits,
    pub(crate) waste: AnimalWasteCycle,
    pub(crate) presentation_model: SelectedAnimalPresentationModel,
    pub(crate) scale: f32,
    pub(crate) species_flags: SpeciesFlags,
    pub(crate) variant_flags: SpeciesVariantFlags,
}
