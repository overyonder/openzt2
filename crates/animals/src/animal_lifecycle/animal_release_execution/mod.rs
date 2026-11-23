use bevy::prelude::*;
use openzt2_game_data::species::LifeStage;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;

use openzt2_zoo_state::animal_health::types::Dead;

use super::{
    animal_birth_and_release_contracts::ReleaseAnimal,
    types::{Animal, AnimalLifeStage, Pregnancy, ReleaseRestricted},
};

pub(super) fn release_eligible_adult_animals_from_the_zoo(
    mut commands: Commands,
    mut requests: MessageReader<ReleaseAnimal>,
    animals: Query<
        (&AnimalLifeStage, Has<Pregnancy>, &WorldMember),
        (With<Animal>, Without<Dead>, Without<ReleaseRestricted>),
    >,
) {
    for request in requests.read() {
        let animal = request.0;
        let Ok((life_stage, pregnant, member)) = animals.get(animal) else {
            continue;
        };
        if pregnant || life_stage.0 != LifeStage::Adult {
            continue;
        }
        commands
            .entity(animal)
            .try_insert(
                super::outgoing_animal_adoption_execution::AnimalDeparture::Release {
                    root: member.root,
                },
            )
            .try_despawn();
    }
}
