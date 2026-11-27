use bevy::ecs::{relationship::RelationshipTarget, system::SystemParam};
use bevy::prelude::*;

use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::{
    WorldDefinitions, WorldDefinitionsView,
};
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_data::AssetId;
use openzt2_interface_state::camera::camera_runtime_state_types::ZooCamera;
use openzt2_interface_state::information::entity_selection_types::{InfoPanel, WorldUiSelection};
use openzt2_interface_state::photos::photo_capture_types::AlbumPhotoReadbacks;
use openzt2_world_state::world_spawn::prefab_presentation_types::PrefabPresentation;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_world_state::world_spawn::world_terrain_hydration::WorldTerrainHorizontalBounds;
use openzt2_zoo_state::animal_health::types::Dead;
use openzt2_zoo_state::extinct_animals::fossil_collection_and_assembly_types::{
    FossilAssemblyTable, FossilPlacementSurface,
};
use openzt2_zoo_state::extinct_animals::fossil_recovery_types::FossilSiteKind;
use openzt2_zoo_state::guests::guest_simulation_types::Guest;
use openzt2_zoo_state::shows::show_schedule_types::{ScheduledShowPerformancePlan, ShowStageRows};
use openzt2_zoo_state::shows::show_stage_types::ShowStage;
use openzt2_zoo_state::transport_tours::transport_vehicle_types::TransportVehicle;

use super::*;
use crate::immersive_modes::super_staff_avatar_start_position_search::SuperStaffAvatarStartSurface;

#[derive(SystemParam)]
pub(in crate::immersive_modes) struct ModeDefinitions<'w> {
    assets: Res<'w, Assets<WorldDefinitionAsset>>,
    current: Res<'w, WorldDefinitions>,
}

impl ModeDefinitions<'_> {
    pub(super) fn get(&self) -> Result<WorldDefinitionsView<'_>, ModeEntryFailure> {
        self.current
            .get(&self.assets)
            .ok_or(ModeEntryFailure::MissingTool)
    }
}

fn subject(subject: Option<Entity>) -> Result<Entity, ModeEntryFailure> {
    subject.ok_or(ModeEntryFailure::MissingSubject)
}

fn camera(cameras: &Query<Entity, With<ZooCamera>>) -> Result<Entity, ModeEntryFailure> {
    cameras.single().map_err(|_| ModeEntryFailure::RuleDenied)
}

pub(in crate::immersive_modes) fn validate_guest(
    requests: Query<(Entity, &PendingMode<GuestViewControl>), With<ModeReservation>>,
    guests: Query<&WorldMember, With<Guest>>,
    definitions: ModeDefinitions,
    cameras: Query<(Entity, &WorldMember, &CameraMode), With<ZooCamera>>,
    output: ModeAdmission,
) {
    let Ok((session, request)) = requests.single() else {
        return;
    };
    let result = (|| {
        let (camera, world, mode) = cameras.single().map_err(|_| ModeEntryFailure::RuleDenied)?;
        if *mode != CameraMode::Overhead {
            return Err(ModeEntryFailure::RuleDenied);
        }
        let subject = request.subject.filter(|subject| {
            guests
                .get(*subject)
                .is_ok_and(|guest| guest.root == world.root)
        });
        let admission =
            Admission::from_authored(definitions.get()?, ImmersiveMode::GuestView, camera)?;
        Ok((admission, subject))
    })();
    let (admission, subject) = match result {
        Ok((admission, subject)) => (Ok(admission), subject),
        Err(reason) => (Err(reason), None),
    };
    commit_admitted_mode::<GuestViewControl>(
        session,
        ImmersiveMode::GuestView,
        subject,
        admission,
        &definitions,
        output,
    );
}

pub(in crate::immersive_modes) fn validate_first_person(
    requests: Query<(Entity, &PendingMode<FirstPersonControl>), With<ModeReservation>>,
    subjects: Query<
        Has<TransportVehicle>,
        (Or<(With<NavAgent>, With<TransportVehicle>)>, Without<Dead>),
    >,
    definitions: ModeDefinitions,
    cameras: Query<Entity, With<ZooCamera>>,
    output: ModeAdmission,
) {
    let Ok((session, request)) = requests.single() else {
        return;
    };
    let result = (|| {
        let subject = subject(request.subject)?;
        let definitions = definitions.get()?;
        let riding = subjects
            .get(subject)
            .map_err(|_| ModeEntryFailure::WrongSubject)?;
        let mut admission =
            Admission::from_authored(definitions, ImmersiveMode::FirstPerson, camera(&cameras)?)?;
        admission.lock_subject &= !riding;
        Ok(admission)
    })();
    commit_admitted_mode::<FirstPersonControl>(
        session,
        ImmersiveMode::FirstPerson,
        request.subject,
        result,
        &definitions,
        output,
    );
}

