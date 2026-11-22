use bevy::{
    ecs::{entity::EntityLocation, relationship::RelationshipTarget, system::SystemParam},
    prelude::*,
};

use crate::animal_lifecycle::types::{Animal, Parents};

#[derive(Component)]
#[relationship(relationship_target = MaternalChildren)]
pub struct AnimalMother(pub Entity);

#[derive(Component)]
#[relationship_target(relationship = AnimalMother)]
pub struct MaternalChildren(Vec<Entity>);

#[derive(Component)]
#[relationship(relationship_target = PaternalChildren)]
pub struct AnimalFather(pub Entity);

#[derive(Component)]
#[relationship_target(relationship = AnimalFather)]
pub struct PaternalChildren(Vec<Entity>);

#[derive(Component)]
#[relationship(relationship_target = FatheredPregnancies)]
pub struct PregnancyFather(pub Entity);

#[derive(Component)]
#[relationship_target(relationship = PregnancyFather)]
pub struct FatheredPregnancies(Vec<Entity>);

#[derive(Component)]
#[relationship(relationship_target = AnimalMates)]
pub struct AnimalMate(pub Entity);

#[derive(Component)]
#[relationship_target(relationship = AnimalMate)]
pub struct AnimalMates(Vec<Entity>);

pub fn insert_animal_parentage(animal: &mut EntityCommands, parents: Parents) {
    if let Some(mother) = parents.mother {
        animal.insert(AnimalMother(mother));
    }
    if let Some(father) = parents.father {
        animal.insert(AnimalFather(father));
    }
}

#[derive(SystemParam)]
pub struct AnimalLineage<'w, 's> {
    parents: Query<'w, 's, (Option<&'static AnimalMother>, Option<&'static AnimalFather>)>,
    mothers: Query<'w, 's, &'static MaternalChildren>,
    fathers: Query<'w, 's, &'static PaternalChildren>,
    live: Query<'w, 's, EntityLocation>,
}

impl AnimalLineage<'_, '_> {
    pub fn parents(&self, animal: Entity) -> Parents {
        self.parents
            .get(animal)
            .map_or(Parents::NONE, |(mother, father)| Parents {
                mother: mother.map(|mother| mother.0),
                father: father.map(|father| father.0),
            })
    }

    pub fn children(&self, animal: Entity) -> Vec<Entity> {
        let maternal = self
            .mothers
            .get(animal)
            .into_iter()
            .flat_map(RelationshipTarget::iter);
        let paternal = self
            .fathers
            .get(animal)
            .into_iter()
            .flat_map(RelationshipTarget::iter);
        let mut children: Vec<_> = maternal
            .chain(paternal)
            .filter_map(|child| {
                self.live.get(child).ok().map(|location| {
                    (
                        location.table_id.as_u32(),
                        location.table_row.index(),
                        child,
                    )
                })
            })
            .collect();
        children.sort_unstable();
        children.dedup();
        children.into_iter().map(|(_, _, child)| child).collect()
    }
}

pub fn end_live_animal_relationships(removed: On<Remove<Animal>>, mut commands: Commands) {
    commands.entity(removed.entity).try_remove::<(
        MaternalChildren,
        PaternalChildren,
        FatheredPregnancies,
        AnimalMates,
    )>();
}
