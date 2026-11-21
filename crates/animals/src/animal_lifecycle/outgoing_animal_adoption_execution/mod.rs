use bevy::prelude::*;
use openzt2_world_state::world_spawn::world_entity_crating::AnimalCrate;
use openzt2_zoo_state::animal_health::types::Dead;
use openzt2_zoo_state::animal_lifecycle::{
    animal_adoption_contracts::PlaceAnimalForAdoption,
    types::{Animal, ReleaseRestricted},
};

pub(super) fn transfer_animals_out_for_adoption(
    mut commands: Commands,
    mut requests: MessageReader<PlaceAnimalForAdoption>,
    animals: Query<(), (With<Animal>, Without<Dead>, Without<ReleaseRestricted>)>,
) {
    for request in requests.read() {
        let animal = request.0;
        if animals.contains(animal) {
            commands.queue(move |world: &mut World| {
                despawn_departing_animal_and_its_crate(world, animal);
            });
        }
    }
}

pub(super) fn despawn_departing_animal_and_its_crate(world: &mut World, animal: Entity) -> bool {
    let crate_entity = world.get::<AnimalCrate>(animal).map(|link| link.0);
    if world.try_despawn(animal).is_err() {
        return false;
    }
    if let Some(crate_entity) = crate_entity {
        let _ = world.try_despawn(crate_entity);
    }
    true
}
