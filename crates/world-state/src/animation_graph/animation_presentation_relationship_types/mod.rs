use bevy::prelude::*;

/// Links a renderable animation playback controller to the gameplay entity
/// whose state it presents.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
#[relationship(relationship_target = AnimationPresentations, allow_self_referential)]
pub struct AnimationPresentationOwner {
    pub gameplay_entity: Entity,
}

#[derive(Component, Debug)]
#[relationship_target(relationship = AnimationPresentationOwner)]
pub struct AnimationPresentations(Vec<Entity>);

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct AnimationPresentationBinder(pub openzt2_game_data::AssetId);

impl AnimationPresentations {
    pub fn for_subject(
        subject: Entity,
        presentations: Option<&Self>,
    ) -> impl Iterator<Item = Entity> + '_ {
        std::iter::once(subject).chain(
            presentations
                .into_iter()
                .flat_map(|presentations| presentations.0.iter().copied())
                .filter(move |controller| *controller != subject),
        )
    }
}
