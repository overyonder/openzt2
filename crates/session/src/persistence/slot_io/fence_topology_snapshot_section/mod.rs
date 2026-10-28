use std::collections::{BTreeMap, BTreeSet};

use bevy::{ecs::system::SystemParam, prelude::*};
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use openzt2_game_data::AssetId;
use openzt2_world_state::world_spawn::{
    persistent_id_types::{PersistentId, PersistentIdAllocator},
    world_membership_types::WorldMember,
};
use openzt2_zoo_state::{
    aquatic::aquatic_simulation_types::TankWallTopHeight,
    topology::{
        gate_operation_types::{Gate, GateAutoClose, GateMechanism},
        topology_edit_types::{TopologyChanged, TopologySnapshot},
        topology_graph_types::{EdgeKey, FenceEdge, TopologyGrid, TopologyIndex, TopologyNode},
        topology_grid_geometry::{calculate_topology_edge_cell_bounds, calculate_topology_edge_world_transform, merge_topology_changed_cell_bounds},
    },
};

use super::super::{
    persistence_failure_types::WorldSnapshotPersistenceFailure as Failure,
    snapshot_container_types::WorldSnapshotSectionDirectoryEntry,
};

const HEADER: [u8; 4] = [1, 0, 0, 0];

#[derive(SystemParam)]
pub(super) struct FenceTopologySnapshotEntities<'w, 's> {
    nodes: Query<
        'w,
        's,
        (
            Entity,
            &'static PersistentId,
            &'static TopologyNode,
            &'static WorldMember,
        ),
    >,
    fences: Query<
        'w,
        's,
        (
            Entity,
            &'static PersistentId,
            &'static FenceEdge,
            Option<&'static Gate>,
            Option<&'static TankWallTopHeight>,
            &'static WorldMember,
        ),
    >,
}

pub(super) fn capture_fence_topology_records(
    entities: &FenceTopologySnapshotEntities,
) -> Result<Vec<TopologySnapshot>, Failure> {
    let nodes: BTreeMap<_, _> = entities
        .nodes
        .iter()
        .map(|(entity, id, node, member)| (entity, (*id, *node, *member)))
        .collect();
    let mut identifiers = BTreeMap::new();
    for (entity, id) in entities
        .nodes
        .iter()
        .map(|(entity, id, ..)| (entity, id))
        .chain(entities.fences.iter().map(|(entity, id, ..)| (entity, id)))
    {
        if identifiers
            .insert(id.0, entity)
            .is_some_and(|prior| prior != entity)
        {
            return Err(Failure::DuplicatePersistentEntityIdentifier);
        }
    }
    let mut records: BTreeMap<_, _> = nodes
        .values()
        .map(|(id, node, _)| {
            (
                id.0,
                TopologySnapshot::Node {
                    id: *id,
                    cell: node.cell,
                },
            )
        })
        .collect();
    for (_, id, edge, gate, top, member) in &entities.fences {
        for entity in [edge.a, edge.b] {
            let (node_id, node, node_member) = nodes
                .get(&entity)
                .ok_or(Failure::BrokenPersistentEntityReference)?;
            if node_member.root != member.root {
                return Err(Failure::BrokenPersistentEntityReference);
            }
            records.insert(
                node_id.0,
                TopologySnapshot::Node {
                    id: *node_id,
                    cell: node.cell,
                },
            );
        }
        if records
            .insert(
                id.0,
                TopologySnapshot::Fence {
                    id: *id,
                    definition: edge.definition,
                    a: nodes[&edge.a].0,
                    b: nodes[&edge.b].0,
                    gate: gate.copied(),
                    wall_top: top.copied(),
                },
            )
            .is_some()
        {
            return Err(Failure::DuplicatePersistentEntityIdentifier);
        }
    }
    let records: Vec<_> = records.into_values().collect();
    validate_fence_topology_records(&records)?;
    Ok(records)
}

