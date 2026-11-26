use openzt2_game_data::{
    AssetId,
    species::{LifeStage, Sex, Species, SpeciesVariant},
};
use openzt2_zoo_state::placement::placement_surface_site::PlacementSiteQueries;

use openzt2_game_assets::species::species_asset_types::SpeciesView;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use openzt2_world_state::simulation_time::deterministic_random_stream::DeterministicRng;
use openzt2_world_state::simulation_time::deterministic_random_stream::RngDomain;
use openzt2_world_state::simulation_time::deterministic_random_stream::ZooSeed;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentIdAllocator;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;

use super::{
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
    placement_sites: &PlacementSiteQueries<'_, '_>,
    habitat: Option<bevy::prelude::Entity>,
) -> Result<(PersistentId, WorldMember, ResolvedAnimalSpawnComponents), AnimalSpawnFailure> {
    let species = species_index
        .find(request.species)
        .ok_or(AnimalSpawnFailure::MissingSpecies)?;
    if request.sex == Some(Sex::Any) {
        return Err(AnimalSpawnFailure::InvalidSex);
    }
    let world = WorldMember {
        root: request.world,
    };
    let preview_id = PersistentId(allocator.next());
    let mut rng = DeterministicRng::from_entity(*seed, preview_id, RngDomain::Reproduction);
    let (variant, sex) = if let Some(id) = request.variant {
        let variant = find_species_variant_by_identifier(species_index, request.species, id)
            .ok_or(AnimalSpawnFailure::InvalidVariant)?;
        let sex = resolve_exact_variant_sex(variant, request.sex, variant.life_stage, &mut rng)
            .ok_or(AnimalSpawnFailure::InvalidVariant)?;
        (variant, sex)
    } else {
        select_species_spawn_variant(
            species_index,
            species,
            None,
            request.sex,
            LifeStage::Adult,
            &mut rng,
        )
        .ok_or(AnimalSpawnFailure::InvalidSex)?
    };
    placement_sites
        .validate(
            variant.placement_surface_policy,
            request.world,
            habitat,
            request.transform.translation,
        )
        .map_err(|_| AnimalSpawnFailure::InvalidHabitat)?;
    let resolved = lower_species_and_variant_to_animal_spawn_components(
        species,
        variant,
        sex,
        variant.life_stage,
        crate::zoo_day_cadence(definitions).ticks_per_zoo_day(),
        &mut rng,
    )
    .ok_or(if request.variant.is_some() {
        AnimalSpawnFailure::InvalidVariant
    } else {
        AnimalSpawnFailure::InvalidSex
    })?;
    let id = allocator
        .allocate()
        .map_err(AnimalSpawnFailure::PersistentId)?;
    Ok((id, world, resolved))
}

pub(super) fn resolve_species_and_variant_to_animal_spawn_components(
    species_index: SpeciesView<'_>,
    species: &Species,
    requested_variant: Option<AssetId>,
    requested_sex: Option<Sex>,
    stage: LifeStage,
    ticks_per_day: f64,
    rng: &mut DeterministicRng,
) -> Option<ResolvedAnimalSpawnComponents> {
    let (variant, sex) = select_species_spawn_variant(
        species_index,
        species,
        requested_variant,
        requested_sex,
        stage,
        rng,
    )?;
    lower_species_and_variant_to_animal_spawn_components(
        species,
        variant,
        sex,
        stage,
        ticks_per_day,
        rng,
    )
}

pub(super) fn select_species_spawn_variant<'a>(
    species_index: SpeciesView<'a>,
    species: &Species,
    requested_variant: Option<AssetId>,
    requested_sex: Option<Sex>,
    stage: LifeStage,
    rng: &mut DeterministicRng,
) -> Option<(&'a SpeciesVariant, Sex)> {
    if let Some(id) = requested_variant {
        let variant = find_species_variant_by_identifier(species_index, species.id, id)?;
        let sex = resolve_exact_variant_sex(variant, requested_sex, stage, rng)?;
        return Some((variant, sex));
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
            return Some((variant, sex));
        }
        roll -= weight;
    }
    None
}

fn resolve_exact_variant_sex(
    variant: &SpeciesVariant,
    requested_sex: Option<Sex>,
    stage: LifeStage,
    rng: &mut DeterministicRng,
) -> Option<Sex> {
    let sex = requested_sex.unwrap_or_else(|| match variant.sex {
        Sex::Any => {
            if rng.next_u32() & 1 == 0 {
                Sex::Male
            } else {
                Sex::Female
            }
        }
        sex => sex,
    });
    (sex != Sex::Any && species_variant_matches_sex_and_life_stage(variant, sex, stage))
        .then_some(sex)
}

pub fn lower_species_and_variant_to_animal_spawn_components(
    species: &Species,
    variant: &SpeciesVariant,
    sex: Sex,
    stage: LifeStage,
    ticks_per_day: impl Into<f64>,
    rng: &mut DeterministicRng,
) -> Option<ResolvedAnimalSpawnComponents> {
    let ticks_per_day = ticks_per_day.into();
    let [
        juvenile_start_days,
        young_start_days,
        adult_start_days,
        elder_start_days,
    ] = species.stage_start_days;
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

fn ticks_from_days(days: f64, ticks_per_day: f64) -> Option<u64> {
    let ticks = days * ticks_per_day;
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
    let (owner, variant) = species_index.find_variant_with_species(id)?;
    (owner.id == species).then_some(variant)
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
    let female = species_index.variants(species).any(|variant| {
        variant.probability != 0
            && species_variant_matches_sex_and_life_stage(variant, Sex::Female, stage)
    });
    let male = species_index.variants(species).any(|variant| {
        variant.probability != 0
            && species_variant_matches_sex_and_life_stage(variant, Sex::Male, stage)
    });
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
