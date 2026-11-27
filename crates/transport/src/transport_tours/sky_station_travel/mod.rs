use bevy::{ecs::entity::EntityHashSet, platform::collections::HashMap, prelude::*};
use openzt2_game_assets::world_definitions::{
    world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
    world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
};
use openzt2_game_data::{
    world_definitions::transportation_and_tours::TransportationTrackKind, AssetId,
};
use openzt2_zoo_state::transport_tours::transport_topology_types::TransportStation;

#[derive(Debug, PartialEq)]
pub(super) struct StationCable {
    points: Box<[Vec3]>,
    distance_at_node: Box<[f32]>,
    stops: [Option<f32>; 2],
}

impl StationCable {
    pub(super) fn length(&self) -> f32 {
        self.distance_at_node.last().copied().unwrap_or_default()
    }

    pub(super) fn end_points(&self) -> [Vec3; 2] {
        [self.points[0], self.points[self.points.len() - 1]]
    }

    pub(super) fn stop(&self, reverse: bool) -> Option<f32> {
        self.stops[usize::from(reverse)]
    }

    pub(super) fn pose(&self, distance: f32) -> (Vec3, Vec3) {
        let index = self
            .distance_at_node
            .partition_point(|node| *node <= distance)
            .saturating_sub(1)
            .min(self.points.len() - 2);
        let [a, b] = [self.points[index], self.points[index + 1]];
        let start = self.distance_at_node[index];
        let span = self.distance_at_node[index + 1] - start;
        (
            a.lerp(b, ((distance - start) / span).clamp(0.0, 1.0)),
            b - a,
        )
    }
}

#[derive(Component, Debug, Default, PartialEq)]
pub(super) struct SkyStationTravel([Option<StationCable>; 2]);

impl SkyStationTravel {
    pub(super) fn between(&self, incoming: Vec3, outgoing: Vec3) -> Option<(&StationCable, bool)> {
        self.0
            .iter()
            .flatten()
            .flat_map(|path| {
                let [first, last] = path.end_points();
                [
                    (
                        (incoming.distance_squared(first) + outgoing.distance_squared(last)),
                        path,
                        false,
                    ),
                    (
                        (incoming.distance_squared(last) + outgoing.distance_squared(first)),
                        path,
                        true,
                    ),
                ]
            })
            .min_by(|first, second| first.0.total_cmp(&second.0))
            .map(|(_, path, reverse)| (path, reverse))
    }
}

fn collect_station_cable(
    nodes: &HashMap<AssetId, Vec3>,
    prefix: &str,
    stop_nodes: &[Option<AssetId>],
) -> Option<StationCable> {
    let mut points = Vec::new();
    let mut lengths = Vec::new();
    let mut stops = [None; 2];
    let mut length = 0.0;
    let mut horizontal_length = 0.0;
    for number in 1.. {
        let key = AssetId::from_key(&format!("{prefix}_{number:02}"));
        let Some(&point) = nodes.get(&key) else {
            break;
        };
        if let Some(&previous) = points.last() {
            length += point.distance(previous);
            horizontal_length += point.xz().distance(previous.xz());
        }
        for (index, stop_node) in stop_nodes.iter().enumerate() {
            if *stop_node == Some(key) {
                stops[index] = Some(horizontal_length);
            }
        }
        if points.last() != Some(&point) {
            points.push(point);
            lengths.push(length);
        }
    }
    (points.len() >= 2).then(|| StationCable {
        points: points.into_boxed_slice(),
        distance_at_node: lengths.into_boxed_slice(),
        stops: stops.map(|stop| {
            stop.filter(|_| horizontal_length > 0.0)
                .map(|stop| stop * (length / horizontal_length))
        }),
    })
}

#[derive(Message)]
pub(super) struct DiscardedStationCableNode(Entity);

