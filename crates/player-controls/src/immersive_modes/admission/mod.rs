mod validation;

use std::marker::PhantomData;

use bevy::{ecs::system::SystemParam, prelude::*};

use openzt2_game_assets::application_lifecycle::GamePhase;
use openzt2_game_assets::scene_prefab::ScenePrefabAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use openzt2_game_data::AssetId;
use openzt2_interface_state::camera::camera_control_message_types::SetCameraMode;
use openzt2_interface_state::camera::camera_runtime_state_types::{CameraMode, FirstPersonLocomotionTuning};
use openzt2_interface_state::photos::photo_capture_types::PhotoMode;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::locomotion::locomotion_types::{DirectLocomotion, NavAgent};

use super::immersive_control_execution_types::{
    FirstPersonControl, FirstPersonViewAngles, FossilAssemblyControl, FossilSearchControl,
    GuestViewControl, ShowEditControl,
};
use super::immersive_mode_control_types::PhotoControl;
use super::immersive_mode_entry_and_policy_operations::{
    attach_authored_immersive_mode_consumer_policies, calculate_initial_first_person_view_angles,
    find_authored_immersive_mode_policy,
};
use super::immersive_mode_message_types::{
    EnterImmersiveMode, ImmersiveModeEntered, ImmersiveModeRejected, ModeEntryFailure,
};
use super::immersive_mode_state_types::{
    ActiveImmersiveMode, ControlledEntity, EquippedInteractionTool, FossilAssemblyView,
    GuestViewTileCount, ImmersiveCamera, ImmersiveMode, ImmersiveSubject,
};
pub(super) use validation::{
    validate_assembly, validate_first_person, validate_guest, validate_photo, validate_search,
    validate_show, validate_staff,
};

#[derive(Component)]
pub(crate) struct ModeReservation;

#[derive(Component, Default)]
pub(super) struct StaffControl;

#[derive(Component)]
pub(in crate::immersive_modes) struct PendingMode<M: Component> {
    subject: Option<Entity>,
    marker: PhantomData<M>,
}

#[derive(Component)]
#[relationship(relationship_target = OwnedImmersiveEntities)]
struct OwnedImmersiveEntity(Entity);

#[derive(Component)]
#[relationship_target(relationship = OwnedImmersiveEntity, linked_spawn)]
struct OwnedImmersiveEntities(Vec<Entity>);

#[derive(Component)]
pub(super) struct PendingFossilSearchDock(Handle<ScenePrefabAsset>);

#[derive(Component)]
pub(super) struct PendingFossilAssemblyLab(Handle<ScenePrefabAsset>);

fn fossil_finding_dock_id() -> AssetId {
    AssetId::from_key("dock_fossilfinding")
}

struct StaffAvatar {
    transform: Transform,
    world: WorldMember,
}

struct Admission {
    camera: Entity,
    definition: AssetId,
    tool: AssetId,
    restore_camera: bool,
    lock_subject: bool,
    initial_angles: Option<FirstPersonViewAngles>,
    avatar: Option<StaffAvatar>,
    fossil_finding_dock: Option<Transform>,
}

impl Admission {
    fn from_authored(
        definitions: WorldDefinitionsView<'_>,
        mode: ImmersiveMode,
        camera: Entity,
    ) -> Result<Self, ModeEntryFailure> {
        let policy = find_authored_immersive_mode_policy(definitions, mode)
            .ok_or(ModeEntryFailure::MissingTool)?;
        let definition = AssetId(policy.camera.0);
        if mode != ImmersiveMode::GuestView {
            definitions
                .find_camera(definition)
                .ok_or(ModeEntryFailure::MissingTool)?;
        }
        Ok(Self {
            camera,
            definition,
            tool: AssetId(policy.id.0),
            restore_camera: mode != ImmersiveMode::GuestView && policy.restore_camera_on_exit,
            lock_subject: mode != ImmersiveMode::GuestView && policy.lock_subject,
            initial_angles: matches!(
                mode,
                ImmersiveMode::FirstPerson
                    | ImmersiveMode::SuperStaff
                    | ImmersiveMode::FossilSearch
            )
            .then(|| calculate_initial_first_person_view_angles(definitions, definition)),
            avatar: None,
            fossil_finding_dock: None,
        })
    }
}

fn pending<M: Component>(subject: Option<Entity>) -> PendingMode<M> {
    PendingMode {
        subject,
        marker: PhantomData,
    }
}

pub(super) fn reserve_mode(commands: &mut Commands, request: EnterImmersiveMode) {
    let mut session = commands.spawn((ModeReservation, DespawnOnExit(GamePhase::InGame)));
    let subject = request.subject;
    match request.mode {
        ImmersiveMode::GuestView => session.insert(pending::<GuestViewControl>(subject)),
        ImmersiveMode::FirstPerson => session.insert(pending::<FirstPersonControl>(subject)),
        ImmersiveMode::FossilSearch => session.insert(pending::<FossilSearchControl>(subject)),
        ImmersiveMode::FossilAssembly => session.insert(pending::<FossilAssemblyControl>(subject)),
        ImmersiveMode::Photo => session.insert(pending::<PhotoControl>(subject)),
        ImmersiveMode::ShowEdit => session.insert(pending::<ShowEditControl>(subject)),
        ImmersiveMode::SuperStaff => session.insert(pending::<StaffControl>(None)),
    };
}

#[derive(SystemParam)]
pub(in crate::immersive_modes) struct ModeAdmission<'w, 's> {
    commands: Commands<'w, 's>,
    locomotion: Query<'w, 's, &'static mut DirectLocomotion>,
    locomotion_tuning: Query<'w, 's, &'static mut FirstPersonLocomotionTuning>,
    camera_requests: MessageWriter<'w, SetCameraMode>,
    entered: MessageWriter<'w, ImmersiveModeEntered>,
    rejected: MessageWriter<'w, ImmersiveModeRejected>,
}

