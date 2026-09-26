use bevy::prelude::*;

use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use crate::plugins::immersive_modes::immersive_mode_state_types::ActiveImmersiveMode;
use crate::plugins::immersive_modes::immersive_mode_state_types::EquippedInteractionTool;
use crate::plugins::immersive_modes::immersive_mode_state_types::ImmersiveMode;
use crate::plugins::world_spawn::persistent_id_types::PersistentIdAllocator;
use crate::plugins::world_spawn::world_membership_types::WorldMember;

use super::{
    fossil_collection_and_assembly_types::{CollectedFossilPiece, CollectedFossilPieceInventory},
    fossil_collection_rules::{
        are_all_fossil_set_pieces_collected,
        calculate_total_uncollected_fossil_piece_discovery_weight,
        select_weighted_uncollected_fossil_piece,
    },
    fossil_recovery_types::{
        ActivateFossilSiteMarkersRequest, ActiveFossilSearchMarker,
        ActiveFossilSonarArtifactTracking, DiscoverFossilAtSiteRequest,
        FossilDiscoveryRandomStream, FossilRecoverySite,
    },
    fossil_sonar_scoring::select_highest_scoring_fossil_sonar_candidate,
};

pub(super) fn update_active_fossil_search_sonar_tracking(
    mut commands: Commands,
    occupancy: Res<crate::plugins::animal_behavior::interaction_container_occupancy::InteractionContainerOccupancy>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    controllers: Query<(Entity, &ActiveImmersiveMode)>,
    cameras: Query<
        &GlobalTransform,
        With<crate::plugins::camera::camera_runtime_state_types::ZooCamera>,
    >,
    artifacts: Query<(Entity, &GlobalTransform), With<ActiveFossilSearchMarker>>,
) {
    let Some(policy) = active_definitions
        .get(&definitions)
        .and_then(WorldDefinitionsView::fossil_placement)
    else {
        return;
    };
    for (controller, active) in &controllers {
        if active.mode != ImmersiveMode::FossilSearch {
            continue;
        }
        let Ok(camera) = cameras.get(active.camera) else {
            continue;
        };
        let retained = artifacts.iter().find_map(|(entity, _)| {
            occupancy
                .container_for_member(entity)
                .filter(|(holder, _)| *holder == controller || Some(*holder) == active.subject)
                .map(|_| entity)
        });
        let result = select_highest_scoring_fossil_sonar_candidate(
            camera.translation(),
            camera.forward().as_vec3(),
            retained,
            f32::from(policy.minimum_sonar_distance_squared),
            f32::from(policy.maximum_sonar_distance_squared),
            f32::from(policy.minimum_sonar_view_dot),
            artifacts
                .iter()
                .map(|(entity, transform)| (entity, transform.translation())),
        );
        commands.entity(controller).insert(result.map_or_else(
            ActiveFossilSonarArtifactTracking::default,
            |candidate| ActiveFossilSonarArtifactTracking {
                selected_artifact: Some(candidate.entity),
                distance_squared: candidate.distance_squared,
                sonar_strength: candidate.score,
            },
        ));
    }
}

pub(super) fn activate_all_fossil_site_markers_when_requested(
    mut commands: Commands,
    mut requests: MessageReader<ActivateFossilSiteMarkersRequest>,
    sites: Query<Entity, (With<FossilRecoverySite>, Without<ActiveFossilSearchMarker>)>,
) {
    if requests.read().next().is_some() {
        for site in &sites {
            commands.entity(site).insert(ActiveFossilSearchMarker);
        }
    }
}

pub(super) fn discover_weighted_fossil_piece_at_requested_site(
    mut requests: MessageReader<DiscoverFossilAtSiteRequest>,
    mode: Query<(Entity, &ActiveImmersiveMode)>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    mut collection: ResMut<CollectedFossilPieceInventory>,
    mut ids: ResMut<PersistentIdAllocator>,
    tools: Query<&EquippedInteractionTool>,
    cameras: Query<
        &GlobalTransform,
        With<crate::plugins::camera::camera_runtime_state_types::ZooCamera>,
    >,
    mut sites: Query<(
        Entity,
        &mut FossilRecoverySite,
        &mut FossilDiscoveryRandomStream,
        &GlobalTransform,
        &WorldMember,
    )>,
    mut commands: Commands,
) {
    let Ok((controller, mode)) = mode.single() else {
        return;
    };
    if mode.mode != ImmersiveMode::FossilSearch {
        return;
    }
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    let (Some(placement_policy), Ok(camera)) =
        (definitions.fossil_placement(), cameras.get(mode.camera))
    else {
        return;
    };

    for request in requests.read() {
        if request.interaction_tool != controller {
            continue;
        }
        let Ok(tool) = tools.get(request.interaction_tool) else {
            continue;
        };
        let Some(tool_definition) = definitions.find_immersive_mode_policy(tool.policy) else {
            continue;
        };
        if !matches!(
            tool_definition.mode,
            openzt2_game_data::world_definitions::immersive_mode_policy::ImmersiveModeKind::FossilSearch
        ) {
            continue;
        }
        let Ok((site_entity, mut site, mut rng, site_transform, world)) =
            sites.get_mut(request.fossil_site)
        else {
            continue;
        };
        if site_entity != request.fossil_site
            || site.all_pieces_collected
            || !super::fossil_dig_target_geometry::is_fossil_site_within_authored_dig_area(
                camera,
                site_transform.translation(),
                placement_policy.dig_distance_m,
            )
        {
            continue;
        }
        let Some(set) = definitions.find_fossil_set(site.fossil_set_definition_identifier) else {
            continue;
        };
        let choice = calculate_total_uncollected_fossil_piece_discovery_weight(
            definitions,
            &set.pieces,
            &collection,
        )
        .and_then(|total| rng.0.range_u32(total))
        .and_then(|target| {
            select_weighted_uncollected_fossil_piece(definitions, &set.pieces, &collection, target)
        });
        let Some(selected_fossil_piece) = choice else {
            site.all_pieces_collected = true;
            continue;
        };
        let piece_index = selected_fossil_piece.global_piece_index;
        let piece = selected_fossil_piece.definition;
        let Ok(piece_index_u16) = u16::try_from(piece_index) else {
            continue;
        };
        let Ok(id) = ids.allocate(world.root) else {
            continue;
        };
        if collection.mark_global_fossil_piece_collected(piece_index) != Some(true) {
            continue;
        }
        commands.spawn((
            CollectedFossilPiece {
                fossil_set_identifier: piece.set,
                global_piece_index: piece_index_u16,
            },
            *world,
            id,
            Transform::from_translation(site_transform.translation()),
            Visibility::default(),
        ));
        site.all_pieces_collected =
            are_all_fossil_set_pieces_collected(definitions, &set.pieces, &collection);
    }
}
