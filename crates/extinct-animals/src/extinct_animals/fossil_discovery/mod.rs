use bevy::{ecs::relationship::RelationshipTarget, platform::collections::HashSet, prelude::*};

use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_data::{AssetId, world_definitions::immersive_mode_policy::ImmersiveModeKind};
use openzt2_interface_state::camera::camera_runtime_state_types::ZooCamera;
use openzt2_interface_state::immersive_modes::immersive_mode_state_types::{
    ActiveImmersiveMode, EquippedInteractionTool, ImmersiveCamera, ImmersiveMode,
};
use openzt2_world_state::world_spawn::persistent_id_types::PersistentIdAllocator;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::extinct_animals::{
    fossil_collection_and_assembly_types::{CollectedFossilPiece, FossilPieceOfWorld, WorldFossilPieces},
    fossil_recovery_types::{DiscoverFossilAtSiteRequest, FossilDiscoveryRandomStream, FossilRecoverySite},
};

use super::fossil_definition_index::FossilDefinitionIndex;

pub(super) fn discover_requested_fossils(
    mut requests: MessageReader<DiscoverFossilAtSiteRequest>,
    modes: Query<(
        Entity,
        &ActiveImmersiveMode,
        &ImmersiveCamera,
        &EquippedInteractionTool,
    )>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    index: Res<FossilDefinitionIndex>,
    cameras: Query<&GlobalTransform, With<ZooCamera>>,
    sites: Query<(&FossilRecoverySite, &GlobalTransform, &WorldMember)>,
    mut commands: Commands,
) {
    let Ok((controller, mode, immersive_camera, tool)) = modes.single() else {
        return;
    };
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    if mode.mode != ImmersiveMode::FossilSearch
        || !definitions
            .find_immersive_mode_policy(tool.policy)
            .is_some_and(|policy| policy.mode == ImmersiveModeKind::FossilSearch)
    {
        return;
    }
    let (Some(policy), Ok(camera)) = (
        definitions.fossil_placement(),
        cameras.get(immersive_camera.0),
    ) else {
        return;
    };

    for request in requests.read() {
        if request.interaction_tool != controller {
            continue;
        }
        let Ok((site, pose, member)) = sites.get(request.fossil_site) else {
            continue;
        };
        let position = pose.translation();
        if !super::fossil_dig_target_geometry::is_fossil_site_within_authored_dig_area(
            camera,
            position,
            policy.dig_distance_m,
        ) {
            continue;
        }
        let Some(set) = definitions.find_fossil_set(site.fossil_set_definition_identifier) else {
            continue;
        };
        let Some(candidates) = set
            .pieces
            .iter()
            .map(|id| {
                let piece = index.piece(*id)?;
                Some((piece.id, u32::from(piece.discovery_weight)))
            })
            .collect::<Option<Vec<_>>>()
        else {
            continue;
        };
        let site_entity = request.fossil_site;
        let root = member.root;
        let pieces_per_location = set.pieces_per_location.max(1);
        commands.queue(move |world: &mut World| {
            commit_fossil_discovery(
                world,
                site_entity,
                root,
                position,
                &candidates,
                pieces_per_location,
            );
        });
    }
}

fn commit_fossil_discovery(
    world: &mut World,
    site: Entity,
    root: Entity,
    position: Vec3,
    candidates: &[(AssetId, u32)],
    pieces_per_location: u16,
) {
    let Some(recovery) = world.get::<FossilRecoverySite>(site).copied() else {
        return;
    };
    if world
        .get::<WorldMember>(site)
        .is_none_or(|member| member.root != root)
    {
        return;
    }
    let discovered: HashSet<AssetId> = world
        .get::<WorldFossilPieces>(root)
        .into_iter()
        .flat_map(RelationshipTarget::iter)
        .filter_map(|entity| {
            world
                .get::<CollectedFossilPiece>(entity)
                .map(|piece| piece.0)
        })
        .collect();
    let mut uncollected = Vec::with_capacity(candidates.len());
    let mut total = 0_u32;
    for &(piece, weight) in candidates {
        if weight == 0 || discovered.contains(&piece) {
            continue;
        }
        let Some(cumulative) = total.checked_add(weight) else {
            return;
        };
        total = cumulative;
        uncollected.push((piece, weight));
    }
    let mut awarded = false;
    for _ in 0..pieces_per_location {
        let Some(draw) = world
            .get_mut::<FossilDiscoveryRandomStream>(site)
            .and_then(|mut random| random.0.range_u32(total))
        else {
            break;
        };
        let mut cumulative = 0;
        let index = uncollected
            .iter()
            .position(|(_, weight)| {
                cumulative += weight;
                draw < cumulative
            })
            .expect("the total contains every remaining candidate");
        let Some(id) = world
            .get_mut::<PersistentIdAllocator>(root)
            .and_then(|mut ids| ids.allocate().ok())
        else {
            break;
        };
        let (piece, weight) = uncollected.remove(index);
        world.spawn((
            CollectedFossilPiece(piece),
            FossilPieceOfWorld(root),
            WorldMember { root },
            id,
            Transform::from_translation(position),
            Visibility::default(),
        ));
        awarded = true;
        total -= weight;
    }
    if total == 0 {
        let retired: Vec<_> = world
            .get::<openzt2_world_state::world_spawn::world_membership_types::WorldMembers>(root)
            .into_iter()
            .flat_map(RelationshipTarget::iter)
            .filter(|entity| {
                world
                    .get::<FossilRecoverySite>(*entity)
                    .is_some_and(|site| *site == recovery)
            })
            .collect();
        for entity in retired {
            world.despawn(entity);
        }
    } else if awarded {
        world.despawn(site);
    }
}
