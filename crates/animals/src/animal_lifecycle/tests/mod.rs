use bevy::prelude::*;
use openzt2_game_data::species::LifeStage;

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
fn lineage_cleanup_clears_only_the_removed_parent() {
    let mut app = App::new();
    app.add_systems(
        Update,
        super::animal_lineage_reference_cleanup::
            clear_lineage_references_to_deleted_animal_entities,
    );
    let mother = app.world_mut().spawn(Animal).id();
    let father = app.world_mut().spawn(Animal).id();
    let child = app
        .world_mut()
        .spawn(Parents {
            mother: Some(mother),
            father: Some(father),
        })
        .id();
    app.update();
    app.world_mut().entity_mut(mother).despawn();
    app.update();
    assert_eq!(
        *app.world().get::<Parents>(child).unwrap(),
        Parents {
            mother: None,
            father: Some(father),
        }
    );
}

#[test]
fn release_emits_one_consumed_outcome_and_removes_the_animal() {
    let mut app = App::new();
    app.add_message::<ReleaseAnimal>()
        .add_message::<AnimalReleased>()
        .add_systems(
            Update,
            super::animal_release_execution::release_eligible_adult_animals_from_the_zoo,
        );
    let animal = app
        .world_mut()
        .spawn((Animal, AnimalLifeStage(LifeStage::Adult)))
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
    assert!(app
        .world()
        .resource::<Messages<AnimalReleased>>()
        .is_empty());
}

#[test]
fn outgoing_adoption_removes_animal_and_crate_without_wild_release_credit() {
    use super::animal_adoption_contracts::PlaceAnimalForAdoption;
    use openzt2_world_state::world_spawn::world_entity_crating::{
        AnimalCrate, ContainedCratedAnimal, WorldEntityIsCrated,
    };

    let mut app = App::new();
    app.add_message::<PlaceAnimalForAdoption>()
        .add_message::<AnimalReleased>()
        .add_systems(Update, (
            super::outgoing_animal_adoption_execution::transfer_animals_out_for_adoption,
            super::animal_lineage_reference_cleanup::clear_lineage_references_to_deleted_animal_entities,
        ).chain());
    let animal = app.world_mut().spawn((Animal, WorldEntityIsCrated)).id();
    let presentation = app.world_mut().spawn(ChildOf(animal)).id();
    let crate_entity = app.world_mut().spawn(ContainedCratedAnimal(animal)).id();
    app.world_mut()
        .entity_mut(animal)
        .insert(AnimalCrate(crate_entity));
    let restricted = app.world_mut().spawn((Animal, ReleaseRestricted)).id();
    let child = app
        .world_mut()
        .spawn(Parents {
            mother: Some(animal),
            father: Some(restricted),
        })
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
    assert_eq!(
        *app.world().get::<Parents>(child).unwrap(),
        Parents {
            mother: None,
            father: Some(restricted)
        }
    );
    assert!(app
        .world()
        .resource::<Messages<AnimalReleased>>()
        .is_empty());
}
