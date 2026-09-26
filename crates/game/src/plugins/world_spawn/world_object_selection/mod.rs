use bevy::{
    picking::mesh_picking::ray_cast::{MeshRayCast, MeshRayCastSettings},
    prelude::*,
};

use crate::{
    plugins::camera::world_pointer_ray_types::WorldPointerRay,
    plugins::information::entity_selection_types::{Inspectable, SelectionRequest},
    plugins::input::input_types::ActiveInputDevice,
    plugins::ui::picking::UiPointerCapture,
};

use super::world_membership_types::WorldMember;

pub(super) fn request_selection_of_clicked_world_object_or_inspectable_ancestor(
    primary_pointer: Res<crate::plugins::input::input_types::PrimaryPointerInputState>,
    active_input: Res<ActiveInputDevice>,
    ui_pointer_capture: Res<UiPointerCapture>,
    world_pointer_ray: Res<WorldPointerRay>,
    mut mesh_ray_cast: MeshRayCast,
    parents: Query<&ChildOf>,
    inspectable_world_objects: Query<(), (With<Inspectable>, With<WorldMember>)>,
    mut selection_requests: MessageWriter<SelectionRequest>,
) {
    if !primary_pointer.just_pressed || ui_pointer_capture.over_ui {
        return;
    }
    let Some(ray) = world_pointer_ray.0 else {
        return;
    };
    let filter = |entity| {
        find_inspectable_world_object_in_ancestor_chain(
            entity,
            &parents,
            &inspectable_world_objects,
        )
        .is_some()
    };
    let Some((hit, _)) = mesh_ray_cast
        .cast_ray(ray, &MeshRayCastSettings::default().with_filter(&filter))
        .first()
    else {
        return;
    };
    if let Some(entity) =
        find_inspectable_world_object_in_ancestor_chain(*hit, &parents, &inspectable_world_objects)
    {
        selection_requests.write(SelectionRequest {
            entity: Some(entity),
            source: active_input.source,
        });
    }
}

fn find_inspectable_world_object_in_ancestor_chain(
    mut entity: Entity,
    parents: &Query<&ChildOf>,
    inspectable_world_objects: &Query<(), (With<Inspectable>, With<WorldMember>)>,
) -> Option<Entity> {
    loop {
        if inspectable_world_objects.contains(entity) {
            return Some(entity);
        }
        let Ok(parent) = parents.get(entity) else {
            return None;
        };
        entity = parent.parent();
    }
}