pub(super) fn encode_fence_topology_records(
    records: &[TopologySnapshot],
) -> Result<Vec<u8>, Failure> {
    validate_fence_topology_records(records)?;
    let mut bytes = HEADER.to_vec();
    for record in records {
        match record {
            TopologySnapshot::Node { id, cell } => {
                bytes.push(0);
                bytes.extend_from_slice(&id.0.to_le_bytes());
                for coordinate in cell.to_array() {
                    bytes.extend_from_slice(&coordinate.to_le_bytes());
                }
            }
            TopologySnapshot::Fence {
                id,
                definition,
                a,
                b,
                gate,
                wall_top,
            } => {
                bytes.push(1);
                bytes.extend_from_slice(&id.0.to_le_bytes());
                bytes.extend_from_slice(&definition.0);
                bytes.extend_from_slice(&a.0.to_le_bytes());
                bytes.extend_from_slice(&b.0.to_le_bytes());
                bytes.push(gate.map_or(0, |gate| {
                    1 | (u8::from(gate.open) << 1) | (u8::from(gate.locked) << 2)
                }));
                bytes.push(u8::from(wall_top.is_some()));
                bytes.extend_from_slice(&wall_top.map_or(0.0, |top| top.0).to_le_bytes());
            }
            _ => return Err(Failure::CorruptSnapshotSection),
        }
    }
    Ok(bytes)
}

/// An empty topology section in older saves leaves baseline fences intact.
pub(super) fn decode_fence_topology_records(
    container: &[u8],
    section: WorldSnapshotSectionDirectoryEntry,
) -> Result<Option<Vec<TopologySnapshot>>, Failure> {
    let bytes = &container[section.payload_byte_range_within_container(container.len())?];
    if bytes.is_empty() && section.encoded_record_count == 0 {
        return Ok(None);
    }
    if !bytes.starts_with(&HEADER)
        || section.encoded_record_count as usize > bytes.len().saturating_sub(4) / 21
    {
        return Err(Failure::CorruptSnapshotSection);
    }
    let mut cursor = 4;
    let mut records = Vec::with_capacity(section.encoded_record_count as usize);
    for _ in 0..section.encoded_record_count {
        let tag = *bytes.get(cursor).ok_or(Failure::CorruptSnapshotSection)?;
        let size = match tag {
            0 => 21,
            1 => 47,
            _ => return Err(Failure::CorruptSnapshotSection),
        };
        let end = cursor.checked_add(size).ok_or(Failure::CapacityExceeded)?;
        let record = bytes
            .get(cursor..end)
            .ok_or(Failure::CorruptSnapshotSection)?;
        let id_at = |offset| -> Result<PersistentId, Failure> {
            Ok(PersistentId(u64::from_le_bytes(
                record[offset..offset + 8]
                    .try_into()
                    .map_err(|_| Failure::CorruptSnapshotSection)?,
            )))
        };
        let id = id_at(1)?;
        if id.0 == 0
            || id.0 == u64::MAX
            || records
                .last()
                .is_some_and(|previous: &TopologySnapshot| previous.id().0 >= id.0)
        {
            return Err(Failure::CorruptSnapshotSection);
        }
        let value = if tag == 0 {
            let coordinate = |offset| -> Result<i32, Failure> {
                Ok(i32::from_le_bytes(
                    record[offset..offset + 4]
                        .try_into()
                        .map_err(|_| Failure::CorruptSnapshotSection)?,
                ))
            };
            TopologySnapshot::Node {
                id,
                cell: IVec3::new(coordinate(9)?, coordinate(13)?, coordinate(17)?),
            }
        } else {
            let a = id_at(25)?;
            let b = id_at(33)?;
            let flags = record[41];
            let height = f32::from_le_bytes(
                record[43..47]
                    .try_into()
                    .map_err(|_| Failure::CorruptSnapshotSection)?,
            );
            if a == b
                || flags > 7
                || flags & 1 == 0 && flags != 0
                || record[42] > 1
                || !height.is_finite()
            {
                return Err(Failure::CorruptSnapshotSection);
            }
            TopologySnapshot::Fence {
                id,
                definition: AssetId(
                    record[9..25]
                        .try_into()
                        .map_err(|_| Failure::CorruptSnapshotSection)?,
                ),
                a,
                b,
                gate: (flags & 1 != 0).then_some(Gate {
                    open: flags & 2 != 0,
                    locked: flags & 4 != 0,
                }),
                wall_top: (record[42] != 0).then_some(TankWallTopHeight(height)),
            }
        };
        records.push(value);
        cursor = end;
    }
    if cursor != bytes.len() {
        return Err(Failure::CorruptSnapshotSection);
    }
    validate_fence_topology_records(&records)?;
    Ok(Some(records))
}

