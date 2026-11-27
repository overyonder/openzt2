use avian3d::prelude::{SpatialQuery, SpatialQueryFilter};
use bevy::picking::pointer::{PointerButton, PointerInput};
use bevy::prelude::*;

use openzt2_game_assets::world_definitions::{
    world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
    world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
};
use openzt2_game_data::{world_definitions::staff_management::StaffRoleKind, AssetId};
use openzt2_interface_state::camera::world_pointer_ray_types::WorldPointerRay;
use openzt2_interface_state::information::entity_selection_types::WorldUiSelection;
use openzt2_interface_state::input::input_types::ActionRequest;
use openzt2_interface_state::input::input_types::GameAction;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentRoot;
use openzt2_world_state::world_spawn::world_membership_types::{DefinitionId, WorldMember};
use openzt2_zoo_state::{animal_health::types::Dead, animal_lifecycle::types::Animal};

use super::{
    staff_assignment_types::{
        KeeperAssignmentSelection, StaffAssignmentMode, StaffAssignmentRecord,
        StaffAssignmentTarget, TrainerAssignmentSelection,
    },
    staff_employment_types::{Staff, StaffRole},
};

#[derive(Message, Clone, Copy)]
pub struct StaffAssignmentPlaced {
    pub worker: Entity,
    pub position: Vec3,
}

pub(super) fn present_staff_assignment_placement(
    mut placements: MessageReader<StaffAssignmentPlaced>,
    definitions: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    effects: Option<
        MessageWriter<openzt2_game_assets::effect::SpawnCompleteParticleEffectDocument>,
    >,
) {
    let effect = definitions
        .get(&assets)
        .and_then(|definitions| definitions.effect(AssetId::from_key("staffassignedarea")));
    let (Some(effect), Some(mut effects)) = (effect, effects) else {
        placements.clear();
        return;
    };
    for placement in placements.read() {
        effects.write(
            openzt2_game_assets::effect::SpawnCompleteParticleEffectDocument {
                particle_effect_asset: effect.clone(),
                parent_entity: None,
                inherit_parent_rotation: false,
                effect_transform: Transform::from_translation(placement.position),
                manual_particle_count: 0,
            },
        );
    }
}

