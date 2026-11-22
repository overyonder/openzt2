use bevy::ecs::{relationship::RelationshipTarget, system::SystemParam};
use bevy::prelude::*;

use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::{
    WorldDefinitions, WorldDefinitionsView,
};
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_interface_state::camera::camera_runtime_state_types::ZooCamera;
use openzt2_interface_state::information::entity_selection_types::{InfoPanel, WorldUiSelection};
use openzt2_interface_state::photos::photo_capture_types::AlbumPhotoReadbacks;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_world_state::world_spawn::world_terrain_hydration::WorldTerrainHorizontalBounds;
use openzt2_zoo_state::animal_health::types::Dead;
use openzt2_zoo_state::extinct_animals::fossil_collection_and_assembly_types::{
    FossilAssemblyTable, FossilCollections, FossilPlacementSurface,
};
use openzt2_zoo_state::extinct_animals::fossil_recovery_types::FossilRecoverySite;
use openzt2_zoo_state::guests::guest_simulation_types::Guest;
use openzt2_zoo_state::locomotion::locomotion_types::NavFlags;
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
    mut request: Query<&mut PendingMode<GuestViewControl>>,
    guests: Query<(), With<Guest>>,
    definitions: ModeDefinitions,
    cameras: Query<Entity, With<ZooCamera>>,
) {
    let Ok(mut request) = request.single_mut() else {
        return;
    };
    request.result = (|| {
        let subject = subject(request.subject)?;
        let definitions = definitions.get()?;
        guests
            .get(subject)
            .map_err(|_| ModeEntryFailure::WrongSubject)?;
        Admission::from_authored(definitions, ImmersiveMode::GuestView, camera(&cameras)?)
    })();
}

pub(in crate::immersive_modes) fn validate_first_person(
    mut request: Query<&mut PendingMode<FirstPersonControl>>,
    subjects: Query<
        Has<TransportVehicle>,
        (Or<(With<NavAgent>, With<TransportVehicle>)>, Without<Dead>),
    >,
    definitions: ModeDefinitions,
    cameras: Query<Entity, With<ZooCamera>>,
) {
    let Ok(mut request) = request.single_mut() else {
        return;
    };
    request.result = (|| {
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
}

pub(in crate::immersive_modes) fn validate_search(
    mut request: Query<&mut PendingMode<FossilSearchControl>>,
    sites: Query<(
        &FossilRecoverySite,
        &openzt2_world_state::world_spawn::world_membership_types::WorldMember,
    )>,
    fossils: FossilCollections,
    definitions: ModeDefinitions,
    cameras: Query<Entity, With<ZooCamera>>,
) {
    let Ok(mut request) = request.single_mut() else {
        return;
    };
    request.result = (|| {
        let definitions = definitions.get()?;
        let viable_site = |site: &FossilRecoverySite, world: &WorldMember| {
            definitions
                .find_fossil_set(site.fossil_set_definition_identifier)
                .is_some_and(|set| {
                    set.pieces
                        .iter()
                        .any(|piece| !fossils.contains(world.root, *piece))
                })
        };
        let searchable = request.subject.map_or_else(
            || sites.iter().any(|(site, world)| viable_site(site, world)),
            |subject| {
                sites
                    .get(subject)
                    .is_ok_and(|(site, world)| viable_site(site, world))
            },
        );
        if !searchable {
            return Err(ModeEntryFailure::WrongSubject);
        }
        Admission::from_authored(definitions, ImmersiveMode::FossilSearch, camera(&cameras)?)
    })();
}

pub(in crate::immersive_modes) fn validate_assembly(
    mut request: Query<&mut PendingMode<FossilAssemblyControl>>,
    assemblies: Query<&FossilPlacementSurface, With<FossilAssemblyTable>>,
    definitions: ModeDefinitions,
    cameras: Query<Entity, With<ZooCamera>>,
) {
    let Ok(mut request) = request.single_mut() else {
        return;
    };
    request.result = (|| {
        if !assemblies
            .get(subject(request.subject)?)
            .is_ok_and(|surface| surface.accepts_fossil_pieces)
        {
            return Err(ModeEntryFailure::WrongSubject);
        }
        Admission::from_authored(
            definitions.get()?,
            ImmersiveMode::FossilAssembly,
            camera(&cameras)?,
        )
    })();
}

pub(in crate::immersive_modes) fn validate_photo(
    mut request: Query<&mut PendingMode<PhotoControl>>,
    readbacks: Query<&AlbumPhotoReadbacks>,
    definitions: ModeDefinitions,
    cameras: Query<Entity, With<ZooCamera>>,
) {
    let Ok(mut request) = request.single_mut() else {
        return;
    };
    request.result = (|| {
        let camera = camera(&cameras)?;
        if readbacks
            .get(camera)
            .is_ok_and(|pending| !pending.is_empty())
        {
            return Err(ModeEntryFailure::RuleDenied);
        }
        Admission::from_authored(definitions.get()?, ImmersiveMode::Photo, camera)
    })();
}

pub(in crate::immersive_modes) fn validate_show(
    mut request: Query<&mut PendingMode<ShowEditControl>>,
    stages: Query<&ShowStageRows, With<ShowStage>>,
    performances: Query<(), With<ScheduledShowPerformancePlan>>,
    panels: Query<&InheritedVisibility, With<InfoPanel>>,
    selection: WorldUiSelection,
    definitions: ModeDefinitions,
    cameras: Query<Entity, With<ZooCamera>>,
) {
    let Ok(mut request) = request.single_mut() else {
        return;
    };
    request.result = (|| {
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
}

pub(in crate::immersive_modes) fn validate_staff(
    mut request: Query<&mut PendingMode<StaffControl>>,
    definitions: ModeDefinitions,
    cameras: Query<(Entity, &Transform, &WorldMember), With<ZooCamera>>,
    world_bounds: Query<&WorldTerrainHorizontalBounds>,
    start_surface: SuperStaffAvatarStartSurface,
) {
    let Ok(mut request) = request.single_mut() else {
        return;
    };
    request.result = (|| {
        let definitions = definitions.get()?;
        let (camera, overhead, world) = cameras
            .single()
            .map_err(|_| ModeEntryFailure::MissingTool)?;
        let mut admission =
            Admission::from_authored(definitions, ImmersiveMode::SuperStaff, camera)?;
        let walking = definitions
            .find_camera(admission.definition)
            .ok_or(ModeEntryFailure::MissingTool)?;
        let bounds = world_bounds
            .get(world.root)
            .map_err(|_| ModeEntryFailure::RuleDenied)?;
        let navigation = NavAgent {
            radius_m: f32::from(walking.collision_radius_cm) * 0.01,
            max_speed_mps: walking.pan_speed_mps,
            acceleration_mps2: walking.ground_acceleration_mps2,
            capabilities: NavFlags::STAFF,
        };
        if !navigation.is_valid() {
            return Err(ModeEntryFailure::MissingTool);
        }
        let (heading, _, _) = overhead.rotation.to_euler(EulerRot::YXZ);
        let position =
            start_surface.find_avatar_start_position(overhead.translation, heading, bounds);
        admission.avatar = Some(StaffAvatar {
            transform: Transform::from_translation(position)
                .with_rotation(Quat::from_rotation_y(heading)),
            navigation,
            world: *world,
        });
        admission.lock_subject = true;
        Ok(admission)
    })();
}
