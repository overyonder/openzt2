use avian3d::prelude::{SpatialQuery, SpatialQueryFilter};
use bevy::{gltf::Gltf, prelude::*};
use openzt2_game_data::world_definitions::object_placement::{
    FootprintCellFlags, PlacementConstraints,
};

use crate::assets::scene_prefab::ScenePrefabAsset;
use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::construction::construction_interaction_types::ConstructionPreview;
use crate::plugins::economy::zoo_cash_types::UnlimitedZooCash;
use crate::plugins::economy::zoo_cash_types::ZooCash;
use crate::plugins::progression::adoption_and_content_availability_types::ScenarioContentAvailability;
use crate::plugins::progression::unlock_types::UnlockedCatalogueDefinitionSet;
use crate::plugins::topology::topology_graph_types::EdgeKey;
use crate::plugins::topology::topology_graph_types::TopologyIndex;
use crate::plugins::world_spawn::prefab_model_readiness::first_missing_prefab_collider_model_asset_id;
use crate::plugins::world_spawn::selected_world_identity::SelectedWorldIdentity;
use crate::plugins::world_spawn::world_membership_types::WorldRoot;

use super::{
    object_placement_definition_queries::{
        authored_object_entrance_definitions, object_placeable_definition_is_unlocked,
        resolve_object_placeable_definition, select_authored_footprint_for_eighth_turns,
    },
    object_placement_validation::{
        calculate_authored_object_placement_eighth_turns,
        calculate_object_placement_footprint_origin, rotate_object_placement_cell_by_quarter_turns,
    },
    ObjectPlacementPrefabSource, ObjectPlacementPreviewPermissionFacts,
    PlacedObjectRelocationSource,
};

pub(super) fn project_authoritative_facts_onto_object_placement_previews(
    cash: Res<ZooCash>,
    unlimited_cash: Option<Res<UnlimitedZooCash>>,
    unlocks: Res<UnlockedCatalogueDefinitionSet>,
    worlds: Query<
        (
            Ref<SelectedWorldIdentity>,
            Option<Ref<ScenarioContentAvailability>>,
        ),
        With<WorldRoot>,
    >,
    topology: Res<TopologyIndex>,
    spatial_query: SpatialQuery,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    prefabs: Res<Assets<ScenePrefabAsset>>,
    models: Res<Assets<Gltf>>,
    mut previews: Query<(
        Entity,
        &ConstructionPreview,
        &mut ObjectPlacementPreviewPermissionFacts,
        &ObjectPlacementPrefabSource,
        Option<&PlacedObjectRelocationSource>,
    )>,
) {
    let Some(catalogue) = active_definitions.get(&definitions) else {
        return;
    };
    let (mode, scenario) = worlds
        .single()
        .map_or((None, None), |(selection, availability)| {
            (Some(selection.mode), availability.map(Ref::into_inner))
        });
    for (entity, preview, mut permission, prefab_source, relocation) in &mut previews {
        let Some(definition) = resolve_object_placeable_definition(catalogue, preview.definition)
        else {
            continue;
        };
        let constraints = definition.constraints;
        let pose = calculate_authored_object_placement_eighth_turns(definition, &preview.transform)
            .and_then(|turns| {
                calculate_object_placement_footprint_origin(definition, &preview.transform, turns)
                    .map(|origin| (origin, turns))
            });
        let topology_valid = pose.is_some_and(|(origin, turns)| {
            let has_path = !constraints.contains_all(PlacementConstraints::REQUIRE_PATH)
                || authored_object_entrance_definitions(definition)
                    .iter()
                    .any(|entrance| {
                        let local_position = Vec3::from_array(
                            entrance.position_cm.map(|value| f32::from(value) * 0.01),
                        );
                        let local_forward = Vec3::from_array(
                            entrance
                                .forward_snorm
                                .map(|value| f32::from(value) / f32::from(i16::MAX)),
                        )
                        .normalize_or_zero();
                        let world_position = preview.transform.transform_point(local_position);
                        let world_forward = preview
                            .transform
                            .rotation
                            .mul_vec3(local_forward)
                            .normalize_or_zero();
                        [world_position, world_position + world_forward]
                            .into_iter()
                            .map(|point| point.round().as_ivec3())
                            .any(|cell| topology.paths.contains_key(&cell))
                    });
            let has_wall = !constraints.contains_all(PlacementConstraints::REQUIRE_WALL)
                || select_authored_footprint_for_eighth_turns(definition, turns)
                    .iter()
                    .filter(|cell| cell.flags.contains_all(FootprintCellFlags::OCCUPIED))
                    .any(|cell| {
                        let local =
                            IVec2::new(i32::from(cell.offset[0]), i32::from(cell.offset[1]));
                        let cell = origin
                            + rotate_object_placement_cell_by_quarter_turns(local, turns / 2);
                        let cell = IVec3::new(
                            cell.x,
                            preview.transform.translation.round().as_ivec3().y,
                            cell.y,
                        );
                        [IVec3::X, IVec3::NEG_X, IVec3::Z, IVec3::NEG_Z]
                            .into_iter()
                            .filter_map(|offset| EdgeKey::new(cell, cell + offset))
                            .any(|edge| topology.edges.contains_key(&edge))
                    });
            has_path && has_wall
        });
        let clearance_m = definition.minimum_headroom_metres;
        let headroom_valid = clearance_m <= f32::EPSILON
            || pose.is_some_and(|(origin, turns)| {
                select_authored_footprint_for_eighth_turns(definition, turns)
                    .iter()
                    .filter(|cell| cell.flags.contains_all(FootprintCellFlags::OCCUPIED))
                    .all(|cell| {
                        let local =
                            IVec2::new(i32::from(cell.offset[0]), i32::from(cell.offset[1]));
                        let cell = (origin
                            + rotate_object_placement_cell_by_quarter_turns(local, turns / 2))
                        .as_vec2();
                        spatial_query
                            .cast_ray_predicate(
                                Vec3::new(cell.x, preview.transform.translation.y + 0.01, cell.y),
                                Dir3::Y,
                                clearance_m,
                                true,
                                &SpatialQueryFilter::DEFAULT,
                                &|hit| {
                                    hit != entity && relocation.is_none_or(|source| hit != source.0)
                                },
                            )
                            .is_none()
                    })
            });
        let next = ObjectPlacementPreviewPermissionFacts {
            unlocked: object_placeable_definition_is_unlocked(
                catalogue,
                preview.definition,
                mode,
                scenario,
                &unlocks,
            ),
            affordable: unlimited_cash.is_some() || cash.0 .0 >= definition.price_cents,
            topology_valid,
            headroom_valid,
            prefab_ready: prefabs.get(&prefab_source.0).is_some_and(|prefab| {
                first_missing_prefab_collider_model_asset_id(prefab, &models).is_none()
            }),
        };
        if *permission != next {
            *permission = next;
        }
    }
}
