use bevy::prelude::*;
use openzt2_world_state::world_spawn::world_entity_crating::{
    AnimalCrate, ContainedCratedAnimal, RemoveWorldEntityFromCrate, WorldEntityIsCrated,
};

pub(super) fn remove_requested_world_entities_from_crates(
    mut commands: Commands,
    mut requests: MessageReader<RemoveWorldEntityFromCrate>,
    mut crated_world_entities: Query<
        (Option<&AnimalCrate>, Option<&mut Transform>),
        (With<WorldEntityIsCrated>, Without<ContainedCratedAnimal>),
    >,
    crates: Query<(&ContainedCratedAnimal, &Transform), Without<AnimalCrate>>,
) {
    let mut released = std::collections::BTreeSet::new();
    for request in requests.read() {
        let animal = crates
            .get(request.0)
            .map_or(request.0, |(contained, _)| contained.0);
        if !released.insert(animal) {
            continue;
        }
        let Ok((link, transform)) = crated_world_entities.get_mut(animal) else {
            continue;
        };
        if let Some(link) = link {
            if let Ok((contained, crate_transform)) = crates.get(link.0) {
                if contained.0 != animal {
                    warn!(?animal, crate_entity = ?link.0, "animal crate link is not reciprocal");
                    continue;
                }
                if let Some(mut transform) = transform {
                    transform.translation = crate_transform.translation;
                    transform.rotation = crate_transform.rotation;
                }
                commands.entity(link.0).despawn();
            }
            commands
                .entity(animal)
                .remove::<AnimalCrate>()
                .insert(Visibility::Inherited);
        }
        commands.entity(animal).remove::<WorldEntityIsCrated>();
    }
}

/// Native crate and animal roots remain separate entities; relocation history
/// moves the crate while the contained animal retains its own authored scale.
pub(super) fn synchronize_contained_animal_transforms(
    crates: Query<(Entity, &ContainedCratedAnimal, &Transform), Without<AnimalCrate>>,
    mut animals: Query<
        (&AnimalCrate, &mut Transform),
        (With<WorldEntityIsCrated>, Without<ContainedCratedAnimal>),
    >,
) {
    for (crate_entity, contained, crate_transform) in &crates {
        let Ok((link, mut transform)) = animals.get_mut(contained.0) else {
            continue;
        };
        if link.0 == crate_entity {
            if transform.translation != crate_transform.translation {
                transform.translation = crate_transform.translation;
            }
            if transform.rotation != crate_transform.rotation {
                transform.rotation = crate_transform.rotation;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moved_native_crate_releases_animal_at_its_position_without_changing_scale() {
        let mut app = App::new();
        app.add_message::<RemoveWorldEntityFromCrate>().add_systems(
            Update,
            (
                synchronize_contained_animal_transforms,
                remove_requested_world_entities_from_crates,
            )
                .chain(),
        );
        let animal = app
            .world_mut()
            .spawn((
                Transform::from_scale(Vec3::splat(0.8)),
                Visibility::Hidden,
                WorldEntityIsCrated,
            ))
            .id();
        let crate_entity = app
            .world_mut()
            .spawn((
                Transform::from_xyz(10.0, 2.0, 5.0),
                ContainedCratedAnimal(animal),
            ))
            .id();
        app.world_mut()
            .entity_mut(animal)
            .insert(AnimalCrate(crate_entity));
        app.update();
        assert_eq!(
            app.world().get::<Transform>(animal).unwrap().translation,
            Vec3::new(10.0, 2.0, 5.0)
        );
        let moved = Transform::from_xyz(20.0, 3.0, 8.0).with_rotation(Quat::from_rotation_y(0.7));
        app.world_mut().entity_mut(crate_entity).insert(moved);
        app.world_mut()
            .write_message(RemoveWorldEntityFromCrate(animal));
        app.update();
        let transform = app.world().get::<Transform>(animal).unwrap();
        assert_eq!(transform.translation, moved.translation);
        assert_eq!(transform.rotation, moved.rotation);
        assert_eq!(transform.scale, Vec3::splat(0.8));
        assert_eq!(
            app.world().get::<Visibility>(animal),
            Some(&Visibility::Inherited)
        );
        assert!(app.world().get::<AnimalCrate>(animal).is_none());
        assert!(app.world().get::<WorldEntityIsCrated>(animal).is_none());
        assert!(app.world().get_entity(crate_entity).is_err());
    }
}
