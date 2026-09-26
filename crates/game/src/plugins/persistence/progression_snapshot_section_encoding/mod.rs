use openzt2_game_data::AssetId;

use crate::plugins::{
    progression::{
        adoption_and_content_availability_types::AnimalAdoptionAvailabilityPolicy,
        fame_history_types::FameHistorySample, fame_types::Fame, rating_types::ZooRating,
    },
    world_spawn::persistent_id_types::PersistentId,
};

use super::{
    persistence_failure_types::WorldSnapshotPersistenceFailure,
    progression_snapshot_types::{
        EarnedProgressionAwardSnapshotRecord, ProgressionAwardDurationSnapshotRecord,
        ProgressionSnapshotRecords, ResearchAvailabilitySnapshotRecord,
        ResearchProjectSnapshotRecord, SpeciesAdoptionChanceSnapshotRecord,
    },
    progression_snapshot_validation::validate_progression_snapshot_record_invariants,
};

const GLOBAL_PROGRESSION_RECORD_BYTE_COUNT: usize = 2 + 5 + 16 + 4 + 4 + 4 + 4 + 1 + 2 + 4;
const PROGRESSION_ENTITY_COLLECTION_COUNTS_BYTE_COUNT: usize = 5 * 4;
const FAME_HISTORY_SNAPSHOT_RECORD_BYTE_COUNT: usize = 5;
const RESEARCH_PROJECT_SNAPSHOT_RECORD_BYTE_COUNT: usize = 8 + 16 + 8 + 8 + 1;
const EARNED_AWARD_SNAPSHOT_RECORD_BYTE_COUNT: usize = 8 + 16 + 8;
const AWARD_DURATION_SNAPSHOT_RECORD_BYTE_COUNT: usize = 8 + 16 + 4 + 8;
const RESEARCH_AVAILABILITY_SNAPSHOT_RECORD_BYTE_COUNT: usize = 8 + 16 + 1 + 1 + 8;
const SPECIES_ADOPTION_CHANCE_SNAPSHOT_RECORD_BYTE_COUNT: usize = 8 + 16 + 2;

