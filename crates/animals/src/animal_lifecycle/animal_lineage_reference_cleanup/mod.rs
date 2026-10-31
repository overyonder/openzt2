use bevy::prelude::*;

use super::types::{Animal, Parents, Pregnancy};

pub(super) fn clear_lineage_references_to_deleted_animal_entities(
    mut removed_animals: RemovedComponents<Animal>,
    mut children: Query<&mut Parents>,
    mut pregnancies: Query<&mut Pregnancy>,
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
        for mut pregnancy in &mut pregnancies {
            if pregnancy.father == Some(removed_animal_entity) {
                pregnancy.father = None;
            }
        }
    }
}
