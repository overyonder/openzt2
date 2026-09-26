use openzt2_game_data::AssetId;

use crate::plugins::{
    progression::{
        adoption_and_content_availability_types::AnimalAdoptionAvailabilityPolicy,
        fame_history_types::FameHistorySample, fame_types::Fame, rating_types::ZooRating,
    },
    world_spawn::persistent_id_types::PersistentId,
};

#[derive(Debug, Clone, Copy)]
pub(super) struct ResearchProjectSnapshotRecord {
    pub(super) persistent_identifier: PersistentId,
    pub(super) definition_identifier: AssetId,
    pub(super) elapsed_simulation_ticks: u64,
    pub(super) required_simulation_ticks: u64,
    pub(super) paused: bool,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct EarnedProgressionAwardSnapshotRecord {
    pub(super) persistent_identifier: PersistentId,
    pub(super) definition_identifier: AssetId,
    pub(super) earned_simulation_tick: u64,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct ProgressionAwardDurationSnapshotRecord {
    pub(super) persistent_identifier: PersistentId,
    pub(super) award_definition_identifier: AssetId,
    pub(super) award_condition_index: u32,
    pub(super) satisfied_simulation_ticks: u64,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct ResearchAvailabilitySnapshotRecord {
    pub(super) persistent_identifier: PersistentId,
    pub(super) catalogue_item_identifier: AssetId,
    pub(super) available: bool,
    pub(super) remaining_unlock_ticks: Option<u64>,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct SpeciesAdoptionChanceSnapshotRecord {
    pub(super) persistent_identifier: PersistentId,
    pub(super) definition_identifier: AssetId,
    pub(super) adoption_chance_multiplier_permille: u16,
}

#[derive(Debug)]
pub(super) struct ProgressionSnapshotRecords {
    pub(super) fame: Fame,
    pub(super) zoo_rating: ZooRating,
    pub(super) unlocked_catalogue_definition_count: u32,
    pub(super) unlocked_catalogue_definition_bit_words: Vec<u64>,
    pub(super) scenario_award_points: u32,
    pub(super) endangered_animal_birth_count: u32,
    pub(super) animal_adoption_availability_policy: AnimalAdoptionAvailabilityPolicy,
    pub(super) fame_history_samples: Vec<FameHistorySample>,
    pub(super) research_projects: Vec<ResearchProjectSnapshotRecord>,
    pub(super) earned_progression_awards: Vec<EarnedProgressionAwardSnapshotRecord>,
    pub(super) award_condition_duration_progress: Vec<ProgressionAwardDurationSnapshotRecord>,
    pub(super) research_project_availability: Vec<ResearchAvailabilitySnapshotRecord>,
    pub(super) species_adoption_chances: Vec<SpeciesAdoptionChanceSnapshotRecord>,
}

impl ProgressionSnapshotRecords {
    pub(super) fn persistent_identifiers(&self) -> impl Iterator<Item = PersistentId> + '_ {
        self.research_projects
            .iter()
            .map(|record| record.persistent_identifier)
            .chain(
                self.earned_progression_awards
                    .iter()
                    .map(|record| record.persistent_identifier),
            )
            .chain(
                self.award_condition_duration_progress
                    .iter()
                    .map(|record| record.persistent_identifier),
            )
            .chain(
                self.research_project_availability
                    .iter()
                    .map(|record| record.persistent_identifier),
            )
            .chain(
                self.species_adoption_chances
                    .iter()
                    .map(|record| record.persistent_identifier),
            )
    }

    pub(super) fn snapshot_record_count(&self) -> u32 {
        1_u32
            .saturating_add(self.fame_history_samples.len() as u32)
            .saturating_add(self.research_projects.len() as u32)
            .saturating_add(self.earned_progression_awards.len() as u32)
            .saturating_add(self.award_condition_duration_progress.len() as u32)
            .saturating_add(self.research_project_availability.len() as u32)
            .saturating_add(self.species_adoption_chances.len() as u32)
    }
}
