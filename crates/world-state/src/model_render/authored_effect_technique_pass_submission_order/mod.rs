use bevy::ecs::query::QueryItem;
use bevy::prelude::*;
use bevy::render::extract_instances::ExtractInstance;

/// Bevy owns draw preparation and submission. This component supplies only the
/// authored ordering constraint between passes of the same invocation.
#[derive(Component, Clone, Copy)]
pub struct AuthoredEffectTechniquePassSubmissionOrder {
    technique_invocation: Entity,
    authored_pass_index: u16,
    opaque_base_pass: bool,
    source_hierarchy_parent: Entity,
    source_hierarchy_index: usize,
}

impl ExtractInstance for AuthoredEffectTechniquePassSubmissionOrder {
    type QueryData = &'static Self;
    type QueryFilter = ();

    fn extract(item: QueryItem<'_, '_, Self::QueryData>) -> Option<Self> {
        Some(*item)
    }
}

impl AuthoredEffectTechniquePassSubmissionOrder {
    pub fn new(technique_invocation: Entity, authored_pass_index: usize) -> Option<Self> {
        Some(Self {
            technique_invocation,
            authored_pass_index: u16::try_from(authored_pass_index).ok()?,
            opaque_base_pass: false,
            source_hierarchy_parent: technique_invocation,
            source_hierarchy_index: 0,
        })
    }

    pub const fn is_first_authored_pass(self) -> bool {
        self.authored_pass_index == 0
    }

    pub fn with_source_hierarchy_order(mut self, parent: Entity, index: usize) -> Self {
        self.source_hierarchy_parent = parent;
        self.source_hierarchy_index = index;
        self
    }

    pub fn belongs_to_technique_invocation(self, technique_invocation: Entity) -> bool {
        self.technique_invocation == technique_invocation
    }
}

impl AuthoredEffectTechniquePassSubmissionOrder {
    pub fn update_opaque_base_pass(&mut self, opaque: bool) {
        self.opaque_base_pass = opaque;
    }

    pub fn record_invocation_distance(
        &self,
        distances: &mut bevy::ecs::entity::EntityHashMap<(bool, f32)>,
        distance: f32,
    ) {
        distances
            .entry(self.technique_invocation)
            .and_modify(|value| value.0 |= self.opaque_base_pass)
            .or_insert((self.opaque_base_pass, distance));
    }

    pub fn invocation_distance(
        &self,
        distances: &bevy::ecs::entity::EntityHashMap<(bool, f32)>,
    ) -> Option<(bool, f32)> {
        distances.get(&self.technique_invocation).copied()
    }

    pub fn compare_hierarchy_order(&self, other: &Self) -> std::cmp::Ordering {
        self.source_hierarchy_parent
            .to_bits()
            .cmp(&other.source_hierarchy_parent.to_bits())
            .then(
                self.source_hierarchy_index
                    .cmp(&other.source_hierarchy_index),
            )
            .then(
                self.technique_invocation
                    .to_bits()
                    .cmp(&other.technique_invocation.to_bits()),
            )
            .then(self.authored_pass_index.cmp(&other.authored_pass_index))
    }
}