pub(super) fn append_encoded_progression_snapshot_section(
    progression_snapshot_section_bytes: &mut Vec<u8>,
    progression_snapshot_records: &ProgressionSnapshotRecords,
) {
    progression_snapshot_section_bytes.extend_from_slice(&[
        progression_snapshot_records.fame.half_stars,
        progression_snapshot_records.fame.maximum_reached,
    ]);
    progression_snapshot_section_bytes.push(u8::from(
        progression_snapshot_records
            .fame
            .maximum_percent_reached
            .is_some(),
    ));
    progression_snapshot_section_bytes.extend_from_slice(
        &progression_snapshot_records
            .fame
            .maximum_percent_reached
            .unwrap_or(0.0)
            .to_bits()
            .to_le_bytes(),
    );
    for zoo_rating_component_value_permille in
        zoo_rating_component_values_in_wire_order(&progression_snapshot_records.zoo_rating)
    {
        progression_snapshot_section_bytes
            .extend_from_slice(&zoo_rating_component_value_permille.to_le_bytes());
    }
    progression_snapshot_section_bytes.extend_from_slice(
        &progression_snapshot_records
            .unlocked_catalogue_definition_count
            .to_le_bytes(),
    );
    progression_snapshot_section_bytes.extend_from_slice(
        &(progression_snapshot_records
            .unlocked_catalogue_definition_bit_words
            .len() as u32)
            .to_le_bytes(),
    );
    progression_snapshot_records
        .unlocked_catalogue_definition_bit_words
        .iter()
        .for_each(|unlocked_catalogue_definition_bit_word| {
            progression_snapshot_section_bytes
                .extend_from_slice(&unlocked_catalogue_definition_bit_word.to_le_bytes());
        });
    progression_snapshot_section_bytes.extend_from_slice(
        &progression_snapshot_records
            .scenario_award_points
            .to_le_bytes(),
    );
    progression_snapshot_section_bytes.extend_from_slice(
        &progression_snapshot_records
            .endangered_animal_birth_count
            .to_le_bytes(),
    );
    progression_snapshot_section_bytes.push(u8::from(
        progression_snapshot_records
            .animal_adoption_availability_policy
            .enabled,
    ));
    progression_snapshot_section_bytes.extend_from_slice(
        &progression_snapshot_records
            .animal_adoption_availability_policy
            .multiplier_permille
            .to_le_bytes(),
    );
    progression_snapshot_section_bytes.extend_from_slice(
        &(progression_snapshot_records.fame_history_samples.len() as u32).to_le_bytes(),
    );
    for fame_history_sample in &progression_snapshot_records.fame_history_samples {
        progression_snapshot_section_bytes
            .extend_from_slice(&fame_history_sample.month_index.to_le_bytes());
        progression_snapshot_section_bytes.push(fame_history_sample.half_stars);
    }
    progression_snapshot_section_bytes.extend_from_slice(
        &(progression_snapshot_records.research_projects.len() as u32).to_le_bytes(),
    );
    progression_snapshot_section_bytes.extend_from_slice(
        &(progression_snapshot_records.earned_progression_awards.len() as u32).to_le_bytes(),
    );
    progression_snapshot_section_bytes.extend_from_slice(
        &(progression_snapshot_records
            .award_condition_duration_progress
            .len() as u32)
            .to_le_bytes(),
    );
    progression_snapshot_section_bytes.extend_from_slice(
        &(progression_snapshot_records
            .research_project_availability
            .len() as u32)
            .to_le_bytes(),
    );
    progression_snapshot_section_bytes.extend_from_slice(
        &(progression_snapshot_records.species_adoption_chances.len() as u32).to_le_bytes(),
    );
    for research_project_snapshot_record in &progression_snapshot_records.research_projects {
        progression_snapshot_section_bytes.extend_from_slice(
            &research_project_snapshot_record
                .persistent_identifier
                .0
                .to_le_bytes(),
        );
        progression_snapshot_section_bytes
            .extend_from_slice(&research_project_snapshot_record.definition_identifier.0);
        progression_snapshot_section_bytes.extend_from_slice(
            &research_project_snapshot_record
                .elapsed_simulation_ticks
                .to_le_bytes(),
        );
        progression_snapshot_section_bytes.extend_from_slice(
            &research_project_snapshot_record
                .required_simulation_ticks
                .to_le_bytes(),
        );
        progression_snapshot_section_bytes.push(u8::from(research_project_snapshot_record.paused));
    }
    for earned_award_snapshot_record in &progression_snapshot_records.earned_progression_awards {
        progression_snapshot_section_bytes.extend_from_slice(
            &earned_award_snapshot_record
                .persistent_identifier
                .0
                .to_le_bytes(),
        );
        progression_snapshot_section_bytes
            .extend_from_slice(&earned_award_snapshot_record.definition_identifier.0);
        progression_snapshot_section_bytes.extend_from_slice(
            &earned_award_snapshot_record
                .earned_simulation_tick
                .to_le_bytes(),
        );
    }
    for award_duration_snapshot_record in
        &progression_snapshot_records.award_condition_duration_progress
    {
        progression_snapshot_section_bytes.extend_from_slice(
            &award_duration_snapshot_record
                .persistent_identifier
                .0
                .to_le_bytes(),
        );
        progression_snapshot_section_bytes
            .extend_from_slice(&award_duration_snapshot_record.award_definition_identifier.0);
        progression_snapshot_section_bytes.extend_from_slice(
            &award_duration_snapshot_record
                .award_condition_index
                .to_le_bytes(),
        );
        progression_snapshot_section_bytes.extend_from_slice(
            &award_duration_snapshot_record
                .satisfied_simulation_ticks
                .to_le_bytes(),
        );
    }
    for research_availability_snapshot_record in
        &progression_snapshot_records.research_project_availability
    {
        progression_snapshot_section_bytes.extend_from_slice(
            &research_availability_snapshot_record
                .persistent_identifier
                .0
                .to_le_bytes(),
        );
        progression_snapshot_section_bytes.extend_from_slice(
            &research_availability_snapshot_record
                .catalogue_item_identifier
                .0,
        );
        progression_snapshot_section_bytes
            .push(u8::from(research_availability_snapshot_record.available));
        progression_snapshot_section_bytes.push(u8::from(
            research_availability_snapshot_record
                .remaining_unlock_ticks
                .is_some(),
        ));
        progression_snapshot_section_bytes.extend_from_slice(
            &research_availability_snapshot_record
                .remaining_unlock_ticks
                .unwrap_or(0)
                .to_le_bytes(),
        );
    }
    for species_adoption_chance_snapshot_record in
        &progression_snapshot_records.species_adoption_chances
    {
        progression_snapshot_section_bytes.extend_from_slice(
            &species_adoption_chance_snapshot_record
                .persistent_identifier
                .0
                .to_le_bytes(),
        );
        progression_snapshot_section_bytes.extend_from_slice(
            &species_adoption_chance_snapshot_record
                .definition_identifier
                .0,
        );
        progression_snapshot_section_bytes.extend_from_slice(
            &species_adoption_chance_snapshot_record
                .adoption_chance_multiplier_permille
                .to_le_bytes(),
        );
    }
}