pub(in crate::immersive_modes) fn validate_search(
    requests: Query<
        (
            Entity,
            &PendingMode<FossilSearchControl>,
            Option<&PendingFossilSearchDock>,
        ),
        With<ModeReservation>,
    >,
    sites: Query<(Entity, &WorldMember, &GlobalTransform), With<FossilSiteKind>>,
    definitions: ModeDefinitions,
    cameras: Query<(Entity, &WorldMember, &Transform), With<ZooCamera>>,
    scenes: Res<Assets<ScenePrefabAsset>>,
    server: Res<AssetServer>,
    world_bounds: Query<&WorldTerrainHorizontalBounds>,
    start_surface: SuperStaffAvatarStartSurface,
    mut output: ModeAdmission,
) {
    let Ok((session, request, pending_dock)) = requests.single() else {
        return;
    };
    let result = (|| {
        let definitions = definitions.get()?;
        let (camera, world, camera_pose) =
            cameras.single().map_err(|_| ModeEntryFailure::RuleDenied)?;
        let subject = if let Some(subject) = request.subject {
            sites
                .get(subject)
                .ok()
                .filter(|(_, site, _)| site.root == world.root)
                .map(|(entity, _, _)| entity)
        } else {
            sites
                .iter()
                .filter(|(_, site, _)| site.root == world.root)
                .min_by(|(left, _, left_pose), (right, _, right_pose)| {
                    left_pose
                        .translation()
                        .distance_squared(camera_pose.translation)
                        .total_cmp(
                            &right_pose
                                .translation()
                                .distance_squared(camera_pose.translation),
                        )
                        .then_with(|| left.to_bits().cmp(&right.to_bits()))
                })
                .map(|(entity, _, _)| entity)
        }
        .ok_or(ModeEntryFailure::WrongSubject)?;
        let mut admission =
            Admission::from_authored(definitions, ImmersiveMode::FossilSearch, camera)?;
        let handle = if let Some(pending) = pending_dock {
            pending.0.clone()
        } else {
            definitions
                .find_object(openzt2_game_data::AssetId::from_key("digpit"))
                .and_then(|pit| definitions.scene(pit.prefab))
                .ok_or(ModeEntryFailure::MissingTool)?
        };
        let Some(prefab) = scenes.get(&handle) else {
            if matches!(
                server.load_state(handle.id()),
                bevy::asset::LoadState::Failed(_)
            ) {
                return Err(ModeEntryFailure::MissingTool);
            }
            if pending_dock.is_none() {
                output
                    .commands
                    .entity(session)
                    .insert(PendingFossilSearchDock(handle));
            }
            return Ok(None);
        };
        let dock = prefab
            .prefab_attachment_transform(fossil_finding_dock_id())
            .ok_or(ModeEntryFailure::MissingTool)?;
        admission.fossil_finding_dock = Some(Transform::from_matrix(dock));
        let bounds = world_bounds
            .get(world.root)
            .map_err(|_| ModeEntryFailure::RuleDenied)?;
        attach_walking_avatar(
            &mut admission,
            definitions,
            *world,
            camera_pose,
            bounds,
            &start_surface,
        )?;
        Ok::<_, ModeEntryFailure>(Some((admission, subject)))
    })();
    let (admission, subject) = match result {
        Ok(None) => return,
        Ok(Some((admission, subject))) => (Ok(admission), Some(subject)),
        Err(reason) => (Err(reason), request.subject),
    };
    commit_admitted_mode::<FossilSearchControl>(
        session,
        ImmersiveMode::FossilSearch,
        subject,
        admission,
        &definitions,
        output,
    );
}

