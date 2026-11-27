use bevy::{
    picking::{
        mesh_picking::ray_cast::{MeshRayCast, MeshRayCastSettings},
        pointer::{PointerAction, PointerButton, PointerInput},
    },
    prelude::*,
};
use openzt2_extinct_animals::extinct_animals::UpdateFossilAssemblyPiecePose;
use openzt2_game_assets::world_definitions::{
    world_definition_asset_set_state_and_borrowing_queries::{
        WorldDefinitions, WorldDefinitionsView,
    },
    world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
};
use openzt2_interface_state::{
    camera::world_pointer_ray_types::WorldPointerRay,
    immersive_modes::immersive_mode_state_types::{
        ActiveImmersiveMode, FossilAssemblyView, ImmersiveSubject,
    },
    input::input_types::{ActionRequest, GameAction},
    ui::{
        input_frame_capture_types::AuthoredModalInputCapture,
        pointer_capture_types::{UiPointerCapture, WorldWheelInput},
    },
};
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::extinct_animals::fossil_collection_and_assembly_types::{
    CollectedFossilPiece, FossilAssemblyTable, FossilPieceInSlot, FossilPieceOfWorld,
    FossilSetAssembly, PlaceFossilPieceInAssemblyRequest,
};

use crate::immersive_modes::{
    immersive_control_execution_types::FossilAssemblyControl,
    immersive_mode_input_policy_operations::interaction_action_flags_allow_requested_game_action,
    immersive_mode_policy_types::AllowedInteractionActions,
};

#[derive(Component, Clone, Copy, PartialEq)]
pub(in crate::immersive_modes) struct DraggedFossilPiece {
    entity: Entity,
    plane_origin: Vec3,
    plane_normal: Vec3,
    cursor_offset: Vec3,
    pose: Transform,
}

impl DraggedFossilPiece {
    fn move_to_pointer(&mut self, ray: Ray3d) {
        let slope = ray.direction.dot(self.plane_normal);
        if slope.abs() <= f32::EPSILON {
            return;
        }
        let distance = (self.plane_origin - ray.origin).dot(self.plane_normal) / slope;
        if distance.is_finite() && distance >= 0.0 {
            self.pose.translation = ray.origin + *ray.direction * distance + self.cursor_offset;
        }
    }
}

type Pieces<'w, 's> = Query<
    'w,
    's,
    (
        &'static CollectedFossilPiece,
        &'static FossilPieceOfWorld,
        &'static Transform,
    ),
    Without<FossilPieceInSlot>,
>;