pub(super) fn decode_and_validate_progression_snapshot_section(
    progression_snapshot_section_bytes: &[u8],
    declared_progression_snapshot_record_count: u32,
) -> Result<ProgressionSnapshotRecords, WorldSnapshotPersistenceFailure> {
    let mut progression_snapshot_section_reader =
        ProgressionSnapshotSectionReader::new(progression_snapshot_section_bytes);
    let fame = Fame {
        half_stars: progression_snapshot_section_reader.read_u8()?,
        maximum_reached: progression_snapshot_section_reader.read_u8()?,
        maximum_percent_reached: {
            let present = progression_snapshot_section_reader.read_boolean()?;
            let value = f32::from_bits(progression_snapshot_section_reader.read_u32()?);
            present.then_some(value)
        },
    };
    let zoo_rating = ZooRating {
        animal_welfare_permille: progression_snapshot_section_reader.read_u16()?,
        guest_satisfaction_permille: progression_snapshot_section_reader.read_u16()?,
        education_permille: progression_snapshot_section_reader.read_u16()?,
        variety_permille: progression_snapshot_section_reader.read_u16()?,
        scenery_permille: progression_snapshot_section_reader.read_u16()?,
        finance_permille: progression_snapshot_section_reader.read_u16()?,
        cleanliness_permille: progression_snapshot_section_reader.read_u16()?,
        overall_permille: progression_snapshot_section_reader.read_u16()?,
        ..Default::default()
    };
    let unlocked_catalogue_definition_count = progression_snapshot_section_reader.read_u32()?;
    let unlocked_catalogue_definition_bit_word_count =
        progression_snapshot_section_reader.read_u32()? as usize;
    if unlocked_catalogue_definition_bit_word_count
        != unlocked_catalogue_definition_count.div_ceil(64) as usize
    {
        return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
    }
    let unlocked_catalogue_definition_bit_words = (0..unlocked_catalogue_definition_bit_word_count)
        .map(|_| progression_snapshot_section_reader.read_u64())
        .collect::<Result<Vec<_>, _>>()?;
    let scenario_award_points = progression_snapshot_section_reader.read_u32()?;
    let endangered_animal_birth_count = progression_snapshot_section_reader.read_u32()?;
    let animal_adoption_availability_policy = AnimalAdoptionAvailabilityPolicy {
        enabled: progression_snapshot_section_reader.read_boolean()?,
        multiplier_permille: progression_snapshot_section_reader.read_u16()?,
    };
    let fame_history_sample_count = progression_snapshot_section_reader.read_u32()? as usize;
    let fame_history_samples = (0..fame_history_sample_count)
        .map(|_| {
            Ok(FameHistorySample {
                month_index: progression_snapshot_section_reader.read_u32()?,
                half_stars: progression_snapshot_section_reader.read_u8()?,
            })
        })
        .collect::<Result<Vec<_>, WorldSnapshotPersistenceFailure>>()?;
    let research_project_record_count = progression_snapshot_section_reader.read_u32()? as usize;
    let earned_award_record_count = progression_snapshot_section_reader.read_u32()? as usize;
    let award_duration_record_count = progression_snapshot_section_reader.read_u32()? as usize;
    let research_availability_record_count =
        progression_snapshot_section_reader.read_u32()? as usize;
    let species_adoption_chance_record_count =
        progression_snapshot_section_reader.read_u32()? as usize;
    let calculated_progression_snapshot_record_count = 1_usize
        .checked_add(fame_history_sample_count)
        .and_then(|accumulated_record_count| {
            accumulated_record_count.checked_add(research_project_record_count)
        })
        .and_then(|accumulated_record_count| {
            accumulated_record_count.checked_add(earned_award_record_count)
        })
        .and_then(|accumulated_record_count| {
            accumulated_record_count.checked_add(award_duration_record_count)
        })
        .and_then(|accumulated_record_count| {
            accumulated_record_count.checked_add(research_availability_record_count)
        })
        .and_then(|accumulated_record_count| {
            accumulated_record_count.checked_add(species_adoption_chance_record_count)
        })
        .ok_or(WorldSnapshotPersistenceFailure::CapacityExceeded)?;
    if calculated_progression_snapshot_record_count
        != declared_progression_snapshot_record_count as usize
    {
        return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
    }
    let calculated_progression_snapshot_section_byte_count = GLOBAL_PROGRESSION_RECORD_BYTE_COUNT
        .checked_add(unlocked_catalogue_definition_bit_word_count.saturating_mul(8))
        .and_then(|accumulated_byte_count| {
            accumulated_byte_count.checked_add(
                fame_history_sample_count.saturating_mul(FAME_HISTORY_SNAPSHOT_RECORD_BYTE_COUNT),
            )
        })
        .and_then(|accumulated_byte_count| {
            accumulated_byte_count.checked_add(PROGRESSION_ENTITY_COLLECTION_COUNTS_BYTE_COUNT)
        })
        .and_then(|accumulated_byte_count| {
            accumulated_byte_count.checked_add(
                research_project_record_count
                    .saturating_mul(RESEARCH_PROJECT_SNAPSHOT_RECORD_BYTE_COUNT),
            )
        })
        .and_then(|accumulated_byte_count| {
            accumulated_byte_count.checked_add(
                earned_award_record_count.saturating_mul(EARNED_AWARD_SNAPSHOT_RECORD_BYTE_COUNT),
            )
        })
        .and_then(|accumulated_byte_count| {
            accumulated_byte_count.checked_add(
                award_duration_record_count
                    .saturating_mul(AWARD_DURATION_SNAPSHOT_RECORD_BYTE_COUNT),
            )
        })
        .and_then(|accumulated_byte_count| {
            accumulated_byte_count.checked_add(
                research_availability_record_count
                    .saturating_mul(RESEARCH_AVAILABILITY_SNAPSHOT_RECORD_BYTE_COUNT),
            )
        })
        .and_then(|accumulated_byte_count| {
            accumulated_byte_count.checked_add(
                species_adoption_chance_record_count
                    .saturating_mul(SPECIES_ADOPTION_CHANCE_SNAPSHOT_RECORD_BYTE_COUNT),
            )
        })
        .ok_or(WorldSnapshotPersistenceFailure::CapacityExceeded)?;
    if calculated_progression_snapshot_section_byte_count
        != progression_snapshot_section_bytes.len()
    {
        return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
    }
    let research_projects = (0..research_project_record_count)
        .map(|_| {
            Ok(ResearchProjectSnapshotRecord {
                persistent_identifier: PersistentId(
                    progression_snapshot_section_reader.read_u64()?,
                ),
                definition_identifier: progression_snapshot_section_reader
                    .read_asset_identifier()?,
                elapsed_simulation_ticks: progression_snapshot_section_reader.read_u64()?,
                required_simulation_ticks: progression_snapshot_section_reader.read_u64()?,
                paused: progression_snapshot_section_reader.read_boolean()?,
            })
        })
        .collect::<Result<Vec<_>, WorldSnapshotPersistenceFailure>>()?;
    let earned_progression_awards = (0..earned_award_record_count)
        .map(|_| {
            Ok(EarnedProgressionAwardSnapshotRecord {
                persistent_identifier: PersistentId(
                    progression_snapshot_section_reader.read_u64()?,
                ),
                definition_identifier: progression_snapshot_section_reader
                    .read_asset_identifier()?,
                earned_simulation_tick: progression_snapshot_section_reader.read_u64()?,
            })
        })
        .collect::<Result<Vec<_>, WorldSnapshotPersistenceFailure>>()?;
    let award_condition_duration_progress = (0..award_duration_record_count)
        .map(|_| {
            Ok(ProgressionAwardDurationSnapshotRecord {
                persistent_identifier: PersistentId(
                    progression_snapshot_section_reader.read_u64()?,
                ),
                award_definition_identifier: progression_snapshot_section_reader
                    .read_asset_identifier()?,
                award_condition_index: progression_snapshot_section_reader.read_u32()?,
                satisfied_simulation_ticks: progression_snapshot_section_reader.read_u64()?,
            })
        })
        .collect::<Result<Vec<_>, WorldSnapshotPersistenceFailure>>()?;
    let research_project_availability = (0..research_availability_record_count)
        .map(|_| {
            let persistent_identifier =
                PersistentId(progression_snapshot_section_reader.read_u64()?);
            let catalogue_item_identifier =
                progression_snapshot_section_reader.read_asset_identifier()?;
            let research_project_available = progression_snapshot_section_reader.read_boolean()?;
            let has_unlock_countdown = progression_snapshot_section_reader.read_boolean()?;
            let remaining_unlock_ticks = progression_snapshot_section_reader.read_u64()?;
            if has_unlock_countdown && (remaining_unlock_ticks == 0 || research_project_available)
                || !has_unlock_countdown && remaining_unlock_ticks != 0
            {
                return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
            }
            Ok(ResearchAvailabilitySnapshotRecord {
                persistent_identifier,
                catalogue_item_identifier,
                available: research_project_available,
                remaining_unlock_ticks: has_unlock_countdown.then_some(remaining_unlock_ticks),
            })
        })
        .collect::<Result<Vec<_>, WorldSnapshotPersistenceFailure>>()?;
    let species_adoption_chances = (0..species_adoption_chance_record_count)
        .map(|_| {
            Ok(SpeciesAdoptionChanceSnapshotRecord {
                persistent_identifier: PersistentId(
                    progression_snapshot_section_reader.read_u64()?,
                ),
                definition_identifier: progression_snapshot_section_reader
                    .read_asset_identifier()?,
                adoption_chance_multiplier_permille: progression_snapshot_section_reader
                    .read_u16()?,
            })
        })
        .collect::<Result<Vec<_>, WorldSnapshotPersistenceFailure>>()?;
    let progression_snapshot_records = ProgressionSnapshotRecords {
        fame,
        zoo_rating,
        unlocked_catalogue_definition_count,
        unlocked_catalogue_definition_bit_words,
        scenario_award_points,
        endangered_animal_birth_count,
        animal_adoption_availability_policy,
        fame_history_samples,
        research_projects,
        earned_progression_awards,
        award_condition_duration_progress,
        research_project_availability,
        species_adoption_chances,
    };
    if progression_snapshot_section_reader.remaining_byte_count() != 0 {
        return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
    }
    validate_progression_snapshot_record_invariants(&progression_snapshot_records)?;
    Ok(progression_snapshot_records)
}

