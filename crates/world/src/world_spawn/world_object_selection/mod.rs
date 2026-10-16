use bevy::camera::visibility::RenderLayers;
use bevy::picking::mesh_picking::ray_cast::MeshRayCast;
use bevy::picking::mesh_picking::ray_cast::MeshRayCastSettings;
use bevy::prelude::*;

use openzt2_interface_state::camera::world_pointer_ray_types::WorldPointerRay;
use openzt2_interface_state::information::entity_selection_types::Inspectable;
use openzt2_interface_state::information::entity_selection_types::SelectionRequest;
use openzt2_interface_state::input::input_types::ActiveInputDevice;
use openzt2_interface_state::ui::pointer_capture_types::UiPointerCapture;

use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_world_state::world_spawn::{
    world_entity_crating::WorldEntityIsCrated, world_entity_view_filter::HiddenByWorldViewFilter,
};
use openzt2_zoo_state::placement::placement_preview_types::RelocatingPlacedObject;

pub(super) fn request_selection_of_clicked_world_object_or_inspectable_ancestor(
    primary_pointer: Res<openzt2_interface_state::input::input_types::PrimaryPointerInputState>,
    active_input: Res<ActiveInputDevice>,
    ui_pointer_capture: Res<UiPointerCapture>,
    world_pointer_ray: Res<WorldPointerRay>,
    mut mesh_ray_cast: MeshRayCast,
    parents: Query<&ChildOf>,
    suppressed: Query<
        (),
        Or<(
            With<WorldEntityIsCrated>,
            With<HiddenByWorldViewFilter>,
            With<RelocatingPlacedObject>,
        )>,
    >,
    layers: Query<&RenderLayers>,
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
        if std::iter::once(entity)
            .chain(parents.iter_ancestors(entity))
            .any(|ancestor| suppressed.contains(ancestor))
            || layers
                .get(entity)
                .is_ok_and(|layers| *layers == RenderLayers::none())
        {
            return false;
        }
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