fn commit_admitted_mode<M: Component + Default>(
    session: Entity,
    mode: ImmersiveMode,
    mut subject: Option<Entity>,
    result: Result<Admission, ModeEntryFailure>,
    definitions: &validation::ModeDefinitions,
    output: ModeAdmission,
) {
    let ModeAdmission {
        mut commands,
        mut camera_requests,
        mut locomotion,
        mut locomotion_tuning,
        mut entered,
        mut rejected,
    } = output;
    let admission = match result {
        Ok(admission) => admission,
        Err(reason) => {
            rejected.write(ImmersiveModeRejected { mode, reason });
            commands.entity(session).despawn();
            return;
        }
    };
    if let Some(avatar) = admission.avatar {
        subject = Some(
            commands
                .spawn((
                    Name::new("first-person avatar"),
                    avatar.transform,
                    GlobalTransform::from(avatar.transform),
                    avatar.world,
                    OwnedImmersiveEntity(session),
                ))
                .id(),
        );
        commands.entity(session).insert(FirstPersonControl);
    }
    let camera_mode = match (mode, subject) {
        (ImmersiveMode::GuestView, _) => CameraMode::Overhead,
        (ImmersiveMode::Photo, _) => CameraMode::Free,
        (ImmersiveMode::ShowEdit, Some(_)) => CameraMode::Follow,
        (_, Some(_)) => CameraMode::FirstPerson,
        (_, None) => {
            rejected.write(ImmersiveModeRejected {
                mode,
                reason: ModeEntryFailure::MissingSubject,
            });
            commands.entity(session).despawn();
            return;
        }
    };
    let mut entity = commands.entity(session);
    entity.remove::<(ModeReservation, PendingMode<M>)>();
    entity.insert(M::default());
    entity.insert((
        ActiveImmersiveMode {
            mode,
            restore_camera_on_exit: admission.restore_camera,
        },
        EquippedInteractionTool {
            policy: admission.tool,
        },
        ImmersiveCamera(admission.camera),
    ));
    if let Some(subject) = subject.filter(|_| mode != ImmersiveMode::GuestView) {
        entity.insert(ImmersiveSubject(subject));
    }
    if let Some(angles) = admission.initial_angles {
        entity.insert(angles);
    }
    if matches!(
        mode,
        ImmersiveMode::FirstPerson
            | ImmersiveMode::SuperStaff
            | ImmersiveMode::Photo
            | ImmersiveMode::FossilSearch
    ) {
        entity.insert(super::first_person_motion::FirstPersonMotion::default());
    }
    if mode == ImmersiveMode::GuestView {
        entity.insert(GuestViewTileCount::default());
    }
    if mode == ImmersiveMode::SuperStaff {
        entity.insert(
            openzt2_zoo_staff::maintenance::first_person_waste_cleaning::FirstPersonStaffAimOffset(
                1.0,
            ),
        );
    }
    if mode == ImmersiveMode::FossilSearch {
        entity.insert(openzt2_zoo_state::extinct_animals::fossil_recovery_types::ActiveFossilSonarArtifactTracking::default());
        commands.write_message(openzt2_zoo_state::extinct_animals::fossil_recovery_types::ActivateFossilSiteMarkersRequest);
    }
    if admission.lock_subject {
        if let Some(subject) = subject {
            commands.entity(subject).insert((ControlledEntity {
                controller: session,
            },));
            if !matches!(
                mode,
                ImmersiveMode::SuperStaff | ImmersiveMode::FossilSearch
            ) {
                if let Ok(mut current) = locomotion.get_mut(subject) {
                    current.set_if_neq(DirectLocomotion::default());
                } else {
                    commands.entity(subject).insert(DirectLocomotion::default());
                }
            }
            if mode == ImmersiveMode::FirstPerson {
                if let Ok(mut current) = locomotion_tuning.get_mut(subject) {
                    current.set_if_neq(FirstPersonLocomotionTuning::default());
                } else {
                    commands
                        .entity(subject)
                        .insert(FirstPersonLocomotionTuning::default());
                }
            }
        }
    }
    if mode == ImmersiveMode::Photo {
        commands.entity(admission.camera).insert(PhotoMode);
    }
    if let Ok(definitions) = definitions.get() {
        attach_authored_immersive_mode_consumer_policies(definitions, mode, session, &mut commands);
        if mode == ImmersiveMode::GuestView {
            if let Some(policy) = find_authored_immersive_mode_policy(definitions, mode) {
                let scene = |id| definitions.scene(id).map(|handle| (id, handle));
                commands.entity(session).insert(
                    super::guest_view_navigation_marker_types::GuestViewProjection {
                        cursor: scene(policy.guest_ground_cursor),
                        marker: scene(policy.guest_navigation_marker),
                        excluded_subject: subject,
                    },
                );
            }
        }
    }
    if let Some(dock) = admission.fossil_finding_dock {
        commands.entity(session).remove::<PendingFossilSearchDock>();
        commands.spawn((
            openzt2_world_state::world_spawn::prefab_authored_attachment_identifier::PrefabAuthoredAttachmentIdentifier(
                fossil_finding_dock_id(),
            ),
            dock,
            Visibility::Inherited,
            ChildOf(admission.camera),
            OwnedImmersiveEntity(session),
        ));
    }
    if mode != ImmersiveMode::GuestView {
        camera_requests.write(SetCameraMode {
            mode: camera_mode,
            subject: None,
            definition: admission.definition,
            transition_seconds: None,
        });
    }
    entered.write(ImmersiveModeEntered { mode, subject });
}
