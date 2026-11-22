use bevy::{
    ecs::{component::Mutable, system::EntityCommands},
    prelude::*,
};

pub(in crate::persistence) fn restore_optional_component<T: Component<Mutability = Mutable>>(
    entity: &mut EntityCommands,
    current: Option<Mut<T>>,
    restored: Option<T>,
    remove_if_absent: bool,
) {
    match (current, restored) {
        (Some(mut current), Some(restored)) => *current = restored,
        (None, Some(restored)) => {
            entity.insert(restored);
        }
        (Some(_), None) if remove_if_absent => {
            entity.remove::<T>();
        }
        _ => {}
    }
}