pub(super) fn assignment_constraint_matches(logic: u8, predicates: [Option<bool>; 3]) -> bool {
    let [spatial, object, key] = predicates.map(|value| value.unwrap_or(logic != 2));
    match logic {
        0 => spatial && object && key,
        1 => spatial || object || key,
        2 => spatial ^ object ^ key,
        3 => !(spatial || object || key),
        _ => false,
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn capture_selected_staff_assignment(
    mut pointer_inputs: MessageReader<PointerInput>,
    pointer_capture: Res<openzt2_interface_state::ui::pointer_capture_types::UiPointerCapture>,
    modal_input: Res<
        openzt2_interface_state::ui::input_frame_capture_types::AuthoredModalInputCapture,
    >,
    world_pointer_ray: Res<WorldPointerRay>,
    spatial_query: SpatialQuery,
    assignment_modes: Query<(), (With<UiDocumentRoot>, With<StaffAssignmentMode>)>,
    selected_entity: WorldUiSelection,
    workers: Query<(&StaffRole, &WorldMember, Option<&Children>), With<Staff>>,
    walkable: openzt2_zoo_state::terrain::dry_surface_eligibility::DrySurfaceEligibility,
    catalogues: (Res<WorldDefinitions>, Res<Assets<WorldDefinitionAsset>>),
    rows: Query<&StaffAssignmentTarget, With<StaffAssignmentRecord>>,
    animals: Query<(&DefinitionId, &WorldMember, &GlobalTransform), (With<Animal>, Without<Dead>)>,
    parents: Query<&ChildOf>,
    mut placed: MessageWriter<StaffAssignmentPlaced>,
    mut commands: Commands,
) {
    let captured = pointer_inputs.read().any(|input| {
        input.pointer_id.is_mouse() && input.button_just_pressed(PointerButton::Primary)
    });
    pointer_inputs.clear();
    if !captured
        || pointer_capture.over_ui
        || modal_input.0.is_some()
        || assignment_modes.is_empty()
    {
        return;
    }
    let (Some(ray), Some(worker)) = (world_pointer_ray.0, selected_entity.subject()) else {
        return;
    };
    let Ok((role, member, children)) = workers.get(worker) else {
        return;
    };
    let Some(definitions) = catalogues.0.get(&catalogues.1) else {
        return;
    };
    let Some(role) = definitions.find_staff(role.0) else {
        return;
    };
    let Some(hit) = spatial_query.cast_ray(
        ray.origin,
        ray.direction,
        f32::MAX,
        false,
        &SpatialQueryFilter::DEFAULT,
    ) else {
        return;
    };
    let position = match role.role {
        StaffRoleKind::Keeper => {
            let point = ray.origin + *ray.direction * hit.distance;
            if walkable.on_land_at_position(member.root, point).is_none() {
                return;
            }
            commands.spawn((ChildOf(worker), StaffAssignmentRecord::Point(point)));
            point
        }
        StaffRoleKind::Trainer => {
            let mut target = hit.entity;
            let (definition, target_world, transform) = loop {
                if let Ok(animal) = animals.get(target) {
                    break animal;
                }
                let Ok(parent) = parents.get(target) else {
                    return;
                };
                target = parent.parent();
            };
            if target_world.root != member.root
                || !definitions.find_object(definition.0).is_some_and(|object| {
                    object
                        .behavior_type_identifiers
                        .contains(&AssetId::from_key("b_showanimal"))
                })
                || children
                    .into_iter()
                    .flat_map(|children| children.iter())
                    .any(|child| rows.get(child).is_ok_and(|row| row.0 == target))
            {
                return;
            }
            commands.spawn((
                ChildOf(worker),
                StaffAssignmentRecord::Animal,
                StaffAssignmentTarget(target),
            ));
            transform.translation()
        }
        _ => return,
    };
    placed.write(StaffAssignmentPlaced { worker, position });
}

#[derive(Message)]
pub struct RemoveSelectedStaffAssignment {
    pub worker: Entity,
    pub trainer: bool,
}

pub(super) fn remove_selected_staff_assignment(
    mut messages: MessageReader<RemoveSelectedStaffAssignment>,
    workers: Query<&Children, With<Staff>>,
    rows: Query<&StaffAssignmentRecord>,
    mut keepers: Query<&mut KeeperAssignmentSelection>,
    mut trainers: Query<&mut TrainerAssignmentSelection>,
    mut commands: Commands,
) {
    let mut removed = bevy::ecs::entity::EntityHashSet::default();
    for message in messages.read() {
        let Ok(children) = workers.get(message.worker) else {
            continue;
        };
        let index = if message.trainer {
            trainers
                .get(message.worker)
                .ok()
                .and_then(|selection| selection.0)
        } else {
            keepers
                .get(message.worker)
                .ok()
                .and_then(|selection| selection.0)
        };
        let Some(index) = index else {
            continue;
        };
        let row = children
            .iter()
            .filter(|&row| {
                !removed.contains(&row)
                    && rows.get(row).is_ok_and(|record| {
                        matches!(record, StaffAssignmentRecord::Animal) == message.trainer
                    })
            })
            .nth(index as usize);
        if let Some(row) = row {
            removed.insert(row);
            commands.entity(row).try_despawn();
            if message.trainer {
                if let Ok(mut selection) = trainers.get_mut(message.worker) {
                    selection.set_if_neq(TrainerAssignmentSelection(None));
                }
            } else if let Ok(mut selection) = keepers.get_mut(message.worker) {
                selection.set_if_neq(KeeperAssignmentSelection(None));
            }
        }
    }
}

pub(super) fn retire_lost_trainer_assignment_rows(
    mut commands: Commands,
    orphaned: Query<(Entity, &StaffAssignmentRecord), Without<StaffAssignmentTarget>>,
) {
    for (entity, record) in &orphaned {
        if matches!(record, StaffAssignmentRecord::Animal) {
            commands.entity(entity).try_despawn();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_point_and_animal_records_use_their_only_constraint() {
        assert!(assignment_constraint_matches(0, [Some(true), None, None]));
        assert!(!assignment_constraint_matches(0, [Some(false), None, None]));
        assert!(assignment_constraint_matches(0, [None, Some(true), None]));
        assert!(!assignment_constraint_matches(0, [None, Some(false), None]));
    }

    #[test]
    fn removing_a_keeper_row_leaves_the_other_point_in_place() {
        let mut app = App::new();
        app.add_message::<RemoveSelectedStaffAssignment>()
            .add_systems(Update, remove_selected_staff_assignment);
        let worker = app
            .world_mut()
            .spawn((Staff, KeeperAssignmentSelection(Some(0))))
            .id();
        let a = app
            .world_mut()
            .spawn((ChildOf(worker), StaffAssignmentRecord::Point(Vec3::X)))
            .id();
        let b = app
            .world_mut()
            .spawn((ChildOf(worker), StaffAssignmentRecord::Point(Vec3::Z)))
            .id();
        app.world_mut()
            .write_message(RemoveSelectedStaffAssignment {
                worker,
                trainer: false,
            });
        app.update();
        assert!(app.world().get_entity(a).is_err());
        assert!(app.world().get::<StaffAssignmentRecord>(b).is_some());
        assert_eq!(
            app.world()
                .get::<KeeperAssignmentSelection>(worker)
                .unwrap()
                .0,
            None
        );
    }

    #[test]
    fn xor_and_nor_include_their_authored_defaults() {
        assert!(assignment_constraint_matches(2, [None, Some(true), None]));
        assert!(!assignment_constraint_matches(
            2,
            [Some(true), Some(true), None]
        ));
        assert!(!assignment_constraint_matches(3, [Some(false), None, None]));
        assert!(assignment_constraint_matches(3, [Some(false); 3]));
    }
}

/// Cancel leaves the staff-assignment tool without mutating any staff relationship.
pub(crate) fn leave_staff_assignment_tools_after_cancel_action(
    mut commands: Commands,
    mut action_requests: MessageReader<ActionRequest>,
    modal_input: Res<
        openzt2_interface_state::ui::input_frame_capture_types::AuthoredModalInputCapture,
    >,
    assignment_tool_roots: Query<Entity, (With<UiDocumentRoot>, With<StaffAssignmentMode>)>,
) {
    if modal_input.0.is_some() {
        action_requests.clear();
        return;
    }
    if action_requests
        .read()
        .any(|request| request.action == GameAction::Cancel)
    {
        for assignment_tool_root in &assignment_tool_roots {
            commands
                .entity(assignment_tool_root)
                .remove::<StaffAssignmentMode>();
        }
    }
}