pub(in crate::immersive_modes) fn validate_assembly(
    requests: Query<
        (
            Entity,
            &PendingMode<FossilAssemblyControl>,
            Option<&PendingFossilAssemblyLab>,
        ),
        With<ModeReservation>,
    >,
    mut assemblies: Query<(
        &FossilAssemblyTable,
        &FossilPlacementSurface,
        &WorldMember,
        Option<&mut FossilAssemblyView>,
    )>,
    definitions: ModeDefinitions,
    cameras: Query<(Entity, &WorldMember), With<ZooCamera>>,
    prefabs: Res<Assets<ScenePrefabAsset>>,
    server: Res<AssetServer>,
    mut output: ModeAdmission,
) {
    let Ok((session, request, pending_lab)) = requests.single() else {
        return;
    };
    let result = (|| {
        let (camera, world) = cameras.single().map_err(|_| ModeEntryFailure::RuleDenied)?;
        let entity = subject(request.subject)?;
        let (table, surface, member, current) = assemblies
            .get_mut(entity)
            .map_err(|_| ModeEntryFailure::WrongSubject)?;
        if !surface.accepts_fossil_pieces || member.root != world.root {
            return Err(ModeEntryFailure::WrongSubject);
        }
        let definitions = definitions.get()?;
        let admission =
            Admission::from_authored(definitions, ImmersiveMode::FossilAssembly, camera)?;
        let object = definitions
            .find_object(table.object_definition_identifier)
            .ok_or(ModeEntryFailure::MissingTool)?;
        let lab = object
            .named_physical_presentations
            .iter()
            .find(|presentation| presentation.name == AssetId::from_key("puzzleinteriorobj"));
        let lab = if let Some(lab) = lab {
            let handle = pending_lab
                .map(|pending| pending.0.clone())
                .or_else(|| definitions.scene(lab.prefab))
                .ok_or(ModeEntryFailure::MissingTool)?;
            let Some(prefab) = prefabs.get(&handle) else {
                if matches!(
                    server.load_state(handle.id()),
                    bevy::asset::LoadState::Failed(_)
                ) {
                    return Err(ModeEntryFailure::MissingTool);
                }
                if pending_lab.is_none() {
                    output
                        .commands
                        .entity(session)
                        .insert(PendingFossilAssemblyLab(handle));
                }
                return Ok(None);
            };
            Some((handle, prefab, lab.scale))
        } else {
            None
        };
        let next = resolve_fossil_assembly_view(
            definitions,
            object,
            lab.as_ref().map(|(_, prefab, scale)| (*prefab, *scale)),
            admission.definition,
        )
        .ok_or(ModeEntryFailure::MissingTool)?;
        if let Some(mut current) = current {
            current.set_if_neq(next);
        } else {
            output.commands.entity(entity).insert(next);
        }
        if let Some((handle, _, scale)) = lab {
            output.commands.spawn((
                OwnedImmersiveEntity(session),
                ChildOf(entity),
                PrefabPresentation::new(handle),
                Transform::from_scale(Vec3::splat(scale)),
                Visibility::Inherited,
                *member,
            ));
        }
        if pending_lab.is_some() {
            output
                .commands
                .entity(session)
                .remove::<PendingFossilAssemblyLab>();
        }
        Ok(Some(admission))
    })();
    let result = match result {
        Ok(None) => return,
        Ok(Some(admission)) => Ok(admission),
        Err(reason) => Err(reason),
    };
    commit_admitted_mode::<FossilAssemblyControl>(
        session,
        ImmersiveMode::FossilAssembly,
        request.subject,
        result,
        &definitions,
        output,
    );
}

fn resolve_fossil_assembly_view(
    definitions: WorldDefinitionsView<'_>,
    object: &openzt2_game_data::world_definitions::world_objects::WorldObjectDefinition,
    lab: Option<(&ScenePrefabAsset, f32)>,
    camera: AssetId,
) -> Option<FossilAssemblyView> {
    let policy = find_authored_immersive_mode_policy(definitions, ImmersiveMode::FossilAssembly)?;
    let piece_rotation_radians = policy.fossil_piece_rotation_radians?;
    let camera = definitions.find_camera(camera)?;
    let table_relative_camera = if let Some((prefab, scale)) = lab {
        let attachment = |name| {
            let local = prefab.prefab_attachment_transform(AssetId::from_key(name))?;
            Some(local.transform_point3(Vec3::ZERO) * scale)
        };
        let focus = attachment("puzzlenode")?;
        let [a, b, c] = [
            attachment("cornernodea")?,
            attachment("cornernodeb")?,
            attachment("cornernodec")?,
        ];
        let normal = (b - a).cross(c - a).try_normalize()?;
        let distance = object
            .authored_runtime
            .numeric_behavior_properties
            .iter()
            .find_map(|&(key, value)| {
                (key == AssetId::from_key("f_puzzlecameradistancefromfocalpoint") && value > 0.0)
                    .then_some(value)
            })?;
        let up = Vec3::Y.reject_from_normalized(normal);
        let up = up.try_normalize().unwrap_or((c - a).normalize());
        Transform::from_translation(focus + normal * distance).looking_at(focus, up)
    } else {
        Transform::from_translation(Vec3::from_array(camera.offset_m))
            .with_rotation(Quat::from_rotation_x(camera.initial_pitch_radians))
    };
    Some(FossilAssemblyView {
        table_relative_camera,
        piece_rotation_radians,
    })
}