fn zoo_rating_component_values_in_wire_order(zoo_rating: &ZooRating) -> [u16; 8] {
    [
        zoo_rating.animal_welfare_permille,
        zoo_rating.guest_satisfaction_permille,
        zoo_rating.education_permille,
        zoo_rating.variety_permille,
        zoo_rating.scenery_permille,
        zoo_rating.finance_permille,
        zoo_rating.cleanliness_permille,
        zoo_rating.overall_permille,
    ]
}

struct ProgressionSnapshotSectionReader<'a> {
    progression_snapshot_section_bytes: &'a [u8],
    current_byte_offset: usize,
}

impl<'a> ProgressionSnapshotSectionReader<'a> {
    const fn new(progression_snapshot_section_bytes: &'a [u8]) -> Self {
        Self {
            progression_snapshot_section_bytes,
            current_byte_offset: 0,
        }
    }

    fn read_fixed_byte_array<const BYTE_COUNT: usize>(
        &mut self,
    ) -> Result<[u8; BYTE_COUNT], WorldSnapshotPersistenceFailure> {
        let value_end_byte_offset = self
            .current_byte_offset
            .checked_add(BYTE_COUNT)
            .ok_or(WorldSnapshotPersistenceFailure::CapacityExceeded)?;
        let fixed_value_bytes = self
            .progression_snapshot_section_bytes
            .get(self.current_byte_offset..value_end_byte_offset)
            .ok_or(WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?
            .try_into()
            .map_err(|_| WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?;
        self.current_byte_offset = value_end_byte_offset;
        Ok(fixed_value_bytes)
    }

    fn read_u8(&mut self) -> Result<u8, WorldSnapshotPersistenceFailure> {
        Ok(self.read_fixed_byte_array::<1>()?[0])
    }

    fn read_boolean(&mut self) -> Result<bool, WorldSnapshotPersistenceFailure> {
        match self.read_u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection),
        }
    }

    fn read_u16(&mut self) -> Result<u16, WorldSnapshotPersistenceFailure> {
        Ok(u16::from_le_bytes(self.read_fixed_byte_array()?))
    }

    fn read_u32(&mut self) -> Result<u32, WorldSnapshotPersistenceFailure> {
        Ok(u32::from_le_bytes(self.read_fixed_byte_array()?))
    }

    fn read_u64(&mut self) -> Result<u64, WorldSnapshotPersistenceFailure> {
        Ok(u64::from_le_bytes(self.read_fixed_byte_array()?))
    }

    fn read_asset_identifier(&mut self) -> Result<AssetId, WorldSnapshotPersistenceFailure> {
        Ok(AssetId(self.read_fixed_byte_array()?))
    }

    fn remaining_byte_count(&self) -> usize {
        self.progression_snapshot_section_bytes
            .len()
            .saturating_sub(self.current_byte_offset)
    }
}
