use bevy::prelude::*;
use openzt2_game_assets::species::species_asset_types::{SpeciesAsset, SpeciesAssets};
use openzt2_game_data::species::SpeciesFlags;
use openzt2_progression::progression::{AwardHistoryOutcome, RecordAwardOutcome};
use openzt2_world_state::world_spawn::world_entity_crating::AnimalCrate;
use openzt2_zoo_state::animal_health::types::Dead;
use openzt2_zoo_state::animal_lifecycle::types::{AnimalVariant, SpeciesHandle};
use openzt2_zoo_state::animal_lifecycle::{
    animal_adoption_contracts::PlaceAnimalForAdoption,
    animal_birth_and_release_contracts::AnimalReleased,
    types::{Animal, ReleaseRestricted},
};

#[derive(Component)]
pub(super) enum AnimalDeparture {
    Release { root: Entity },
    Adoption,
}

pub(super) fn transfer_animals_out_for_adoption(
    mut commands: Commands,
    mut requests: MessageReader<PlaceAnimalForAdoption>,
    animals: Query<(), (With<Animal>, Without<Dead>, Without<ReleaseRestricted>)>,
) {
    for request in requests.read() {
        let animal = request.0;
        if animals.contains(animal) {
            commands
                .entity(animal)
                .try_insert(AnimalDeparture::Adoption)
                .try_despawn();
        }
    }
}

pub(super) fn settle_departing_animal(
    departure: On<Despawn<AnimalDeparture>>,
    animals: Query<(
        &AnimalDeparture,
        Option<&AnimalCrate>,
        Option<&SpeciesHandle>,
        Option<&AnimalVariant>,
    )>,
    assets: Option<Res<Assets<SpeciesAsset>>>,
    catalogue: Option<Res<SpeciesAssets>>,
    mut commands: Commands,
) {
    let Ok((departure, animal_crate, species, variant)) = animals.get(departure.entity) else {
        return;
    };
    if let Some(animal_crate) = animal_crate {
        commands.entity(animal_crate.0).try_despawn();
    }
    if let AnimalDeparture::Release { root } = *departure {
        let view = catalogue
            .as_deref()
            .zip(assets.as_deref())
            .and_then(|(catalogue, assets)| catalogue.get(assets));
        let definition = view.and_then(|view| view.find(species?.species));
        let authored_name = variant
            .and_then(|variant| view?.find_variant_binding(variant.0))
            .and_then(|variant| variant.authored_species_name_override.as_deref())
            .or_else(|| definition.map(|species| species.authored_species_name.as_str()))
            .unwrap_or_default()
            .to_owned();
        let extinct =
            definition.is_some_and(|species| species.flags.contains(SpeciesFlags::EXTINCT));
        let id = species.map_or(Default::default(), |species| species.species);
        commands.write_message(AnimalReleased { root });
        if definition.is_some() {
            commands.write_message(RecordAwardOutcome {
                root,
                outcome: AwardHistoryOutcome::Released {
                    species: id,
                    name: authored_name,
                    extinct,
                },
            });
        }
    }
}
