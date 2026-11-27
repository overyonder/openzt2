use super::prefab_world_instance_spawning::award_crate_delivery::AwardCrateObjectOccupant;
use super::world_prefab_record_hydration::ApplyStartingAnimalHiddenVisibility;
use bevy::prelude::*;
use openzt2_world_state::world_spawn::world_entity_crating::{
    AnimalCrate, ContainedCratedAnimal, CratedWorldEntity, RemoveWorldEntityFromCrate,
    ScriptedCrating, UncratedWorldEntity,
};

pub(super) fn remove_requested_world_entities_from_crates(
    mut commands: Commands,
    mut requests: MessageReader<RemoveWorldEntityFromCrate>,
    mut animals: Query<
        (
            Option<&AnimalCrate>,
            Option<&mut Transform>,
            Has<AwardCrateObjectOccupant>,
        ),
        (CratedWorldEntity, Without<ContainedCratedAnimal>),
    >,
    crates: Query<(&ContainedCratedAnimal, &Transform), UncratedWorldEntity>,
) {
    let mut released = bevy::ecs::entity::EntityHashSet::new();
    for request in requests.read() {
        let animal = crates
            .get(request.0)
            .map_or(request.0, |(contained, _)| contained.occupant());
        if !released.insert(animal) {
            continue;
        }
        let Ok((crate_link, transform, award_object)) = animals.get_mut(animal) else {
            continue;
        };
        if let Some(crate_link) = crate_link {
            if let Ok((_, crate_pose)) = crates.get(crate_link.0) {
                if let Some(mut transform) = transform {
                    let scale = transform.scale;
                    transform.set_if_neq(Transform {
                        translation: crate_pose.translation,
                        rotation: crate_pose.rotation,
                        scale,
                    });
                }
                commands.entity(crate_link.0).try_despawn();
            }
            commands
                .entity(animal)
                .remove::<(AnimalCrate, ApplyStartingAnimalHiddenVisibility)>();
            if award_object {
                commands.entity(animal).remove::<(
                    AwardCrateObjectOccupant,
                    avian3d::prelude::RigidBodyDisabled,
                )>();
            }
        } else {
            commands.entity(animal).remove::<ScriptedCrating>();
        }
    }
}

pub(super) fn synchronize_contained_animal_transforms(
    crates: Query<
        (&ContainedCratedAnimal, &Transform),
        (
            Without<AnimalCrate>,
            Or<(Changed<Transform>, Changed<ContainedCratedAnimal>)>,
        ),
    >,
    mut animals: Query<&mut Transform, With<AnimalCrate>>,
) {
    for (contained, crate_pose) in &crates {
        if let Ok(mut transform) = animals.get_mut(contained.occupant()) {
            let scale = transform.scale;
            transform.set_if_neq(Transform {
                translation: crate_pose.translation,
                rotation: crate_pose.rotation,
                scale,
            });
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
            .spawn(Transform::from_scale(Vec3::splat(0.8)))
            .id();
        let crate_entity = app
            .world_mut()
            .spawn(Transform::from_xyz(10.0, 2.0, 5.0))
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
        assert!(app.world().get_entity(crate_entity).is_err());
    }
}
