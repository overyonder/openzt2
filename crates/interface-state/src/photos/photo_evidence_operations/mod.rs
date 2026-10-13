use super::photo_capture_types::PhotoSubjects;
use bevy::prelude::Entity;

impl PhotoSubjects {
    pub fn insert(&mut self, entity: Entity) -> bool {
        let Err(index) = self
            .0
            .binary_search_by_key(&entity.to_bits(), |candidate| candidate.to_bits())
        else {
            return false;
        };
        self.0.insert(index, entity);
        true
    }
}
