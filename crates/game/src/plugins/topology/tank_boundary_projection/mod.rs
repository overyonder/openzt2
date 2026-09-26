//! Validates tank boundaries and links their habitat entities to fence segments.

use bevy::prelude::*;
use openzt2_game_data::AssetId;

use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::aquatic::aquatic_simulation_types::Tank;
use crate::plugins::habitat::habitat_types::HabitatRegion;
use crate::plugins::world_spawn::world_membership_types::WorldMember;

use super::{
    tank_boundary_types::{
        TankBoundaryMember, TankBoundaryMemberships, TankBoundaryOf, TankBoundarySegment,
    },
    topology_graph_types::{FenceEdge, TopologyNode},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TankBoundaryFailure {
    InvalidSegment,
    WrongWallDefinition,
    InvalidGeometry,
}

/// Validates and establishes exact tank-wall membership after habitat changes a
/// tank habitat region.
///
/// Every boundary edge is checked before commands are queued, so failure
/// cannot leave a partially reassigned tank. The system copies neither cells
/// nor endpoints: consumers join `TankBoundaryOf -> FenceEdge -> TopologyNode`.
#[allow(clippy::type_complexity)]
pub(super) fn synchronize_tank_boundary_relationships_after_habitat_region_changes(
    mut commands: Commands,
    changed_tanks: Query<
        (Entity, &Tank, &HabitatRegion),
        Or<(Added<Tank>, Changed<HabitatRegion>)>,
    >,
    fences: Query<(&FenceEdge, &WorldMember)>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    nodes: Query<&TopologyNode>,
    members: Query<&WorldMember>,
    mut removed_tanks: RemovedComponents<Tank>,
    existing_memberships: Query<(Entity, &TankBoundaryOf), With<TankBoundaryMember>>,
) {
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    for removed_tank in removed_tanks.read() {
        for (membership_entity, boundary) in &existing_memberships {
            if boundary.0 == removed_tank {
                commands.entity(membership_entity).despawn();
            }
        }
        if let Ok(mut entity) = commands.get_entity(removed_tank) {
            entity.remove::<TankBoundaryMemberships>();
        }
    }

    for (tank_entity, tank, region) in &changed_tanks {
        if region.boundary_fence_entities.is_empty() {
            continue;
        }
        let Ok(tank_member) = members.get(tank_entity) else {
            continue;
        };
        let Some(wall_definition) = definitions
            .find_tank(tank.definition)
            .map(|definition| AssetId(definition.wall.0))
            .filter(|definition| *definition != AssetId::default())
        else {
            continue;
        };

        let validation = region
            .boundary_fence_entities
            .iter()
            .copied()
            .try_for_each(|segment| {
                let (edge, member) = fences
                    .get(segment)
                    .map_err(|_| TankBoundaryFailure::InvalidSegment)?;
                if member.root != tank_member.root {
                    return Err(TankBoundaryFailure::InvalidSegment);
                }
                if edge.definition != wall_definition {
                    return Err(TankBoundaryFailure::WrongWallDefinition);
                }
                nodes
                    .get(edge.a)
                    .and_then(|_| nodes.get(edge.b))
                    .map_err(|_| TankBoundaryFailure::InvalidGeometry)?;
                Ok(())
            });
        if validation.is_err() {
            continue;
        }

        for (membership_entity, boundary) in &existing_memberships {
            if boundary.0 == tank_entity {
                commands.entity(membership_entity).despawn();
            }
        }
        for segment in region.boundary_fence_entities.iter().copied() {
            commands.spawn((
                TankBoundaryMember,
                TankBoundaryOf(tank_entity),
                TankBoundarySegment(segment),
                *tank_member,
            ));
        }
    }
}

/// Removes membership entities when their tank or fence segment disappears.
pub(super) fn remove_tank_boundary_relationship_entities_with_missing_endpoints(
    mut commands: Commands,
    memberships: Query<
        (
            Entity,
            Option<&TankBoundaryOf>,
            Option<&TankBoundarySegment>,
        ),
        With<TankBoundaryMember>,
    >,
    tanks: Query<(), With<Tank>>,
    fences: Query<(), With<FenceEdge>>,
) {
    for (relationship_entity, tank, segment) in &memberships {
        if tank.is_none_or(|tank| tanks.get(tank.0).is_err())
            || segment.is_none_or(|segment| fences.get(segment.0).is_err())
        {
            commands.entity(relationship_entity).despawn();
        }
    }
}
