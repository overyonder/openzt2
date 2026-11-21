use bevy::prelude::*;
use openzt2_game_data::species::LifeStage;

use openzt2_zoo_state::animal_health::types::Dead;

use super::{
    animal_birth_and_release_contracts::{AnimalReleased, ReleaseAnimal},
    outgoing_animal_adoption_execution::despawn_departing_animal_and_its_crate,
    types::{Animal, AnimalLifeStage, Pregnancy, ReleaseRestricted},
};

pub(super) fn release_eligible_adult_animals_from_the_zoo(
    mut commands: Commands,
    mut requests: MessageReader<ReleaseAnimal>,
    animals: Query<
        (&AnimalLifeStage, Has<Pregnancy>),
        (With<Animal>, Without<Dead>, Without<ReleaseRestricted>),
    >,
) {
    for request in requests.read() {
        let animal = request.0;
        let Ok((life_stage, pregnant)) = animals.get(animal) else {
            continue;
        };
        if pregnant || life_stage.0 != LifeStage::Adult {
            continue;
        }
        commands.queue(move |world: &mut World| {
            if despawn_departing_animal_and_its_crate(world, animal) {
                world.write_message(AnimalReleased);
            }
        });
    }
}
