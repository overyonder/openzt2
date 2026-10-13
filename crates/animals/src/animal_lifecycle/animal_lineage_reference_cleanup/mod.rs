use bevy::prelude::*;

use super::types::{Animal, Parents, Pregnancy};

pub(super) fn clear_lineage_references_to_deleted_animal_entities(
    mut commands: Commands,
    mut removed_animals: RemovedComponents<Animal>,
    mut children: Query<&mut Parents>,
    pregnancies: Query<(Entity, &Pregnancy)>,
) {
    for removed_animal_entity in removed_animals.read() {
        for mut parents in &mut children {
            if parents.mother == Some(removed_animal_entity) {
                parents.mother = None;
            }
            if parents.father == Some(removed_animal_entity) {
                parents.father = None;
            }
        }
        for (mother_entity, pregnancy) in &pregnancies {
            if pregnancy.father == removed_animal_entity {
                commands.entity(mother_entity).remove::<Pregnancy>();
            }
        }
    }
}
