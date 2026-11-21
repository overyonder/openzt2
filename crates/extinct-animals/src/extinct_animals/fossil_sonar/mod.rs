use bevy::{ecs::entity::EntityLocation, prelude::*};

use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_interface_state::camera::camera_runtime_state_types::ZooCamera;
use openzt2_interface_state::immersive_modes::immersive_mode_state_types::{
    ActiveImmersiveMode, ImmersiveCamera, ImmersiveMode, ImmersiveSubject,
};
use openzt2_zoo_state::animal_behavior::interaction_container_membership::{
    InteractionContainerMemberships, InteractionContainerSlotMembers, InteractionContainerSlots,
};
use openzt2_zoo_state::extinct_animals::fossil_recovery_types::{
    ActivateFossilSiteMarkersRequest, ActiveFossilSonarArtifactTracking, FossilRecoverySite,
};

use super::fossil_sonar_scoring::select_highest_scoring_fossil_sonar_candidate;

#[derive(Component)]
pub(super) struct ActiveFossilSonar;

pub(super) fn activate_sonar_markers_for_search_session(
    mut commands: Commands,
    mut requests: MessageReader<ActivateFossilSiteMarkersRequest>,
    sessions: Query<(Entity, &ActiveImmersiveMode), Without<ActiveFossilSonar>>,
) {
    if requests.read().count() == 0 {
        return;
    }
    if let Some((session, _)) = sessions
        .iter()
        .find(|(_, mode)| mode.mode == ImmersiveMode::FossilSearch)
    {
        commands.entity(session).insert(ActiveFossilSonar);
    }
}

pub(super) fn update_fossil_sonar(
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    mut sessions: Query<(
        Entity,
        Ref<ActiveFossilSonar>,
        Ref<ImmersiveCamera>,
        Option<Ref<ImmersiveSubject>>,
        &mut ActiveFossilSonarArtifactTracking,
    )>,
    cameras: Query<Ref<GlobalTransform>, With<ZooCamera>>,
    artifacts: Query<(Entity, &GlobalTransform, EntityLocation), With<FossilRecoverySite>>,
    changed_artifacts: Query<
        (),
        (
            With<FossilRecoverySite>,
            Or<(Added<FossilRecoverySite>, Changed<GlobalTransform>)>,
        ),
    >,
    changed_membership: Query<
        (),
        Or<(
            Changed<InteractionContainerSlots>,
            Changed<InteractionContainerSlotMembers>,
        )>,
    >,
    mut removed_sites: RemovedComponents<FossilRecoverySite>,
    mut removed_subjects: RemovedComponents<ImmersiveSubject>,
    mut removed_membership: RemovedComponents<InteractionContainerSlotMembers>,
    memberships: InteractionContainerMemberships,
) {
    let removed_artifact = removed_sites.read().count() != 0;
    let removed_member = removed_membership.read().count() != 0;
    let removed_subject = removed_subjects.read().count() != 0;
    let search_changed = active_definitions.is_changed()
        || definitions.is_changed()
        || removed_artifact
        || removed_member
        || removed_subject
        || !changed_artifacts.is_empty()
        || !changed_membership.is_empty();
    let policy = active_definitions
        .get(&definitions)
        .and_then(|definitions| definitions.fossil_placement());
    for (session, active, camera, subject, mut tracking) in &mut sessions {
        let Ok(view) = cameras.get(camera.0) else {
            tracking.set_if_neq(ActiveFossilSonarArtifactTracking::default());
            continue;
        };
        if !search_changed
            && !active.is_added()
            && !camera.is_changed()
            && !view.is_changed()
            && !subject.as_ref().is_some_and(|subject| subject.is_changed())
        {
            continue;
        }
        let retained = memberships
            .members_of_container(session)
            .chain(
                subject
                    .iter()
                    .flat_map(|subject| memberships.members_of_container(subject.0)),
            )
            .filter_map(|member| artifacts.get(member).ok())
            .min_by_key(|(_, _, location)| (location.table_id.as_u32(), location.table_row.index()))
            .map(|(member, _, _)| member);
        let result = policy.and_then(|policy| {
            select_highest_scoring_fossil_sonar_candidate(
                view.translation(),
                view.forward().as_vec3(),
                retained,
                policy.minimum_sonar_distance_squared,
                policy.maximum_sonar_distance_squared,
                policy.minimum_sonar_view_dot,
                artifacts
                    .iter()
                    .map(|(entity, pose, _)| (entity, pose.translation())),
            )
        });
        tracking.set_if_neq(result.map_or_else(
            ActiveFossilSonarArtifactTracking::default,
            |candidate| ActiveFossilSonarArtifactTracking {
                selected_artifact: Some(candidate.entity),
                distance_squared: candidate.distance_squared,
                sonar_strength: candidate.score,
            },
        ));
    }
}
