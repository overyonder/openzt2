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
pub(super) struct ResolvedAnimalSpawnComponents {
    pub(super) sex: Sex,
    pub(super) life_stage: LifeStage,
    pub(super) variant: AssetId,
    pub(super) definition: AssetId,
    pub(super) life_stage_start_ticks: AuthoredAnimalLifeStageStartTicks,
    pub(super) reproduction: ReproductionTraits,
    pub(super) waste: AnimalWasteCycle,
    pub(super) presentation_model: SelectedAnimalPresentationModel,
    pub(super) scale: f32,
    pub(super) species_flags: SpeciesFlags,
    pub(super) variant_flags: SpeciesVariantFlags,
}
