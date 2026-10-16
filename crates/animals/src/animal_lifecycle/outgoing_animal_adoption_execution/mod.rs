use bevy::prelude::*;
use openzt2_world_state::world_spawn::world_entity_crating::{AnimalCrate, ContainedCratedAnimal};
use openzt2_zoo_state::animal_health::types::Dead;
use openzt2_zoo_state::animal_lifecycle::{
    animal_adoption_contracts::PlaceAnimalForAdoption,
    types::{Animal, ReleaseRestricted},
};

pub(super) fn transfer_animals_out_for_adoption(
    mut commands: Commands,
    mut requests: MessageReader<PlaceAnimalForAdoption>,
    animals: Query<Option<&AnimalCrate>, (With<Animal>, Without<Dead>, Without<ReleaseRestricted>)>,
    crates: Query<&ContainedCratedAnimal>,
) {
    for request in requests.read() {
        let Ok(crate_link) = animals.get(request.0) else {
            continue;
        };
        if let Some(crate_link) = crate_link {
            if crates
                .get(crate_link.0)
                .is_ok_and(|contained| contained.0 == request.0)
            {
                commands.entity(crate_link.0).try_despawn();
            }
        }
        commands.entity(request.0).try_despawn();
    }
}