fn validate_fence_topology_records(records: &[TopologySnapshot]) -> Result<(), Failure> {
    for (index, record) in records.iter().enumerate() {
        if record.id().0 == 0
            || record.id().0 == u64::MAX
            || index != 0 && records[index - 1].id().0 >= record.id().0
        {
            return Err(Failure::CorruptSnapshotSection);
        }
        match record {
            TopologySnapshot::Fence { wall_top, .. } => {
                if wall_top.is_some_and(|top| !top.0.is_finite()) {
                    return Err(Failure::CorruptSnapshotSection);
                }
            }
            TopologySnapshot::Node { .. } => {}
            _ => return Err(Failure::CorruptSnapshotSection),
        }
    }
    let nodes: BTreeMap<_, _> = records
        .iter()
        .filter_map(|record| {
            if let TopologySnapshot::Node { id, cell } = record {
                Some((id.0, *cell))
            } else {
                None
            }
        })
        .collect();
    let mut cells = BTreeSet::new();
    let mut edges = BTreeSet::new();
    for cell in nodes.values() {
        if !cells.insert(cell.to_array()) {
            return Err(Failure::CorruptSnapshotSection);
        }
    }
    for record in records {
        if let TopologySnapshot::Fence { a, b, .. } = record {
            let (a, b) = (
                nodes
                    .get(&a.0)
                    .ok_or(Failure::BrokenPersistentEntityReference)?,
                nodes
                    .get(&b.0)
                    .ok_or(Failure::BrokenPersistentEntityReference)?,
            );
            if EdgeKey::new(*a, *b).is_none() {
                return Err(Failure::CorruptSnapshotSection);
            }
            let key = if a.to_array() < b.to_array() {
                (a.to_array(), b.to_array())
            } else {
                (b.to_array(), a.to_array())
            };
            if !edges.insert(key) {
                return Err(Failure::CorruptSnapshotSection);
            }
        }
    }
    Ok(())
}

#[derive(SystemParam)]
pub(super) struct FenceTopologySnapshotApplication<'w, 's> {
    entities: FenceTopologySnapshotEntities<'w, 's>,
    persistent: Query<'w, 's, (Entity, &'static PersistentId)>,
    index: ResMut<'w, TopologyIndex>,
    grid: Res<'w, TopologyGrid>,
    changed: MessageWriter<'w, TopologyChanged>,
}

impl FenceTopologySnapshotApplication<'_, '_> {
    pub(super) fn baseline_entities(&self, root: Entity) -> impl Iterator<Item = Entity> + '_ {
        self.entities
            .nodes
            .iter()
            .filter(move |(_, _, _, member)| member.root == root)
            .map(|(entity, ..)| entity)
            .chain(
                self.entities
                    .fences
                    .iter()
                    .filter(move |(_, _, _, _, _, member)| member.root == root)
                    .map(|(entity, ..)| entity),
            )
    }
}

