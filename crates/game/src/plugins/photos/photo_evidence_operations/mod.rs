//! Deduplicated insertion, and default construction for photo evidence data.

use bevy::prelude::{Entity, IVec3};

use super::photo_capture_types::{
    PendingPhotoEvidence, PhotoEvidence, PhotoSemanticEvidence,
    PhotoSubjectContainedEntitiesEvidence, PhotoSubjectTaskEvidence, PhotoSubjects,
};

impl PhotoSubjects {
    pub(crate) fn insert(&mut self, entity: Entity) -> bool {
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

impl PendingPhotoEvidence {
    pub(crate) fn insert(&mut self, evidence: PhotoEvidence) -> bool {
        let Err(index) = self
            .0
            .binary_search_by_key(&evidence.entity.to_bits(), |current| {
                current.entity.to_bits()
            })
        else {
            return false;
        };
        self.0.insert(index, evidence);
        true
    }
}

impl Default for PhotoSemanticEvidence {
    fn default() -> Self {
        Self {
            entity: Entity::PLACEHOLDER,
            position_mm: IVec3::ZERO,
            animal_variant: None,
            behavior: None,
            animation: None,
            object_use: None,
            habitat: None,
            aquatic: false,
            show_trick: None,
            fossil: None,
            fossil_bone_level: None,
            health: None,
            life_stage: None,
            mother: None,
            father: None,
            needs_q16: [None; 10],
            super_animal: false,
            named: false,
            contained: false,
            task: PhotoSubjectTaskEvidence::UnresolvedAtCapture,
            contained_entities: PhotoSubjectContainedEntitiesEvidence::UnsupportedAtCapture,
        }
    }
}