pub(in crate::immersive_modes) fn validate_photo(
    requests: Query<(Entity, &PendingMode<PhotoControl>), With<ModeReservation>>,
    readbacks: Query<&AlbumPhotoReadbacks>,
    definitions: ModeDefinitions,
    cameras: Query<Entity, With<ZooCamera>>,
    output: ModeAdmission,
) {
    let Ok((session, request)) = requests.single() else {
        return;
    };
    let result = (|| {
        let camera = camera(&cameras)?;
        if readbacks
            .get(camera)
            .is_ok_and(|pending| !pending.is_empty())
        {
            return Err(ModeEntryFailure::RuleDenied);
        }
        Admission::from_authored(definitions.get()?, ImmersiveMode::Photo, camera)
    })();
    commit_admitted_mode::<PhotoControl>(
        session,
        ImmersiveMode::Photo,
        request.subject,
        result,
        &definitions,
        output,
    );
}

pub(in crate::immersive_modes) fn validate_show(
    requests: Query<(Entity, &PendingMode<ShowEditControl>), With<ModeReservation>>,
    stages: Query<&ShowStageRows, With<ShowStage>>,
    performances: Query<(), With<ScheduledShowPerformancePlan>>,
    panels: Query<&InheritedVisibility, With<InfoPanel>>,
    selection: WorldUiSelection,
    definitions: ModeDefinitions,
    cameras: Query<Entity, With<ZooCamera>>,
    output: ModeAdmission,
) {
    let Ok((session, request)) = requests.single() else {
        return;
    };
    let result = (|| {
        let subject = subject(request.subject)?;
        if !stages
            .get(subject)
            .is_ok_and(|rows| rows.iter().any(|row| performances.contains(row)))
            || selection.subject() != Some(subject)
            || !panels.iter().any(|visible| visible.get())
        {
            return Err(ModeEntryFailure::WrongSubject);
        }
        Admission::from_authored(
            definitions.get()?,
            ImmersiveMode::ShowEdit,
            camera(&cameras)?,
        )
    })();
    commit_admitted_mode::<ShowEditControl>(
        session,
        ImmersiveMode::ShowEdit,
        request.subject,
        result,
        &definitions,
        output,
    );
}

pub(in crate::immersive_modes) fn validate_staff(
    requests: Query<(Entity, &PendingMode<StaffControl>), With<ModeReservation>>,
    definitions: ModeDefinitions,
    cameras: Query<(Entity, &Transform, &WorldMember), With<ZooCamera>>,
    world_bounds: Query<&WorldTerrainHorizontalBounds>,
    start_surface: SuperStaffAvatarStartSurface,
    output: ModeAdmission,
) {
    let Ok((session, request)) = requests.single() else {
        return;
    };
    let result = (|| {
        let definitions = definitions.get()?;
        let (camera, overhead, world) = cameras
            .single()
            .map_err(|_| ModeEntryFailure::MissingTool)?;
        let mut admission =
            Admission::from_authored(definitions, ImmersiveMode::SuperStaff, camera)?;
        let bounds = world_bounds
            .get(world.root)
            .map_err(|_| ModeEntryFailure::RuleDenied)?;
        attach_walking_avatar(
            &mut admission,
            definitions,
            *world,
            overhead,
            bounds,
            &start_surface,
        )?;
        Ok(admission)
    })();
    commit_admitted_mode::<StaffControl>(
        session,
        ImmersiveMode::SuperStaff,
        request.subject,
        result,
        &definitions,
        output,
    );
}

fn attach_walking_avatar(
    admission: &mut Admission,
    definitions: WorldDefinitionsView<'_>,
    world: WorldMember,
    camera: &Transform,
    bounds: &WorldTerrainHorizontalBounds,
    surface: &SuperStaffAvatarStartSurface,
) -> Result<(), ModeEntryFailure> {
    if !definitions
        .find_camera(admission.definition)
        .is_some_and(|camera| camera.first_person_motion.is_some())
    {
        return Err(ModeEntryFailure::MissingTool);
    }
    let (heading, _, _) = camera.rotation.to_euler(EulerRot::YXZ);
    let position =
        surface.find_avatar_start_position(world.root, camera.translation, heading, bounds);
    admission.avatar = Some(StaffAvatar {
        transform: Transform::from_translation(position)
            .with_rotation(Quat::from_rotation_y(heading)),
        world,
    });
    admission.lock_subject = true;
    Ok(())
}