pub(in crate::immersive_modes) fn interact_with_active_fossil_assembly(
    mut pointer_events: MessageReader<PointerInput>,
    mut wheel: MessageReader<WorldWheelInput>,
    mut actions: MessageReader<ActionRequest>,
    pointer: Res<WorldPointerRay>,
    capture: Res<UiPointerCapture>,
    modal: Res<AuthoredModalInputCapture>,
    mut sessions: Query<
        (
            Entity,
            &ImmersiveSubject,
            Option<&AllowedInteractionActions>,
            Option<&mut DraggedFossilPiece>,
        ),
        (With<ActiveImmersiveMode>, With<FossilAssemblyControl>),
    >,
    tables: Query<
        (
            &WorldMember,
            Option<&FossilSetAssembly>,
            Option<&FossilAssemblyView>,
        ),
        With<FossilAssemblyTable>,
    >,
    pieces: Pieces,
    parents: Query<&ChildOf>,
    mut picking: MeshRayCast,
    definitions: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    (mut poses, mut placements): (
        MessageWriter<UpdateFossilAssemblyPiecePose>,
        MessageWriter<PlaceFossilPieceInAssemblyRequest>,
    ),
    mut commands: Commands,
) {
    let (Ok((session, subject, allowed, current)), Some(definitions)) =
        (sessions.single_mut(), definitions.get(&assets))
    else {
        pointer_events.clear();
        wheel.clear();
        actions.clear();
        return;
    };
    let table = subject.0;
    let Ok((world, set, rotation_step)) = tables.get(table) else {
        return;
    };
    let permits = |action| interaction_action_flags_allow_requested_game_action(allowed, action);
    let mut turns = 0.0;
    let mut cancelled = modal.0.is_some();
    for action in actions.read().filter(|action| permits(action.action)) {
        match action.action {
            GameAction::Cancel => cancelled = true,
            GameAction::RotateLeft | GameAction::RotateObjectLeft => turns -= 1.0,
            GameAction::RotateRight | GameAction::RotateObjectRight => turns += 1.0,
            _ => {}
        }
    }
    if cancelled {
        pointer_events.clear();
        wheel.clear();
        if current.is_some() {
            commands.entity(session).remove::<DraggedFossilPiece>();
        }
        return;
    }
    for event in wheel.read() {
        if permits(GameAction::RotateObjectRight) {
            turns -= event.notches.signum();
        }
    }
    let ray = pointer.0.filter(|_| !capture.over_ui);
    let mut dragged = current
        .as_deref()
        .copied()
        .filter(|drag| pieces.contains(drag.entity));
    let selectable_piece = |mesh| {
        std::iter::once(mesh)
            .chain(parents.iter_ancestors::<ChildOf>(mesh))
            .find(|&entity| {
                pieces.get(entity).is_ok_and(|(piece, member, _)| {
                    world.root == member.0
                        && definitions
                            .find_fossil_piece(piece.0)
                            .is_some_and(|piece| set.is_none_or(|set| set.0 == piece.set))
                })
            })
    };
    let update = |drag: &mut DraggedFossilPiece, turns: f32| {
        if let Some(ray) = ray {
            drag.move_to_pointer(ray);
        }
        if let Some(step) =
            rotation_step.filter(|step| step.piece_rotation_radians.is_finite() && turns != 0.0)
        {
            drag.pose.rotate(Quat::from_axis_angle(
                drag.plane_normal,
                step.piece_rotation_radians * turns,
            ));
        }
    };
    for event in pointer_events
        .read()
        .filter(|event| event.pointer_id.is_mouse())
    {
        match event.action {
            PointerAction::Press(PointerButton::Primary)
                if dragged.is_none() && permits(GameAction::PrimaryPointer) =>
            {
                let Some(ray) = ray else { continue };
                let pickable = |entity| selectable_piece(entity).is_some();
                let settings = MeshRayCastSettings::default().with_filter(&pickable);
                let Some((mesh, hit)) = picking.cast_ray(ray, &settings).first() else {
                    continue;
                };
                let Some(entity) = selectable_piece(*mesh) else {
                    continue;
                };
                let Ok((_, _, pose)) = pieces.get(entity) else {
                    continue;
                };
                dragged = Some(DraggedFossilPiece {
                    entity,
                    plane_origin: hit.point,
                    plane_normal: pose.rotation * Vec3::Z,
                    cursor_offset: pose.translation - hit.point,
                    pose: *pose,
                });
            }
            PointerAction::Release(PointerButton::Primary) => {
                if let Some(mut drag) = dragged.take() {
                    update(&mut drag, std::mem::take(&mut turns));
                    if let Some(request) = release_request(definitions, &pieces, table, drag) {
                        placements.write(request);
                    }
                }
            }
            _ => {}
        }
    }
    if let Some(mut drag) = dragged {
        update(&mut drag, turns);
        let changed = if let Some(mut state) = current {
            state.set_if_neq(drag)
        } else {
            commands.entity(session).insert(drag);
            true
        };
        if changed {
            poses.write(UpdateFossilAssemblyPiecePose {
                assembly: table,
                piece: drag.entity,
                pose: drag.pose,
            });
        }
    } else if current.is_some() {
        commands.entity(session).remove::<DraggedFossilPiece>();
    }
}

fn release_request(
    definitions: WorldDefinitionsView<'_>,
    pieces: &Pieces,
    table: Entity,
    drag: DraggedFossilPiece,
) -> Option<PlaceFossilPieceInAssemblyRequest> {
    let (collected, _, _) = pieces.get(drag.entity).ok()?;
    let piece = definitions.find_fossil_piece(collected.0)?;
    let global_index = definitions
        .fossil_pieces()
        .position(|entry| entry.id == piece.id)?;
    let slot = definitions
        .find_fossil_set(piece.set)?
        .pieces
        .iter()
        .position(|&id| id == piece.id)?;
    Some(PlaceFossilPieceInAssemblyRequest {
        assembly_entity: table,
        global_piece_index: u16::try_from(global_index).ok()?,
        authored_slot_index: u16::try_from(slot).ok()?,
        piece: drag.entity,
        released_pose: drag.pose,
    })
}
