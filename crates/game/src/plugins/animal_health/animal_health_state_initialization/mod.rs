use bevy::prelude::*;

use crate::plugins::animal_lifecycle::types::Animal;

use super::types::Vitality;

pub(super) fn initialize_new_animals_with_full_vitality(
    mut commands: Commands,
    animals: Query<Entity, (With<Animal>, Without<Vitality>)>,
) {
    for animal in &animals {
        commands.entity(animal).insert(Vitality(1.0));
    }
}
