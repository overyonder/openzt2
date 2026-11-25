use super::*;

#[test]
fn retired_crate_links_release_visibility_only_after_the_last_crating_reason() {
    let mut world = World::new();
    let original = world.spawn_empty().id();
    let replacement = world.spawn_empty().id();
    let animal = world
        .spawn((Visibility::Inherited, AnimalCrate(original)))
        .id();
    assert_eq!(world.get::<Visibility>(animal), Some(&Visibility::Hidden));

    world.entity_mut(animal).insert(AnimalCrate(replacement));
    world.entity_mut(original).despawn();
    world.flush();
    assert_eq!(
        world.get::<AnimalCrate>(animal).map(|link| link.0),
        Some(replacement)
    );
    assert_eq!(world.get::<Visibility>(animal), Some(&Visibility::Hidden));

    world.entity_mut(animal).insert(ScriptedCrating);
    world.entity_mut(replacement).despawn();
    world.flush();
    assert!(world.get::<AnimalCrate>(animal).is_none());
    assert_eq!(world.get::<Visibility>(animal), Some(&Visibility::Hidden));

    world.entity_mut(animal).remove::<ScriptedCrating>();
    world.flush();
    assert_eq!(
        world.get::<Visibility>(animal),
        Some(&Visibility::Inherited)
    );
}
