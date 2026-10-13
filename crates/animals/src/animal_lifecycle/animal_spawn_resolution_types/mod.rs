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
pub struct ResolvedAnimalSpawnComponents {
    pub sex: Sex,
    pub life_stage: LifeStage,
    pub variant: AssetId,
    pub definition: AssetId,
    pub life_stage_start_ticks: AuthoredAnimalLifeStageStartTicks,
    pub reproduction: ReproductionTraits,
    pub waste: AnimalWasteCycle,
    pub presentation_model: SelectedAnimalPresentationModel,
    pub scale: f32,
    pub species_flags: SpeciesFlags,
    pub variant_flags: SpeciesVariantFlags,
}
