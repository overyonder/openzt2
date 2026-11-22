mod validation;

use std::marker::PhantomData;

use bevy::prelude::*;

use openzt2_game_assets::application_lifecycle::GamePhase;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use openzt2_game_data::AssetId;
use openzt2_interface_state::camera::camera_control_message_types::SetCameraMode;
use openzt2_interface_state::camera::camera_runtime_state_types::CameraMode;
use openzt2_interface_state::photos::photo_capture_types::PhotoMode;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::locomotion::locomotion_types::{DirectLocomotion, NavAgent};

use super::immersive_control_execution_types::{
    FirstPersonControl, FirstPersonViewAngles, FossilAssemblyControl, FossilSearchControl,
    GuestViewControl, GuestViewTileCount, ShowEditControl,
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
    ActiveImmersiveMode, ControlledEntity, EquippedInteractionTool, ImmersiveCamera, ImmersiveMode,
    ImmersiveSubject,
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
    result: Result<Admission, ModeEntryFailure>,
    marker: PhantomData<M>,
}

#[derive(Component)]
#[relationship(relationship_target = OwnedImmersiveAvatars)]
struct OwnedImmersiveAvatar(Entity);

#[derive(Component)]
#[relationship_target(relationship = OwnedImmersiveAvatar, linked_spawn)]
struct OwnedImmersiveAvatars(Vec<Entity>);

struct StaffAvatar {
    transform: Transform,
    navigation: NavAgent,
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
        definitions
            .find_camera(definition)
            .ok_or(ModeEntryFailure::MissingTool)?;
        Ok(Self {
            camera,
            definition,
            tool: AssetId(policy.id.0),
            restore_camera: policy.restore_camera_on_exit,
            lock_subject: policy.lock_subject,
            initial_angles: matches!(
                mode,
                ImmersiveMode::GuestView | ImmersiveMode::FirstPerson | ImmersiveMode::SuperStaff
            )
            .then(|| calculate_initial_first_person_view_angles(definitions, definition)),
            avatar: None,
        })
    }
}

fn pending<M: Component>(subject: Option<Entity>) -> PendingMode<M> {
    PendingMode {
        subject,
        result: Err(ModeEntryFailure::RuleDenied),
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

#[derive(bevy::ecs::query::QueryData)]
#[query_data(mutable)]
pub(super) struct ValidatedModes {
    session: Entity,
    guest: Option<&'static mut PendingMode<GuestViewControl>>,
    first_person: Option<&'static mut PendingMode<FirstPersonControl>>,
    search: Option<&'static mut PendingMode<FossilSearchControl>>,
    assembly: Option<&'static mut PendingMode<FossilAssemblyControl>>,
    photo: Option<&'static mut PendingMode<PhotoControl>>,
    show: Option<&'static mut PendingMode<ShowEditControl>>,
    staff: Option<&'static mut PendingMode<StaffControl>>,
}

fn take_result<M: Component + Default>(
    mut pending: Mut<PendingMode<M>>,
    entity: Entity,
    commands: &mut Commands,
) -> (Option<Entity>, Result<Admission, ModeEntryFailure>) {
    let subject = pending.subject;
    let result = std::mem::replace(&mut pending.result, Err(ModeEntryFailure::RuleDenied));
    let mut session = commands.entity(entity);
    session.remove::<PendingMode<M>>();
    if result.is_ok() {
        session.insert(M::default());
    }
    (subject, result)
}

pub(super) fn admit_validated_mode(
    mut pending: Query<ValidatedModes, With<ModeReservation>>,
    definitions: validation::ModeDefinitions,
    mut commands: Commands,
    mut camera_requests: MessageWriter<SetCameraMode>,
    mut entered: MessageWriter<ImmersiveModeEntered>,
    mut rejected: MessageWriter<ImmersiveModeRejected>,
) {
    let Ok(pending) = pending.single_mut() else {
        return;
    };
    let session = pending.session;
    let (mode, (mut subject, result)) = if let Some(pending) = pending.guest {
        (
            ImmersiveMode::GuestView,
            take_result(pending, session, &mut commands),
        )
    } else if let Some(pending) = pending.first_person {
        (
            ImmersiveMode::FirstPerson,
            take_result(pending, session, &mut commands),
        )
    } else if let Some(pending) = pending.search {
        (
            ImmersiveMode::FossilSearch,
            take_result(pending, session, &mut commands),
        )
    } else if let Some(pending) = pending.assembly {
        (
            ImmersiveMode::FossilAssembly,
            take_result(pending, session, &mut commands),
        )
    } else if let Some(pending) = pending.photo {
        (
            ImmersiveMode::Photo,
            take_result(pending, session, &mut commands),
        )
    } else if let Some(pending) = pending.show {
        (
            ImmersiveMode::ShowEdit,
            take_result(pending, session, &mut commands),
        )
    } else if let Some(pending) = pending.staff {
        (
            ImmersiveMode::SuperStaff,
            take_result(pending, session, &mut commands),
        )
    } else {
        return;
    };
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
                    Name::new("super staff avatar"),
                    avatar.transform,
                    GlobalTransform::from(avatar.transform),
                    avatar.navigation,
                    avatar.world,
                    OwnedImmersiveAvatar(session),
                ))
                .id(),
        );
        commands.entity(session).insert(FirstPersonControl);
    }
    let camera_mode = match (mode, subject) {
        (ImmersiveMode::Photo, _) => CameraMode::Free,
        (ImmersiveMode::ShowEdit, Some(subject)) => CameraMode::Follow(subject),
        (_, Some(subject)) => CameraMode::FirstPerson(subject),
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
    entity.remove::<ModeReservation>();
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
    if let Some(subject) = subject {
        entity.insert(ImmersiveSubject(subject));
    }
    if let Some(angles) = admission.initial_angles {
        entity.insert(angles);
    }
    if mode == ImmersiveMode::GuestView {
        entity.insert(GuestViewTileCount::default());
    }
    if mode == ImmersiveMode::FossilSearch {
        entity.insert(openzt2_zoo_state::extinct_animals::fossil_recovery_types::ActiveFossilSonarArtifactTracking::default());
        commands.write_message(openzt2_zoo_state::extinct_animals::fossil_recovery_types::ActivateFossilSiteMarkersRequest);
    }
    if admission.lock_subject {
        if let Some(subject) = subject {
            commands.entity(subject).insert((
                ControlledEntity {
                    controller: session,
                },
                DirectLocomotion {
                    local_axes: Vec2::ZERO,
                },
            ));
        }
    }
    if mode == ImmersiveMode::Photo {
        commands.entity(admission.camera).insert(PhotoMode);
    }
    if let Ok(definitions) = definitions.get() {
        attach_authored_immersive_mode_consumer_policies(definitions, mode, session, &mut commands);
    }
    camera_requests.write(SetCameraMode {
        mode: camera_mode,
        definition: admission.definition,
        transition_seconds: None,
    });
    entered.write(ImmersiveModeEntered { mode, subject });
}
