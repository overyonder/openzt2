use bevy::prelude::*;
use openzt2_game_data::species::LifeStage;

use crate::plugins::animal_health::types::Dead;

use super::{
    animal_birth_and_release_contracts::{AnimalReleased, ReleaseAnimal},
    types::{Animal, AnimalLifeStage, Pregnancy, ReleaseRestricted},
};

pub(super) fn release_eligible_adult_animals_from_the_zoo(
    mut commands: Commands,
    mut release_requests: MessageReader<ReleaseAnimal>,
    animals: Query<
        (&AnimalLifeStage, Option<&Pregnancy>),
        (With<Animal>, Without<Dead>, Without<ReleaseRestricted>),
    >,
    mut released_animals: MessageWriter<AnimalReleased>,
) {
    for release_request in release_requests.read() {
        let Ok((life_stage, pregnancy)) = animals.get(release_request.0) else {
            continue;
        };
        if pregnancy.is_some() || life_stage.0 != LifeStage::Adult {
            continue;
        }
        released_animals.write(AnimalReleased);
        commands.entity(release_request.0).despawn();
    }
}
