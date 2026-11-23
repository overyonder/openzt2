use bevy::prelude::*;
use openzt2_game_data::species::LifeStage;
use openzt2_zoo_state::animal_behavior::animal_lineage::{AnimalFather, AnimalMother};

use super::{
    animal_age_resolution::resolve_animal_life_stage_from_elapsed_age_ticks,
    animal_birth_and_release_contracts::{AnimalReleased, ReleaseAnimal},
    types::*,
};

#[test]
fn age_crosses_each_authored_stage_at_its_exact_tick() {
    let starts = [0, 10, 25, 40];
    assert_eq!(
        resolve_animal_life_stage_from_elapsed_age_ticks(0, starts),
        LifeStage::Juvenile
    );
    assert_eq!(
        resolve_animal_life_stage_from_elapsed_age_ticks(9, starts),
        LifeStage::Juvenile
    );
    assert_eq!(
        resolve_animal_life_stage_from_elapsed_age_ticks(10, starts),
        LifeStage::Young
    );
    assert_eq!(
        resolve_animal_life_stage_from_elapsed_age_ticks(24, starts),
        LifeStage::Young
    );
    assert_eq!(
        resolve_animal_life_stage_from_elapsed_age_ticks(25, starts),
        LifeStage::Adult
    );
    assert_eq!(
        resolve_animal_life_stage_from_elapsed_age_ticks(39, starts),
        LifeStage::Adult
    );
    assert_eq!(
        resolve_animal_life_stage_from_elapsed_age_ticks(40, starts),
        LifeStage::Elder
    );
    assert_eq!(
        resolve_animal_life_stage_from_elapsed_age_ticks(u64::MAX, starts),
        LifeStage::Elder
    );
}

#[test]
fn lineage_relationship_removes_only_the_despawned_parent() {
    let mut app = App::new();
    let mother = app.world_mut().spawn(Animal).id();
    let father = app.world_mut().spawn(Animal).id();
    let child = app
        .world_mut()
        .spawn((AnimalMother(mother), AnimalFather(father)))
        .id();
    app.world_mut().entity_mut(mother).despawn();
    assert!(app.world().get::<AnimalMother>(child).is_none());
    assert_eq!(app.world().get::<AnimalFather>(child).unwrap().0, father);
}

#[test]
fn release_emits_one_consumed_outcome_and_removes_the_animal() {
    let mut app = App::new();
    app.add_message::<ReleaseAnimal>()
        .add_message::<AnimalReleased>()
        .add_observer(super::outgoing_animal_adoption_execution::settle_departing_animal)
        .add_systems(
            Update,
            super::animal_release_execution::release_eligible_adult_animals_from_the_zoo,
        );
    let root = app.world_mut().spawn_empty().id();
    let animal = app
        .world_mut()
        .spawn((
            Animal,
            AnimalLifeStage(LifeStage::Adult),
            openzt2_world_state::world_spawn::world_membership_types::WorldMember { root },
        ))
        .id();
    app.world_mut().write_message(ReleaseAnimal(animal));
    app.update();
    assert!(app.world().get_entity(animal).is_err());
    let events = app.world().resource::<Messages<AnimalReleased>>();
    assert_eq!(events.len(), 1);
}

#[test]
fn pregnant_and_non_adult_animals_are_not_released() {
    let mut app = App::new();
    app.add_message::<ReleaseAnimal>()
        .add_message::<AnimalReleased>()
        .add_observer(super::outgoing_animal_adoption_execution::settle_departing_animal)
        .add_systems(
            Update,
            super::animal_release_execution::release_eligible_adult_animals_from_the_zoo,
        );
    let juvenile = app
        .world_mut()
        .spawn((Animal, AnimalLifeStage(LifeStage::Juvenile)))
        .id();
    let pregnant = app
        .world_mut()
        .spawn((
            Animal,
            AnimalLifeStage(LifeStage::Adult),
            Pregnancy {
                father: Some(juvenile),
            },
        ))
        .id();
    app.world_mut().write_message(ReleaseAnimal(juvenile));
    app.world_mut().write_message(ReleaseAnimal(pregnant));
    app.update();
    assert!(app.world().get_entity(juvenile).is_ok());
    assert!(app.world().get_entity(pregnant).is_ok());
    assert!(
        app.world()
            .resource::<Messages<AnimalReleased>>()
            .is_empty()
    );
}

#[test]
fn outgoing_adoption_removes_animal_and_crate_without_wild_release_credit() {
    use super::animal_adoption_contracts::PlaceAnimalForAdoption;
    use bevy::ecs::relationship::Relationship;
    use openzt2_world_state::world_spawn::world_entity_crating::{
        AnimalCrate, WorldEntityIsCrated,
    };

    let mut app = App::new();
    app.add_message::<PlaceAnimalForAdoption>()
        .add_message::<AnimalReleased>()
        .add_observer(super::outgoing_animal_adoption_execution::settle_departing_animal)
        .add_systems(
            Update,
            super::outgoing_animal_adoption_execution::transfer_animals_out_for_adoption,
        );
    let animal = app.world_mut().spawn((Animal, WorldEntityIsCrated)).id();
    let presentation = app.world_mut().spawn(ChildOf(animal)).id();
    let crate_entity = app.world_mut().spawn_empty().id();
    app.world_mut()
        .entity_mut(animal)
        .insert(AnimalCrate(crate_entity));
    assert_eq!(
        app.world()
            .get::<AnimalCrate>(animal)
            .map(Relationship::get),
        Some(crate_entity)
    );
    let restricted = app.world_mut().spawn((Animal, ReleaseRestricted)).id();
    let child = app
        .world_mut()
        .spawn((AnimalMother(animal), AnimalFather(restricted)))
        .id();
    app.world_mut()
        .write_message(PlaceAnimalForAdoption(animal));
    app.world_mut()
        .write_message(PlaceAnimalForAdoption(animal));
    app.world_mut()
        .write_message(PlaceAnimalForAdoption(restricted));
    app.update();
    assert!(app.world().get_entity(animal).is_err());
    assert!(app.world().get_entity(presentation).is_err());
    assert!(app.world().get_entity(crate_entity).is_err());
    assert!(app.world().get_entity(restricted).is_ok());
    assert!(app.world().get::<AnimalMother>(child).is_none());
    assert_eq!(
        app.world().get::<AnimalFather>(child).unwrap().0,
        restricted
    );
    assert!(
        app.world()
            .resource::<Messages<AnimalReleased>>()
            .is_empty()
    );
}
