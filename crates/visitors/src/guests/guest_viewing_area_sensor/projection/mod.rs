use avian3d::prelude::{Collider, SpatialQuery, SpatialQueryFilter};
use bevy::{
    ecs::entity::{EntityHashMap, EntityHashSet},
    platform::collections::HashMap,
    prelude::*,
};
use openzt2_game_assets::world_definitions::{
    world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
    world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
};
use openzt2_game_data::AssetId;
use openzt2_interface_state::camera::camera_runtime_state_types::ZooCamera;
use openzt2_world_state::{
    simulation_time::simulation_clock_types::ZooClock,
    world_spawn::{
        persistent_id_types::PersistentId,
        world_entity_crating::UncratedWorldEntity,
        world_membership_types::{DefinitionId, WorldMember},
    },
};
use openzt2_zoo_state::{
    animal_health::types::Dead,
    animal_lifecycle::types::SpeciesHandle,
    authored_float_attributes::AuthoredFloatAttributes,
    habitat::habitat_types::{HabitatMember, HabitatRegion},
    staff::staff_job_types::StaffLocalViewingArea,
    terrain::dry_surface_eligibility::TerrainDrySurfaceEligibility,
    topology::topology_graph_types::{FenceEdge, PathTile},
};

use super::{
    GuestViewSupport, GuestViewingArea, GuestViewingAreaWorklist, GuestViewpointContactScores,
    GuestViewpointSupports,
};
use crate::guests::{
    guest_arrival_execution::GuestViewabilityRefreshRequested,
    guest_destination_and_viewing_execution::{view_is_clear, ViewingGroundSlots},
    guest_simulation_types::ViewingOpportunity,
};