pub(super) fn notify_discarded_station_cable_node(
    discarded: On<Discard<(Name, ChildOf)>>,
    parents: Query<&ChildOf>,
    names: Query<&Name>,
    stations: Query<(), With<TransportStation>>,
    mut changes: MessageWriter<DiscardedStationCableNode>,
) {
    let Ok(name) = names.get(discarded.entity) else {
        return;
    };
    if !is_cable_node(&name.as_str().to_ascii_lowercase()) {
        return;
    }
    if let Some(station) = parents
        .iter_ancestors::<ChildOf>(discarded.entity)
        .find(|&entity| stations.contains(entity))
    {
        changes.write(DiscardedStationCableNode(station));
    }
}

fn is_cable_node(name: &str) -> bool {
    name.starts_with("travelpoint_") || name.starts_with("returntravelpoint_")
}

pub(super) fn project_sky_station_travel(
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active: Res<WorldDefinitions>,
    stations: Query<(Entity, &TransportStation)>,
    changed_stations: Query<Entity, Changed<TransportStation>>,
    changed_nodes: Query<
        Entity,
        (
            With<Name>,
            Or<(Changed<GlobalTransform>, Changed<Name>, Changed<ChildOf>)>,
        ),
    >,
    mut discarded: MessageReader<DiscardedStationCableNode>,
    parents: Query<&ChildOf>,
    children: Query<&Children>,
    nodes: Query<(&Name, &GlobalTransform)>,
    mut travel: Query<&mut SkyStationTravel>,
    mut commands: Commands,
) {
    let definitions_changed = definitions.is_changed() || active.is_changed();
    let Some(definitions) = active.get(&definitions) else {
        return;
    };
    let mut affected: EntityHashSet = changed_stations.iter().collect();
    affected.extend(discarded.read().map(|node| node.0));
    if definitions_changed {
        affected.extend(stations.iter().map(|(entity, _)| entity));
    }
    for entity in &changed_nodes {
        let Ok((name, _)) = nodes.get(entity) else {
            continue;
        };
        let name = name.as_str().to_ascii_lowercase();
        if is_cable_node(&name) {
            affected.extend(
                parents
                    .iter_ancestors::<ChildOf>(entity)
                    .find(|station| stations.contains(*station)),
            );
        }
    }
    for station in affected {
        let Ok((_, settings)) = stations.get(station) else {
            continue;
        };
        let Some(definition) = definitions.find_station(settings.definition) else {
            continue;
        };
        if definition.kind != TransportationTrackKind::Sky {
            continue;
        }
        let stops = definitions
            .sky_station_stop_nodes(settings.definition)
            .unwrap_or_default();
        let mut sockets = HashMap::new();
        for child in children.iter_descendants_depth_first::<Children>(station) {
            let Ok((name, pose)) = nodes.get(child) else {
                continue;
            };
            let name = name.as_str().to_ascii_lowercase();
            if is_cable_node(&name) {
                sockets
                    .entry(AssetId::from_key(&name))
                    .or_insert(pose.translation());
            }
        }
        let geometry = SkyStationTravel([
            collect_station_cable(&sockets, "travelpoint", &stops[..2]),
            collect_station_cable(&sockets, "returntravelpoint", &stops[2..]),
        ]);
        if let Ok(mut current) = travel.get_mut(station) {
            current.set_if_neq(geometry);
        } else if geometry != SkyStationTravel::default() {
            commands.entity(station).try_insert(geometry);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn station_samples_the_local_span_and_uses_each_direction_stop() {
        let first_stop = AssetId::from_key("travelpoint_02");
        let reverse_stop = AssetId::from_key("travelpoint_03");
        let nodes: HashMap<_, _> = [
            (AssetId::from_key("travelpoint_01"), Vec3::ZERO),
            (first_stop, Vec3::X),
            (reverse_stop, Vec3::new(1.0, 0.0, 1.0)),
        ]
        .into_iter()
        .collect();
        let cable = collect_station_cable(
            &nodes,
            "travelpoint",
            &[Some(first_stop), Some(reverse_stop)],
        )
        .unwrap();
        assert_eq!(cable.pose(1.5), (Vec3::new(1.0, 0.0, 0.5), Vec3::Z));
        assert_eq!(cable.stop(false), Some(1.0));
        assert_eq!(cable.stop(true), Some(2.0));
    }
}
