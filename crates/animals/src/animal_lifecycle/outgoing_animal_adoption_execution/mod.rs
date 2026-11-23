use bevy::prelude::*;
use openzt2_world_state::world_spawn::world_entity_crating::AnimalCrate;
use openzt2_zoo_state::animal_health::types::Dead;
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
    animals: Query<(&AnimalDeparture, Option<&AnimalCrate>)>,
    mut commands: Commands,
) {
    let Ok((departure, animal_crate)) = animals.get(departure.entity) else {
        return;
    };
    if let Some(animal_crate) = animal_crate {
        commands.entity(animal_crate.0).try_despawn();
    }
    if let AnimalDeparture::Release { root } = *departure {
        commands.write_message(AnimalReleased { root });
    }
}