#[allow(clippy::type_complexity)]
pub(in crate::guests) fn project_guest_viewing_areas(
    mut commands: Commands,
    clock: Single<&ZooClock>,
    active: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    mut requested: MessageReader<GuestViewabilityRefreshRequested>,
    surfaces: TerrainDrySurfaceEligibility,
    spatial: SpatialQuery,
    parents: Query<&ChildOf>,
    contact_owners: Query<(
        &WorldMember,
        Option<&DefinitionId>,
        Option<&FenceEdge>,
        Option<&AuthoredFloatAttributes>,
    )>,
    camera: Query<(&Camera, &GlobalTransform), With<ZooCamera>>,
    habitats: Query<&HabitatRegion>,
    mut roots: Query<(Entity, &mut GuestViewingAreaWorklist)>,
    mut animals: Query<
        (
            Entity,
            &WorldMember,
            &PersistentId,
            &GlobalTransform,
            &SpeciesHandle,
            &Visibility,
            Option<&HabitatMember>,
            Option<&DefinitionId>,
            Option<&AuthoredFloatAttributes>,
            Option<&mut GuestViewSupport>,
        ),
        (Without<Dead>, UncratedWorldEntity),
    >,
    mut viewpoints: Query<
        (
            Entity,
            &WorldMember,
            &PersistentId,
            &GlobalTransform,
            Option<&PathTile>,
            Option<&ViewingGroundSlots>,
            Option<&mut GuestViewpointSupports>,
            Option<&mut GuestViewpointContactScores>,
        ),
        Or<(
            With<PathTile>,
            With<ViewingOpportunity>,
            With<GuestViewpointSupports>,
        )>,
    >,
    mut areas: Query<(&mut GuestViewingArea, Has<StaffLocalViewingArea>)>,
) {
    let refreshed: EntityHashSet = requested.read().map(|event| event.root).collect();
    let Some(definitions) = active.get(&assets) else {
        return;
    };
    let Some(policy) = definitions.guest_viewing() else {
        return;
    };
    let Some(step) = definitions.simulation_timestep() else {
        return;
    };
    if !(policy.ground_range_m.is_finite() && policy.ground_range_m > 0.0) {
        return;
    }
    let refresh_ticks = u64::try_from(500_000_000_u128.div_ceil(step.as_nanos())).unwrap_or(1);
    let eye_height = Vec3::Y * policy.guest_eye_height_m;
    let view_bonus = AssetId::from_key("f_viewbonus");
    let range_squared = policy.ground_range_m * policy.ground_range_m;
    let bucket = |point: Vec3| (point.xz() / policy.ground_range_m).floor().as_ivec2();
    let cameras: Vec<_> = camera
        .iter()
        .filter(|(camera, _)| camera.is_active)
        .collect();
    let guest_type = AssetId::from_key("guest");
    let entrance_type = AssetId::from_key("entrance");
    let body_height = 2.9_f32;
    let radius = definitions
        .ordinary_guest_definitions()
        .next()
        .map_or(0.0, |guest| f32::from(guest.radius_cm) * 0.01);
    let contact_shape = (radius > 0.0 && radius * 2.0 < body_height)
        .then(|| Collider::capsule(radius, body_height - radius * 2.0));

    for (root, mut worklist) in &mut roots {
        if clock.tick < worklist.next_projection_tick && !refreshed.contains(&root) {
            continue;
        }
        worklist.bypass_change_detection().next_projection_tick =
            clock.tick.saturating_add(refresh_ticks);
        let mut supports_at =
            HashMap::<IVec2, Vec<(u64, Entity, Entity, Vec3, AssetId)>>::default();
        let mut area_order = EntityHashMap::default();
        let mut next_areas = EntityHashMap::<GuestViewingArea>::default();
        let mut camera_support = false;

        for (
            animal,
            member,
            id,
            pose,
            species,
            visibility,
            habitat,
            definition,
            attributes,
            support,
        ) in &mut animals
        {
            if member.root != root || *visibility == Visibility::Hidden {
                continue;
            }
            let point = pose.translation();
            let area = habitat
                .map(|habitat| habitat.habitat_entity)
                .filter(|&habitat| habitats.contains(habitat))
                .unwrap_or(root);
            let authored_bonus = definition
                .and_then(|definition| definitions.find_object(definition.0))
                .and_then(|object| {
                    object
                        .authored_runtime
                        .numeric_behavior_properties
                        .iter()
                        .find_map(|&(key, value)| (key == view_bonus).then_some(value))
                });
            let next = GuestViewSupport {
                species: Some(species.species),
                view_bonus: attributes
                    .and_then(|attributes| attributes.get(view_bonus))
                    .or(authored_bonus)
                    .unwrap_or_default(),
                excluded_surface: false,
            };
            if let Some(mut support) = support {
                support.set_if_neq(next);
            } else {
                commands.entity(animal).insert(next);
            }
            supports_at.entry(bucket(point)).or_default().push((
                id.0,
                animal,
                area,
                point,
                species.species,
            ));
            area_order
                .entry(area)
                .and_modify(|first: &mut u64| *first = (*first).min(id.0))
                .or_insert(id.0);
            next_areas.entry(area).or_default().supports.push(animal);

            if !camera_support {
                camera_support = cameras.iter().any(|(camera, pose)| {
                    camera.world_to_ndc(pose, point).is_some_and(|ndc| {
                        ndc.x.abs() <= 1.0 && ndc.y.abs() <= 1.0 && (0.0..=1.0).contains(&ndc.z)
                    }) && view_is_clear(
                        &spatial,
                        &parents,
                        pose.translation(),
                        point,
                        root,
                        root,
                        animal,
                    )
                });
            }
        }

        let mut visible_species = bevy::platform::collections::HashSet::<AssetId>::default();
        let mut intersections = Vec::new();
        for (point, member, id, pose, path, slots, support, contact) in &mut viewpoints {
            if member.root != root {
                continue;
            }
            let mut matched = Vec::new();
            let mut best_contact = 10_000;
            let path_position = path.map(|_| pose.translation());
            let stands: &[Vec3] = if path.is_some() {
                path_position.as_slice()
            } else {
                slots.map_or(&[][..], |slots| slots.positions.as_slice())
            };
            for &stand in stands {
                if !path
                    .and_then(|path| definitions.find_path(path.definition))
                    .is_some_and(|definition| definition.elevated)
                    && surfaces.on_land(root, stand.xz()) == Some(false)
                {
                    continue;
                }
                let mut stand_score = 0;
                if let Some(shape) = &contact_shape {
                    spatial.shape_intersections_callback(
                        shape,
                        stand + Vec3::Y * (body_height * 0.5),
                        Quat::IDENTITY,
                        &SpatialQueryFilter::DEFAULT,
                        |collider| {
                            for candidate in
                                std::iter::once(collider).chain(parents.iter_ancestors(collider))
                            {
                                if candidate == point {
                                    break;
                                }
                                let Ok((world, object, fence, attributes)) =
                                    contact_owners.get(candidate)
                                else {
                                    continue;
                                };
                                let definition =
                                    object.map(|definition| definition.0).or_else(|| {
                                        definitions
                                            .find_fence(fence?.definition)
                                            .map(|fence| fence.object)
                                    });
                                let Some(definition) = definition.filter(|_| world.root == root)
                                else {
                                    continue;
                                };
                                if definitions.find_object(definition).is_some_and(|object| {
                                    object.behavior_type_identifiers.contains(&entrance_type)
                                }) {
                                    break;
                                }
                                if let Some(scores) = definitions.collision_scores(definition) {
                                    let score = scores
                                        .by_attribute
                                        .iter()
                                        .find(|row| {
                                            row.subject_type == guest_type
                                                && attributes
                                                    .and_then(|attributes| {
                                                        attributes.get(row.property)
                                                    })
                                                    .is_some_and(|value| value != 0.0)
                                        })
                                        .map(|row| row.score)
                                        .or_else(|| {
                                            scores.by_type.iter().find_map(|&(kind, score)| {
                                                (kind == guest_type).then_some(score)
                                            })
                                        })
                                        .or(scores.base)
                                        .unwrap_or_default();
                                    stand_score = stand_score.max(score);
                                }
                                break;
                            }
                            stand_score < 10_000
                        },
                    );
                }
                best_contact = best_contact.min(stand_score);
                let center = bucket(stand);
                for x in -1..=1 {
                    for y in -1..=1 {
                        let Some(candidates) = supports_at.get(&(center + IVec2::new(x, y))) else {
                            continue;
                        };
                        for &(order, animal, area, target, species) in candidates {
                            if stand.distance_squared(target) <= range_squared
                                && view_is_clear(
                                    &spatial,
                                    &parents,
                                    stand + eye_height,
                                    target + Vec3::Y * 0.5,
                                    root,
                                    point,
                                    animal,
                                )
                            {
                                matched.push((order, animal, area, species));
                            }
                        }
                    }
                }
            }
            matched.sort_unstable_by_key(|(order, animal, ..)| (*order, *animal));
            matched.dedup_by_key(|(_, animal, ..)| *animal);
            let next =
                GuestViewpointSupports(matched.iter().map(|&(_, animal, ..)| animal).collect());
            if let Some(mut support) = support {
                support.set_if_neq(next);
            } else if !next.0.is_empty() {
                commands.entity(point).insert(next);
            }
            let next = GuestViewpointContactScores(vec![best_contact]);
            if let Some(mut contact) = contact {
                contact.set_if_neq(next);
            } else if !matched.is_empty() {
                commands.entity(point).insert(next);
            }
            for (_, _, area, species) in matched {
                intersections.push((id.0, point, area));
                visible_species.insert(species);
            }
        }

        intersections.sort_unstable_by_key(|&(order, point, _)| (order, point));
        for (_, point, area) in intersections {
            let intersections = &mut next_areas.get_mut(&area).expect("live area").intersections;
            if intersections.last() != Some(&point) {
                intersections.push(point);
            }
        }

        let mut ordered: Vec<_> = area_order.into_iter().collect();
        ordered.sort_unstable_by_key(|&(area, first)| (first, area));
        let retired: Vec<_> = worklist
            .areas
            .iter()
            .copied()
            .filter(|area| !next_areas.contains_key(area))
            .collect();
        for area in retired {
            if areas.contains(area) {
                commands
                    .entity(area)
                    .remove::<(GuestViewingArea, StaffLocalViewingArea)>();
            }
        }
        for (area, next) in next_areas {
            let has_viewpoints = !next.intersections.is_empty();
            let mut has_area_flag = false;
            if let Ok((mut current, flag)) = areas.get_mut(area) {
                current.set_if_neq(next);
                has_area_flag = flag;
            } else {
                commands.entity(area).insert(next);
            }
            match (has_area_flag, has_viewpoints) {
                (false, true) => {
                    commands.entity(area).insert(StaffLocalViewingArea);
                }
                (true, false) => {
                    commands.entity(area).remove::<StaffLocalViewingArea>();
                }
                _ => {}
            }
        }
        worklist
            .reborrow()
            .map_unchanged(|worklist| &mut worklist.areas)
            .set_if_neq(ordered.into_iter().map(|(area, _)| area).collect());
        worklist
            .reborrow()
            .map_unchanged(|worklist| &mut worklist.has_viewable_candidates)
            .set_if_neq(!supports_at.is_empty());
        worklist
            .reborrow()
            .map_unchanged(|worklist| &mut worklist.has_user_camera_view_candidate)
            .set_if_neq(camera_support);
        worklist
            .reborrow()
            .map_unchanged(|worklist| &mut worklist.mode_one_visible_species_count)
            .set_if_neq(Some(visible_species.len()));
    }
}
