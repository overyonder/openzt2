//! Guest tasks address the gameplay entity, not an intermediate prefab node.

use bevy::prelude::*;

use crate::plugins::animation_graph::animation_presentation_relationship_types::AnimationPresentationOwner;

use super::guest_simulation_types::Guest;

/// Runs in Presentation, after animation attachment in Intent has inserted its
/// controller and initial direct-parent owner. Reattachment also changes that
/// component, so a hot-reloaded model follows the same hierarchy resolution.
pub(super) fn project_guest_animation_owners(
    mut controllers: Query<
        (Entity, &mut AnimationPresentationOwner),
        Changed<AnimationPresentationOwner>,
    >,
    parents: Query<&ChildOf>,
    guests: Query<(), With<Guest>>,
) {
    for (controller, mut owner) in &mut controllers {
        if guests.contains(owner.gameplay_entity) {
            continue;
        }
        if let Some(guest) = parents
            .iter_ancestors::<ChildOf>(controller)
            .find(|ancestor| guests.contains(*ancestor))
        {
            owner.gameplay_entity = guest;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_guest_controller_and_reattachment_keep_gameplay_owner() {
        let mut app = App::new();
        app.add_systems(Update, project_guest_animation_owners);
        let guest = app.world_mut().spawn(Guest).id();
        let prefab_root = app.world_mut().spawn(ChildOf(guest)).id();
        let controller = app
            .world_mut()
            .spawn((
                ChildOf(prefab_root),
                AnimationPresentationOwner {
                    gameplay_entity: prefab_root,
                },
            ))
            .id();
        app.update();
        assert_eq!(
            app.world()
                .get::<AnimationPresentationOwner>(controller)
                .unwrap()
                .gameplay_entity,
            guest
        );
        app.world_mut()
            .entity_mut(controller)
            .insert(AnimationPresentationOwner {
                gameplay_entity: prefab_root,
            });
        app.update();
        assert_eq!(
            app.world()
                .get::<AnimationPresentationOwner>(controller)
                .unwrap()
                .gameplay_entity,
            guest
        );
    }
}
