use bevy::picking::{events::PointerPress, pointer::PointerButton};
use bevy::prelude::*;

use openzt2_interface_state::{
    camera::camera_runtime_state_types::ZooCamera,
    information::entity_selection_types::{Inspectable, SelectionRequest},
    input::input_types::ActiveInputDevice,
    ui::pointer_capture_types::UiPointerCapture,
};
use openzt2_world_state::world_spawn::{
    world_entity_crating::CratedWorldEntity, world_entity_view_filter::HiddenByWorldViewFilter,
    world_membership_types::WorldMember,
};
use openzt2_zoo_state::{
    construction::construction_tool_and_placement_policy_types::ConstructionTool,
    placement::placement_preview_types::RelocatingPlacedObject,
};

pub(super) fn mark_inspectable_meshes_for_picking(
    mut commands: Commands,
    meshes: Query<
        Entity,
        (
            With<Mesh3d>,
            Without<Pickable>,
            Or<(Added<Mesh3d>, Changed<ChildOf>)>,
        ),
    >,
    parents: Query<&ChildOf>,
    owners: Query<(), (With<Inspectable>, With<WorldMember>)>,
    new_owners: Query<
        Entity,
        (
            With<Inspectable>,
            With<WorldMember>,
            Or<(Added<Inspectable>, Added<WorldMember>)>,
        ),
    >,
    children: Query<&Children>,
    unmarked_meshes: Query<(), (With<Mesh3d>, Without<Pickable>)>,
) {
    let pickable = Pickable {
        is_hoverable: true,
        should_block_lower: false,
    };
    for entity in &meshes {
        if owners.contains(entity)
            || parents
                .iter_ancestors(entity)
                .any(|parent| owners.contains(parent))
        {
            commands.entity(entity).insert_if_new(pickable);
        }
    }
    for owner in &new_owners {
        for mesh in
            std::iter::once(owner).chain(children.iter_descendants_depth_first::<Children>(owner))
        {
            if unmarked_meshes.contains(mesh) {
                commands.entity(mesh).insert_if_new(pickable);
            }
        }
    }
}

pub(super) fn select_world_object_on_pointer_press(
    mut presses: MessageReader<PointerPress>,
    tool: Option<Res<ConstructionTool>>,
    active_input: Res<ActiveInputDevice>,
    ui_pointer_capture: Res<UiPointerCapture>,
    cameras: Query<(), With<ZooCamera>>,
    parents: Query<&ChildOf>,
    suppressed: Query<
        (),
        Or<(
            CratedWorldEntity,
            With<HiddenByWorldViewFilter>,
            With<RelocatingPlacedObject>,
        )>,
    >,
    owners: Query<(), (With<Inspectable>, With<WorldMember>)>,
    mut requests: MessageWriter<SelectionRequest>,
) {
    if ui_pointer_capture.over_ui
        || tool.is_some_and(|tool| {
            matches!(
                *tool,
                ConstructionTool::Delete
                    | ConstructionTool::SkyTrackPlacement(_)
                    | ConstructionTool::GroundTrackPlacement(_)
                    | ConstructionTool::Place(_)
                    | ConstructionTool::Fence(_)
                    | ConstructionTool::Path(_)
                    | ConstructionTool::Terrain(_)
                    | ConstructionTool::Biome
                    | ConstructionTool::TankEdit
            )
        })
    {
        presses.clear();
        return;
    }
    let nearest = presses
        .read()
        .filter_map(|press| {
            if press.button != PointerButton::Primary
                || !press.pointer.id.is_mouse()
                || !cameras.contains(press.hit.camera)
            {
                return None;
            }
            let mut owner = None;
            for entity in std::iter::once(press.entity).chain(parents.iter_ancestors(press.entity))
            {
                if suppressed.contains(entity) {
                    return None;
                }
                if owner.is_none() && owners.contains(entity) {
                    owner = Some(entity);
                }
            }
            owner.map(|owner| (owner, press.hit.depth))
        })
        .min_by(|(left, left_depth), (right, right_depth)| {
            left_depth
                .total_cmp(right_depth)
                .then_with(|| left.cmp(right))
        });
    if let Some((entity, _)) = nearest {
        requests.write(SelectionRequest {
            entity: Some(entity),
            source: active_input.source,
        });
    }
}
