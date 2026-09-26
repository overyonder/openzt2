use bevy::prelude::{Query, With};
use openzt2_game_data::{
    species::{LifeStage, Sex, Species, SpeciesVariant},
    AssetId,
};

use crate::assets::species::species_asset_types::SpeciesView;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use crate::plugins::habitat::habitat_types::Habitat;
use crate::plugins::simulation_time::deterministic_random_stream::DeterministicRng;
use crate::plugins::simulation_time::deterministic_random_stream::RngDomain;
use crate::plugins::simulation_time::deterministic_random_stream::ZooSeed;
use crate::plugins::world_spawn::persistent_id_types::PersistentId;
use crate::plugins::world_spawn::persistent_id_types::PersistentIdAllocator;
use crate::plugins::world_spawn::world_membership_types::WorldMember;

use super::{
    animal_habitat_placement_validation::{
        validate_animal_habitat_against_authored_aquatic_requirements, AnimalAquaticHabitatQueries,
    },
    animal_spawn_operation_contracts::{AnimalSpawnFailure, SpawnAnimalRequest},
    animal_spawn_resolution_types::ResolvedAnimalSpawnComponents,
    types::{
        AnimalWasteCycle, AuthoredAnimalLifeStageStartTicks, ReproductionTraits,
        SelectedAnimalPresentationModel,
    },
};

pub(super) fn resolve_animal_spawn_request_to_identifiers_and_components(
    request: &SpawnAnimalRequest,
    seed: &ZooSeed,
    species_index: SpeciesView<'_>,
    definitions: WorldDefinitionsView<'_>,
    allocator: &mut PersistentIdAllocator,
    habitats: &Query<&WorldMember, With<Habitat>>,
    aquatic_habitats: &AnimalAquaticHabitatQueries<'_, '_>,
) -> Result<(PersistentId, WorldMember, ResolvedAnimalSpawnComponents), AnimalSpawnFailure> {
    let species = species_index
        .find(request.species)
        .ok_or(AnimalSpawnFailure::MissingSpecies)?;
    if request.sex == Some(Sex::Any) {
        return Err(AnimalSpawnFailure::InvalidSex);
    }
    let world = habitats
        .get(request.habitat)
        .map_err(|_| AnimalSpawnFailure::InvalidHabitat)?;
    validate_animal_habitat_against_authored_aquatic_requirements(
        definitions,
        request.species,
        request.habitat,
        aquatic_habitats,
    )
    .map_err(AnimalSpawnFailure::MarinePlacement)?;
    if !allocator.owns_world(world.root) {
        return Err(AnimalSpawnFailure::PersistentId(
            crate::plugins::world_spawn::persistent_id_types::PersistentIdError::WrongWorld,
        ));
    }
    let preview_id = PersistentId(allocator.next());
    let mut rng = DeterministicRng::from_entity(*seed, preview_id, RngDomain::Reproduction);
    let desired_stage = request
        .variant
        .and_then(|variant| {
            find_species_variant_by_identifier(species_index, request.species, variant)
        })
        .map(|variant| variant.life_stage)
        .unwrap_or(LifeStage::Adult);
    let resolved = resolve_species_and_variant_to_animal_spawn_components(
        species_index,
        species,
        request.variant,
        request.sex,
        desired_stage,
        definitions.timing().ticks_per_day,
        &mut rng,
    )
    .ok_or(if request.variant.is_some() {
        AnimalSpawnFailure::InvalidVariant
    } else {
        AnimalSpawnFailure::InvalidSex
    })?;
    let id = allocator
        .allocate(world.root)
        .map_err(AnimalSpawnFailure::PersistentId)?;
    Ok((id, *world, resolved))
}