pub(super) fn apply_fence_topology_records(
    commands: &mut Commands,
    records: &[TopologySnapshot],
    application: &mut FenceTopologySnapshotApplication,
    definitions: WorldDefinitionsView<'_>,
    allocator: &mut PersistentIdAllocator,
    replaced_entities: &[Entity],
    root: Entity,
) -> Result<BTreeMap<u64, Entity>, Failure> {
    validate_fence_topology_records(records)?;
    let mut existing = BTreeMap::new();
    for (entity, id) in &application.persistent {
        if replaced_entities.contains(&entity) {
            continue;
        }
        if existing.insert(id.0, entity).is_some() {
            return Err(Failure::DuplicatePersistentEntityIdentifier);
        }
    }
    let baseline_edges = application
        .entities
        .fences
        .iter()
        .filter(|(_, _, _, _, _, member)| member.root == root)
        .map(|(entity, id, edge, ..)| {
            let (_, _, a, _) = application
                .entities
                .nodes
                .get(edge.a)
                .map_err(|_| Failure::BrokenPersistentEntityReference)?;
            let (_, _, b, _) = application
                .entities
                .nodes
                .get(edge.b)
                .map_err(|_| Failure::BrokenPersistentEntityReference)?;
            let key = EdgeKey::new(a.cell, b.cell).ok_or(Failure::CorruptSnapshotSection)?;
            Ok((
                entity,
                *id,
                key,
                calculate_topology_edge_cell_bounds(a.cell, b.cell),
            ))
        })
        .collect::<Result<Vec<_>, Failure>>()?;
    for record in records {
        if let Some(entity) = existing.get(&record.id().0) {
            let same_kind = match record {
                TopologySnapshot::Node { .. } => application
                    .entities
                    .nodes
                    .get(*entity)
                    .is_ok_and(|(_, _, _, member)| member.root == root),
                TopologySnapshot::Fence { .. } => application
                    .entities
                    .fences
                    .get(*entity)
                    .is_ok_and(|(_, _, _, _, _, member)| member.root == root),
                _ => false,
            };
            if !same_kind {
                return Err(Failure::DuplicatePersistentEntityIdentifier);
            }
        }
        if let TopologySnapshot::Fence {
            definition, gate, ..
        } = record
        {
            if definitions.find_fence(*definition).is_none()
                || gate.is_some() && definitions.find_gate_fence(*definition).is_none()
            {
                return Err(Failure::UnknownAssetIdentifier);
            }
        }
        allocator
            .reserve_imported(root, record.id())
            .map_err(|_| Failure::CapacityExceeded)?;
    }
    let ids: BTreeSet<_> = records.iter().map(|record| record.id().0).collect();
    let mut bounds = None;
    for (entity, id, key, edge_bounds) in baseline_edges {
        application.index.edges.remove(&key);
        bounds = merge_topology_changed_cell_bounds(bounds, edge_bounds);
        if !ids.contains(&id.0) {
            commands.entity(entity).despawn();
        }
    }
    for (entity, id, node, member) in &application.entities.nodes {
        if member.root == root {
            application.index.nodes.remove(&node.cell);
            if !ids.contains(&id.0) {
                commands.entity(entity).despawn();
            }
        }
    }
    let mut nodes = BTreeMap::new();
    let mut restored = BTreeMap::new();
    for record in records {
        let TopologySnapshot::Node { id, cell } = record else {
            continue;
        };
        let entity = existing
            .get(&id.0)
            .copied()
            .unwrap_or_else(|| commands.spawn_empty().id());
        commands
            .entity(entity)
            .insert((*id, TopologyNode { cell: *cell }, WorldMember { root }));
        application.index.nodes.insert(*cell, entity);
        nodes.insert(id.0, (entity, *cell));
        restored.insert(id.0, entity);
    }
    for record in records {
        let TopologySnapshot::Fence {
            id,
            definition,
            a,
            b,
            gate,
            wall_top,
        } = record
        else {
            continue;
        };
        let (a, b) = (nodes[&a.0], nodes[&b.0]);
        let entity = existing
            .get(&id.0)
            .copied()
            .unwrap_or_else(|| commands.spawn_empty().id());
        commands.entity(entity).insert((
            *id,
            FenceEdge {
                definition: *definition,
                a: a.0,
                b: b.0,
            },
            WorldMember { root },
            calculate_topology_edge_world_transform(a.1, b.1, *application.grid),
        ));
        commands
            .entity(entity)
            .remove::<(Gate, TankWallTopHeight, GateMechanism, GateAutoClose)>();
        if let Some(gate) = gate {
            commands.entity(entity).insert(*gate);
        }
        if let Some(top) = wall_top {
            commands.entity(entity).insert(*top);
        }
        application.index.edges.insert(
            EdgeKey::new(a.1, b.1).expect("validated fence endpoints"),
            entity,
        );
        restored.insert(id.0, entity);
        bounds = merge_topology_changed_cell_bounds(
            bounds,
            calculate_topology_edge_cell_bounds(a.1, b.1),
        );
    }
    if let Some(bounds) = bounds {
        application.changed.write(TopologyChanged {
            transaction: None,
            bounds,
        });
    }
    Ok(restored)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fence_snapshot_round_trip_preserves_gate_state_and_wall_top() {
        let records = vec![
            TopologySnapshot::Node {
                id: PersistentId(1),
                cell: IVec3::ZERO,
            },
            TopologySnapshot::Node {
                id: PersistentId(2),
                cell: IVec3::X,
            },
            TopologySnapshot::Fence {
                id: PersistentId(3),
                definition: AssetId::from_key("wall"),
                a: PersistentId(1),
                b: PersistentId(2),
                gate: Some(Gate {
                    open: true,
                    locked: false,
                }),
                wall_top: Some(TankWallTopHeight(6.25)),
            },
        ];
        let payload = encode_fence_topology_records(&records).unwrap();
        let section = WorldSnapshotSectionDirectoryEntry {
            encoded_record_count: records.len() as u32,
            payload_start_byte_offset: 0,
            payload_byte_count: payload.len() as u64,
        };
        let restored = decode_fence_topology_records(&payload, section)
            .unwrap()
            .unwrap();
        assert_eq!(restored, records);
        assert_eq!(encode_fence_topology_records(&restored).unwrap(), payload);
    }
}