pub(super) fn resolve_species_and_variant_to_animal_spawn_components(
    species_index: SpeciesView<'_>,
    species: &Species,
    requested_variant: Option<AssetId>,
    requested_sex: Option<Sex>,
    stage: LifeStage,
    ticks_per_day: u32,
    rng: &mut DeterministicRng,
) -> Option<ResolvedAnimalSpawnComponents> {
    if let Some(id) = requested_variant {
        let variant = find_species_variant_by_identifier(species_index, species.id, id)?;
        if variant.probability == 0 {
            return None;
        }
        let variant_sex = variant.sex;
        let sex = requested_sex.unwrap_or(variant_sex);
        if sex == Sex::Any
            || (variant_sex != Sex::Any && variant_sex != sex)
            || variant.life_stage != stage
        {
            return None;
        }
        return lower_species_and_variant_to_animal_spawn_components(
            species,
            variant,
            sex,
            stage,
            ticks_per_day,
            rng,
        );
    }

    let sex = requested_sex
        .unwrap_or_else(|| choose_available_animal_sex(species_index, species.id, stage, rng));
    if sex == Sex::Any {
        return None;
    }
    let total = species_index
        .variants(species.id)
        .filter(|variant| {
            species_variant_matches_sex_and_life_stage(variant, sex, stage)
                && variant.probability != 0
        })
        .map(|variant| u32::from(variant.probability))
        .sum::<u32>();
    let mut roll = rng.range_u32(total)?;
    for variant in species_index.variants(species.id).filter(|variant| {
        species_variant_matches_sex_and_life_stage(variant, sex, stage) && variant.probability != 0
    }) {
        let weight = u32::from(variant.probability);
        if roll < weight {
            return lower_species_and_variant_to_animal_spawn_components(
                species,
                variant,
                sex,
                stage,
                ticks_per_day,
                rng,
            );
        }
        roll -= weight;
    }
    None
}

pub(super) fn lower_species_and_variant_to_animal_spawn_components(
    species: &Species,
    variant: &SpeciesVariant,
    sex: Sex,
    stage: LifeStage,
    ticks_per_day: u32,
    rng: &mut DeterministicRng,
) -> Option<ResolvedAnimalSpawnComponents> {
    let [juvenile_start_days, young_start_days, adult_start_days, elder_start_days] =
        species.stage_start_days;
    let stage_start_ticks = [
        ticks_from_days(juvenile_start_days, ticks_per_day)?,
        ticks_from_days(young_start_days, ticks_per_day)?,
        ticks_from_days(adult_start_days, ticks_per_day)?,
        ticks_from_days(elder_start_days, ticks_per_day)?,
    ];
    Some(ResolvedAnimalSpawnComponents {
        sex,
        life_stage: stage,
        variant: variant.id,
        definition: species.world_definition,
        life_stage_start_ticks: AuthoredAnimalLifeStageStartTicks { stage_start_ticks },
        reproduction: ReproductionTraits {
            gestation_ticks: ticks_from_days(species.gestation_days, ticks_per_day)?,
            litter: species.litter,
        },
        waste: AnimalWasteCycle {
            definition: species.waste_definition,
            interval_ticks: u32::try_from(ticks_from_days(
                species.waste_interval_days,
                ticks_per_day,
            )?)
            .ok()?,
            units: species.waste_units,
        },
        presentation_model: SelectedAnimalPresentationModel {
            model: AssetId::from_virtual_path(&variant.model),
        },
        scale: select_f32_value_from_authored_range(variant.scale, rng),
        species_flags: species.flags,
        variant_flags: variant.flags,
    })
}

fn ticks_from_days(days: f64, ticks_per_day: u32) -> Option<u64> {
    let ticks = days * f64::from(ticks_per_day);
    (days.is_finite() && days >= 0.0 && ticks <= u64::MAX as f64).then(|| ticks.round() as u64)
}

pub(super) fn select_f32_value_from_authored_range(
    range: [f32; 2],
    rng: &mut DeterministicRng,
) -> f32 {
    range[0] + (range[1] - range[0]) * rng.unit_f32()
}

pub(super) fn find_species_variant_by_identifier<'a>(
    species_index: SpeciesView<'a>,
    species: AssetId,
    id: AssetId,
) -> Option<&'a SpeciesVariant> {
    species_index
        .variants(species)
        .find(|variant| variant.id == id)
}

pub(super) fn species_variant_matches_sex_and_life_stage(
    variant: &SpeciesVariant,
    sex: Sex,
    stage: LifeStage,
) -> bool {
    (variant.sex == sex || variant.sex == Sex::Any) && variant.life_stage == stage
}

pub(super) fn choose_available_animal_sex(
    species_index: SpeciesView<'_>,
    species: AssetId,
    stage: LifeStage,
    rng: &mut DeterministicRng,
) -> Sex {
    let female = species_index
        .variants(species)
        .any(|variant| species_variant_matches_sex_and_life_stage(variant, Sex::Female, stage));
    let male = species_index
        .variants(species)
        .any(|variant| species_variant_matches_sex_and_life_stage(variant, Sex::Male, stage));
    match (female, male) {
        (true, true) => {
            if rng.range_u32(2) == Some(0) {
                Sex::Female
            } else {
                Sex::Male
            }
        }
        (true, false) => Sex::Female,
        (false, true) => Sex::Male,
        (false, false) => Sex::Any,
    }
}
