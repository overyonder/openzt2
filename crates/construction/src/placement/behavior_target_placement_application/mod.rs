//! Siting, payment and creation of objects that behaviors put down.
//!
//! The site is the recipient's ground position, turned to face the subject.
//! The object is paid for through its authored `build` transaction when the
//! placement set returns, and only then enters the world. Behavior placement
//! never edits terrain: an object that a player placement would flatten under
//! is put down on the terrain as it stands.

use crate::placement::ground_path_placement_stomp::{GroundPathPlacementAction, GroundPathPlacementStomp};
use openzt2_zoo_state::placement::object_placement_definition_queries::collect_occupied_placement_cells_for_transform;
use openzt2_zoo_state::placement::object_placement_definition_queries::resolve_object_placeable_definition;
use openzt2_zoo_state::placement::object_placement_geometry::ScenePlacementFootprints;
use openzt2_zoo_state::placement::object_placement_validation::calculate_authored_object_placement_eighth_turns;
use openzt2_zoo_state::placement::object_placement_validation::validate_object_placement_footprint_cells;
use openzt2_zoo_state::placement::object_placement_validation::ObjectPlacementAuthoritativeFacts;
use crate::placement::placed_object_creation_edit_application::apply_prepared_placed_object_creation_edit;
use crate::placement::stomped_object_edit_preparation::{
    object_stomp_prevents_placement_at_cell, prepare_stomped_object_removals,
    StompablePlacedObject,
};
use openzt2_zoo_state::topology::topology_edit_types::{TopologyChanged, TopologySnapshot};
use openzt2_zoo_state::topology::topology_graph_types::topology_cell_rectangle;
use openzt2_zoo_state::topology::topology_grid_geometry::merge_topology_changed_cell_bounds;
use openzt2_zoo_state::placement::placement_preview_types::ObjectPlacementCellCollectionScratch;
use openzt2_zoo_state::placement::placement_transaction_types::ObjectPlacementCommitted;
use openzt2_zoo_state::placement::placed_object_types::PlacedObjectDefinitionReference;
use openzt2_zoo_state::placement::placed_object_types::PlacedObjectFootprintOccupancy;
use openzt2_zoo_state::placement::placement_occupancy_index::PlacedObjectFootprintQueries;
use openzt2_zoo_state::placement::placement_transaction_types::PreparedObjectPlacementEditMutationKind;
use openzt2_zoo_state::placement::placement_transaction_types::PreparedObjectPlacementMutation;
use openzt2_zoo_state::animal_behavior::behavior_target_placement_types::{
    BehaviorPlacementAwaitingPayment, BehaviorPlacementOf, BehaviorPlacementPutDown,
    BehaviorPlacementRecipient, BehaviorPlacementResult, BehaviorPlacementSiting,
    BehaviorTargetPlacement, BehaviorTargetPlacementSite,
};
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionState;
use openzt2_zoo_state::behavior_task_execution_types::PendingBehaviorTaskFailure;
use openzt2_zoo_state::construction::construction_interaction_types::PlacementValidity;
use openzt2_zoo_state::construction::construction_tool_and_placement_policy_types::ConstructionTool;
use openzt2_zoo_state::economy::account_transaction_types::Account;
use openzt2_zoo_state::economy::account_transaction_types::TransactionCompleted;
use openzt2_zoo_state::economy::account_transaction_types::TransactionKind;
use openzt2_zoo_state::economy::account_transaction_types::TransactionRejected;
use openzt2_zoo_state::economy::account_transaction_types::TransactionRequest;
use openzt2_zoo_state::economy::money_types::Money;
use openzt2_zoo_state::habitat::habitat_types::HabitatMember;
use openzt2_zoo_state::habitat::habitat_types::{HabitatIndex, HabitatRegion};
use openzt2_zoo_state::habitat::habitat_membership_and_containment::locate_habitat_at_world_position;
use openzt2_zoo_state::locomotion::locomotion_types::{ActiveTerrainDerivedNavigationGraph, NavAgent};
use openzt2_zoo_state::topology::topology_graph_types::TopologyGrid;
use openzt2_zoo_state::terrain::terrain_chunk_types::EditedTerrainSamples;
use openzt2_zoo_state::terrain::terrain_chunk_types::TerrainChunk;
use openzt2_zoo_state::terrain::terrain_chunk_types::TerrainIndex;
use openzt2_zoo_state::terrain::terrain_world_sampling::sample_terrain;
use openzt2_zoo_state::terrain::terrain_world_sampling::terrain_chunk_at;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentIdAllocator;
use openzt2_world_state::world_spawn::prefab_model_readiness::first_missing_prefab_collider_model_asset_id;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_world_state::world_spawn::world_entity_crating::WorldEntityIsCrated;
use openzt2_zoo_state::animal_health::types::Dead;
use bevy::asset::LoadState;
use bevy::ecs::system::SystemParam;
use bevy::gltf::Gltf;
use bevy::prelude::*;
use openzt2_game_assets::scene_prefab::ScenePrefabAsset;
use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_data::world_definitions::economy_transactions::{
    EconomyTransactionDefinition, EconomyTransactionKind,
};
use openzt2_zoo_state::animal_behavior::behavior_random_stream_state::BehaviorRandomStream;
use openzt2_game_data::world_definitions::object_placement::PlaceableDefinition;
use openzt2_game_data::world_definitions::object_placement::{FootprintCellFlags, PlacementConstraints};
use openzt2_zoo_state::placement::object_placement_definition_queries::authored_object_entrance_definitions;
use openzt2_zoo_state::placement::object_placement_validation::{
    calculate_object_placement_footprint_origin, rotate_object_placement_cell_by_quarter_turns,
};
use openzt2_zoo_state::topology::topology_graph_types::{EdgeKey, TopologyIndex};
use openzt2_zoo_state::economy::authored_transaction_amount::{
    authored_currency_to_money, evaluate_authored_transaction_amount,
};
use avian3d::prelude::{SpatialQuery, SpatialQueryFilter};
use openzt2_game_data::AssetId;

/// Payment for one placement operation.
#[derive(Component)]
pub(super) struct BehaviorTargetPlacementPayment {
    placement: Entity,
    definition_price: i64,
    terms: Vec<EconomyTransactionDefinition>,
}

/// Return of a payment whose placement could not be completed.
#[derive(Component)]
pub(super) struct BehaviorTargetPlacementRefund;

#[derive(SystemParam)]
pub(super) struct BehaviorTargetPlacementWorld<'w, 's> {
    surfaces: openzt2_zoo_state::terrain::dry_surface_eligibility::DrySurfaceEligibility<'w, 's>,
    server: Res<'w, AssetServer>,
    prefabs: Res<'w, Assets<ScenePrefabAsset>>,
    models: Res<'w, Assets<Gltf>>,
    terrain_indexes: Query<'w, 's, &'static TerrainIndex>,
    terrain_assets: Res<'w, Assets<TerrainAsset>>,
    terrain_chunks: Query<'w, 's, (&'static TerrainChunk, Option<&'static EditedTerrainSamples>)>,
    ground_paths: GroundPathPlacementStomp<'w, 's>,
    footprints: Res<'w, ScenePlacementFootprints>,
    recipients: Query<
        'w,
        's,
        (
            &'static WorldMember,
            &'static GlobalTransform,
            Option<&'static HabitatMember>,
        ),
        (Without<Dead>, Without<WorldEntityIsCrated>),
    >,
    actor_transforms: Query<'w, 's, &'static GlobalTransform>,
    agents: Query<'w, 's, &'static NavAgent>,
    tank_water: Query<
        'w,
        's,
        &'static openzt2_zoo_state::aquatic::tank_navigation_water::TankNavigationWater,
    >,
    navigation: Query<'w, 's, &'static ActiveTerrainDerivedNavigationGraph>,
    roots: Query<
        'w,
        's,
        (
            &'static HabitatIndex,
            &'static TopologyGrid,
            &'static TopologyIndex,
        ),
    >,
    regions: Query<'w, 's, &'static HabitatRegion>,
    spatial: SpatialQuery<'w, 's>,
}

#[derive(SystemParam)]
pub(super) struct BehaviorTargetPlacementCreation<'w, 's> {
    placement: PlacedObjectFootprintQueries<'w, 's>,
    placed: Query<
        'w,
        's,
        (
            &'static PersistentId,
            &'static PlacedObjectDefinitionReference,
            &'static Transform,
            &'static PlacedObjectFootprintOccupancy,
        ),
    >,
    stomp_targets: Query<'w, 's, StompablePlacedObject>,
    scratch: ResMut<'w, ObjectPlacementCellCollectionScratch>,
    allocators: Query<'w, 's, &'static mut PersistentIdAllocator>,
    committed: MessageWriter<'w, ObjectPlacementCommitted>,
    payments: MessageWriter<'w, TransactionRequest>,
}

#[derive(SystemParam)]
pub(super) struct BehaviorTargetPlacementActions<'w, 's> {
    actors: Query<
        'w,
        's,
        (&'static BehaviorTaskExecutionState, &'static WorldMember),
        Without<PendingBehaviorTaskFailure>,
    >,
    siting: Query<
        'w,
        's,
        (
            Entity,
            &'static BehaviorPlacementOf,
            &'static BehaviorPlacementRecipient,
            &'static BehaviorTargetPlacement,
            &'static mut BehaviorPlacementSiting,
        ),
    >,
    put_down: Query<
        'w,
        's,
        (
            Entity,
            &'static BehaviorPlacementOf,
            &'static BehaviorPlacementRecipient,
            &'static BehaviorTargetPlacement,
            &'static BehaviorTargetPlacementSite,
        ),
        (
            With<BehaviorPlacementPutDown>,
            Without<BehaviorPlacementAwaitingPayment>,
            Without<BehaviorPlacementResult>,
        ),
    >,
}

pub(super) fn apply_behavior_target_placements(
    mut commands: Commands,
    definitions: Res<WorldDefinitions>,
    definition_assets: Res<Assets<WorldDefinitionAsset>>,
    world: BehaviorTargetPlacementWorld,
    creation: BehaviorTargetPlacementCreation,
    mut actions: BehaviorTargetPlacementActions,
) {
    let Some(catalogue) = definitions.get(&definition_assets) else {
        return;
    };
    for (operation, owner, recipient, placement, mut siting) in &mut actions.siting {
        let actor = owner.0;
        if !actions.actors.get(actor).is_ok_and(|(task, world)| {
            placement.execution_id == task.execution_id
                && placement.root == world.root
                && placement.is_current(task)
        }) {
            continue;
        }
        let prefab = catalogue
            .find_object(placement.definition)
            .and_then(|object| catalogue.scene(object.prefab));
        if siting.0 != prefab {
            siting.0 = prefab;
        }
        match choose_behavior_target_placement_site(
            actor,
            recipient.0,
            placement,
            catalogue,
            definitions.is_complete(),
            &world,
            &creation,
        ) {
            SiteChoice::Pending => {}
            SiteChoice::Chosen(site) => {
                commands
                    .entity(operation)
                    .remove::<BehaviorPlacementSiting>()
                    .insert(site);
            }
            SiteChoice::Rejected => {
                commands
                    .entity(operation)
                    .remove::<BehaviorPlacementSiting>()
                    .insert(BehaviorPlacementResult(false));
            }
        }
    }
    for (operation, ..) in &actions.put_down {
        commands.run_system_cached_with(put_down_behavior_target_placement, operation);
    }
}

fn put_down_behavior_target_placement(
    In(operation): In<Entity>,
    mut commands: Commands,
    definitions: Res<WorldDefinitions>,
    definition_assets: Res<Assets<WorldDefinitionAsset>>,
    world: BehaviorTargetPlacementWorld,
    mut creation: BehaviorTargetPlacementCreation,
    actions: BehaviorTargetPlacementActions,
    mut random_streams: Query<&mut BehaviorRandomStream>,
) {
    let (Some(catalogue), Ok((_, owner, recipient, placement, site))) = (
        definitions.get(&definition_assets),
        actions.put_down.get(operation),
    ) else {
        return;
    };
    let actor = owner.0;
    if !actions.actors.get(actor).is_ok_and(|(task, world)| {
        placement.execution_id == task.execution_id
            && placement.root == world.root
            && placement.is_current(task)
    }) {
        return;
    }
    let Some((object, terms)) = authored_build_terms(catalogue, placement.definition) else {
        commands
            .entity(operation)
            .insert(BehaviorPlacementResult(false));
        return;
    };
    let mut random = random_streams.get_mut(actor).ok();
    let charge = authored_build_charge(&terms, object.price_cents, || {
        random.as_deref_mut().map(BehaviorRandomStream::next_u32)
    });
    if let Some(amount) = charge.filter(|amount| amount.is_positive()) {
        if !site.uses_current_prefab(catalogue, placement.definition)
            || !site_is_still_valid(recipient.0, placement, site, catalogue, &world, &creation)
        {
            commands
                .entity(operation)
                .insert(BehaviorPlacementResult(false));
            return;
        }
        let payment = commands
            .spawn(BehaviorTargetPlacementPayment {
                placement: operation,
                definition_price: object.price_cents,
                terms: terms.into_iter().cloned().collect::<Vec<_>>(),
            })
            .id();
        creation.payments.write(TransactionRequest {
            operation: payment,
            debit: Account::Zoo,
            credit: Account::External,
            amount,
            kind: TransactionKind::Service,
            subject: Some(actor),
        });
        commands
            .entity(operation)
            .insert(BehaviorPlacementAwaitingPayment(payment));
    } else {
        let accepted = charge.is_some()
            && create_or_refund(
                &mut commands,
                actor,
                recipient.0,
                placement.root,
                placement,
                site,
                Money::ZERO,
                catalogue,
                &world,
                &mut creation,
            );
        commands
            .entity(operation)
            .insert(BehaviorPlacementResult(accepted));
    }
}

/// Settles payment before creating the object. A stale or invalid placement
/// returns the payment to the zoo.
pub(super) fn settle_behavior_target_placement_payments(
    mut commands: Commands,
    mut completed: MessageReader<TransactionCompleted>,
    mut rejected: MessageReader<TransactionRejected>,
) {
    for payment in completed.read().copied() {
        commands.run_system_cached_with(complete_behavior_target_placement_payment, payment);
    }
    for payment in rejected.read().copied() {
        commands.run_system_cached_with(reject_behavior_target_placement_payment, payment);
    }
}

fn complete_behavior_target_placement_payment(
    In(payment): In<TransactionCompleted>,
    mut commands: Commands,
    definitions: Res<WorldDefinitions>,
    definition_assets: Res<Assets<WorldDefinitionAsset>>,
    world: BehaviorTargetPlacementWorld,
    mut creation: BehaviorTargetPlacementCreation,
    payments: Query<&BehaviorTargetPlacementPayment>,
    refunds: Query<(), With<BehaviorTargetPlacementRefund>>,
    placements: Query<
        (
            &BehaviorPlacementOf,
            &BehaviorPlacementRecipient,
            &BehaviorTargetPlacement,
            &BehaviorTargetPlacementSite,
            &BehaviorPlacementAwaitingPayment,
        ),
        (
            With<BehaviorPlacementPutDown>,
            Without<BehaviorPlacementResult>,
        ),
    >,
    actors: Query<(&BehaviorTaskExecutionState, &WorldMember), Without<PendingBehaviorTaskFailure>>,
) {
    let catalogue = definitions.get(&definition_assets);
    if refunds.contains(payment.operation) {
        commands.entity(payment.operation).despawn();
        return;
    }
    let Ok(owner) = payments.get(payment.operation) else {
        return;
    };
    commands.entity(payment.operation).despawn();
    let owned =
        placements
            .get(owner.placement)
            .ok()
            .filter(|(owner, _, placement, _, awaiting)| {
                awaiting.0 == payment.operation
                    && actors.get(owner.0).is_ok_and(|(task, world)| {
                        placement.execution_id == task.execution_id
                            && placement.root == world.root
                            && placement.is_current(task)
                    })
            });
    let (Some((actor, recipient, placement, site, _)), Some(catalogue)) = (owned, catalogue) else {
        request_behavior_target_placement_refund(
            &mut commands,
            &mut creation.payments,
            payment.amount,
        );
        return;
    };
    let terms_still_match =
        authored_build_terms(catalogue, placement.definition).is_some_and(|(object, terms)| {
            object.price_cents == owner.definition_price && terms.into_iter().eq(owner.terms.iter())
        });
    let accepted = if terms_still_match {
        create_or_refund(
            &mut commands,
            actor.0,
            recipient.0,
            placement.root,
            placement,
            site,
            payment.amount,
            catalogue,
            &world,
            &mut creation,
        )
    } else {
        request_behavior_target_placement_refund(
            &mut commands,
            &mut creation.payments,
            payment.amount,
        );
        false
    };
    commands
        .entity(owner.placement)
        .remove::<BehaviorPlacementAwaitingPayment>()
        .insert(BehaviorPlacementResult(accepted));
}

fn reject_behavior_target_placement_payment(
    In(payment): In<TransactionRejected>,
    mut commands: Commands,
    payments: Query<&BehaviorTargetPlacementPayment>,
    refunds: Query<(), With<BehaviorTargetPlacementRefund>>,
    placements: Query<
        (
            &BehaviorPlacementOf,
            &BehaviorPlacementRecipient,
            &BehaviorTargetPlacement,
            &BehaviorTargetPlacementSite,
            &BehaviorPlacementAwaitingPayment,
        ),
        (
            With<BehaviorPlacementPutDown>,
            Without<BehaviorPlacementResult>,
        ),
    >,
) {
    if refunds.contains(payment.operation) {
        warn!(reason = ?payment.reason, "behavior placement refund was rejected");
        commands.entity(payment.operation).despawn();
        return;
    }
    let Ok(owner) = payments.get(payment.operation) else {
        return;
    };
    commands.entity(payment.operation).despawn();
    if placements
        .get(owner.placement)
        .is_ok_and(|(_, _, _, _, awaiting)| awaiting.0 == payment.operation)
    {
        commands
            .entity(owner.placement)
            .remove::<BehaviorPlacementAwaitingPayment>()
            .insert(BehaviorPlacementResult(false));
    }
}

enum SiteChoice {
    Pending,
    Chosen(BehaviorTargetPlacementSite),
    Rejected,
}

fn choose_behavior_target_placement_site(
    actor: Entity,
    recipient: Entity,
    pending: &BehaviorTargetPlacement,
    catalogue: WorldDefinitionsView<'_>,
    catalogue_is_complete: bool,
    world: &BehaviorTargetPlacementWorld,
    creation: &BehaviorTargetPlacementCreation,
) -> SiteChoice {
    let (Some(object), Some(definition)) = (
        catalogue.find_object(pending.definition),
        resolve_object_placeable_definition(catalogue, pending.definition),
    ) else {
        return if catalogue_is_complete {
            SiteChoice::Rejected
        } else {
            SiteChoice::Pending
        };
    };
    let Some(handle) = catalogue.scene(object.prefab) else {
        return if catalogue_is_complete {
            SiteChoice::Rejected
        } else {
            SiteChoice::Pending
        };
    };
    if matches!(
        world.server.get_load_state(handle.id()),
        Some(LoadState::Failed(_))
    ) {
        return SiteChoice::Rejected;
    }
    let Some(prefab) = world.prefabs.get(&handle) else {
        return SiteChoice::Pending;
    };
    if prefab.canonical_scene_prefab_document().entities.is_empty() {
        return SiteChoice::Rejected;
    }
    // Automatic footprints are hydrated from the loaded prefab's bounds; a
    // prefab without bounds can never supply one.
    if definition.automatic_footprint
        && !world.footprints.has_scene_bounds(&handle)
        && definition.footprint.is_empty()
    {
        return if prefab
            .canonical_scene_prefab_document()
            .automatic_placement_bounds_xz
            .is_some()
        {
            SiteChoice::Pending
        } else {
            SiteChoice::Rejected
        };
    }
    if let Some(model) = first_missing_prefab_collider_model_asset_id(prefab, &world.models) {
        let failed = prefab.loaded_model_asset_handle(model).is_none_or(|model| {
            matches!(
                world.server.get_load_state(model.id()),
                Some(LoadState::Failed(_))
            )
        });
        return if failed {
            SiteChoice::Rejected
        } else {
            SiteChoice::Pending
        };
    }
    let (Ok((recipient_world, recipient_transform, habitat)), Ok(subject)) = (
        world.recipients.get(recipient),
        world.actor_transforms.get(actor),
    ) else {
        return SiteChoice::Rejected;
    };
    if recipient_world.root != pending.root
        || !object.prefab_scale.is_finite()
        || object.prefab_scale <= 0.0
    {
        return SiteChoice::Rejected;
    }
    let mut candidates = Vec::new();
    if let Some(member) = habitat {
        let Ok(agent) = world.agents.get(actor) else {
            return SiteChoice::Rejected;
        };
        let (Ok(navigation), Ok((_, grid, _)), Ok(region)) = (
            world.navigation.get(pending.root),
            world.roots.get(pending.root),
            world.regions.get(member.habitat_entity),
        ) else {
            return SiteChoice::Pending;
        };
        navigation.collect_dry_ground_candidates(
            world.tank_water.get(pending.root).ok(),
            region,
            *grid,
            agent,
            recipient_transform.translation(),
            &mut candidates,
        );
    }
    // Preserve the recipient site when valid, then try dry shore nodes. The
    // authored placement set still navigates to and docks at the chosen site.
    for candidate in std::iter::once(recipient_transform.translation()).chain(candidates) {
        let position = candidate.xz();
        let Some(point) = sample_behavior_placement_terrain(world, pending.root, position) else {
            continue;
        };
        if point.1 {
            continue;
        }
        let toward_subject = (subject.translation().xz() - position)
            .try_normalize()
            .unwrap_or(Vec2::Y);
        let step = authored_rotation_step_degrees(definition);
        let yaw = (toward_subject.x.atan2(toward_subject.y).to_degrees() / step).round() * step;
        let site = BehaviorTargetPlacementSite {
            rules: definition.clone(),
            transform: Transform {
                translation: Vec3::new(position.x, point.0, position.y),
                rotation: Quat::from_rotation_y(yaw.to_radians()),
                scale: Vec3::splat(object.prefab_scale),
            },
            prefab: handle.clone(),
        };
        if site_is_still_valid(recipient, pending, &site, catalogue, world, creation) {
            return SiteChoice::Chosen(site);
        }
    }
    SiteChoice::Rejected
}

fn site_is_still_valid(
    recipient: Entity,
    pending: &BehaviorTargetPlacement,
    site: &BehaviorTargetPlacementSite,
    catalogue: WorldDefinitionsView<'_>,
    world: &BehaviorTargetPlacementWorld,
    creation: &BehaviorTargetPlacementCreation,
) -> bool {
    let Some(definition) = resolve_object_placeable_definition(catalogue, pending.definition)
    else {
        return false;
    };
    let Ok((recipient_world, _, habitat)) = world.recipients.get(recipient) else {
        return false;
    };
    if recipient_world.root != pending.root {
        return false;
    }
    let (Ok((habitats, grid, topology)), Ok(terrain_index)) = (
        world.roots.get(pending.root),
        world.terrain_indexes.get(pending.root),
    ) else {
        return false;
    };
    let expected_habitat = habitat.map(|member| member.habitat_entity);
    if locate_habitat_at_world_position(habitats, site.transform.translation.xz(), *grid)
        != expected_habitat
        || !sample_behavior_placement_terrain(world, pending.root, site.transform.translation.xz())
            .is_some_and(|(_, water)| !water)
    {
        return false;
    }
    let Some(turns) = calculate_authored_object_placement_eighth_turns(definition, &site.transform)
    else {
        return false;
    };
    let paths = world
        .ground_paths
        .overlaps(pending.root, definition, catalogue);
    let geometry = world.footprints.resolve(definition, Some(&site.prefab));
    let Some(origin) =
        calculate_object_placement_footprint_origin(geometry, &site.transform, turns)
    else {
        return false;
    };
    let constraints = definition.constraints;
    let occupied = || {
        geometry
            .cells(turns)
            .iter()
            .filter(|cell| cell.flags.contains_all(FootprintCellFlags::OCCUPIED))
            .map(|cell| {
                let local = IVec2::new(i32::from(cell.offset[0]), i32::from(cell.offset[1]));
                origin + rotate_object_placement_cell_by_quarter_turns(local, turns / 2)
            })
    };
    let path_valid = !constraints.contains_all(PlacementConstraints::REQUIRE_PATH)
        || authored_object_entrance_definitions(definition)
            .iter()
            .any(|entrance| {
                let local_position =
                    Vec3::from_array(entrance.position_cm.map(|value| f32::from(value) * 0.01));
                let local_forward = Vec3::from_array(
                    entrance
                        .forward_snorm
                        .map(|value| f32::from(value) / f32::from(i16::MAX)),
                )
                .normalize_or_zero();
                let position = site.transform.transform_point(local_position);
                let forward = site
                    .transform
                    .rotation
                    .mul_vec3(local_forward)
                    .normalize_or_zero();
                [position, position + forward]
                    .into_iter()
                    .any(|point| topology.paths.contains_key(&point.round().as_ivec3()))
            });
    let wall_valid = !constraints.contains_all(PlacementConstraints::REQUIRE_WALL)
        || occupied().any(|cell| {
            let cell = IVec3::new(
                cell.x,
                site.transform.translation.round().as_ivec3().y,
                cell.y,
            );
            [IVec3::X, IVec3::NEG_X, IVec3::Z, IVec3::NEG_Z]
                .into_iter()
                .filter_map(|offset| EdgeKey::new(cell, cell + offset))
                .any(|edge| topology.edges.contains_key(&edge))
        });
    let clearance = definition.minimum_headroom_metres;
    let headroom_valid = clearance <= f32::EPSILON
        || occupied().all(|cell| {
            let cell = cell.as_vec2();
            let ray_origin = Vec3::new(cell.x, site.transform.translation.y + 0.01, cell.y);
            world
                .spatial
                .cast_ray(
                    ray_origin,
                    Dir3::Y,
                    clearance,
                    true,
                    &SpatialQueryFilter::DEFAULT,
                )
                .is_none()
        });
    let validity = validate_object_placement_footprint_cells(
        geometry,
        geometry.cells(turns),
        &site.transform,
        |position| {
            if locate_habitat_at_world_position(habitats, position, *grid) != expected_habitat {
                return None;
            }
            let entity = terrain_chunk_at(terrain_index, position)?;
            let (chunk, edited) = world.terrain_chunks.get(entity).ok()?;
            sample_terrain(
                chunk,
                world.terrain_assets.get(&chunk.asset)?,
                edited,
                position,
            )
            .filter(|_| world.surfaces.on_land(pending.root, position) == Some(true))
        },
        |cell| {
            paths.prevents_placement(cell)
                || object_stomp_prevents_placement_at_cell(
                    cell,
                    definition,
                    catalogue,
                    pending.root,
                    &creation.placement,
                    &creation.stomp_targets,
                    |_| false,
                )
        },
        ObjectPlacementAuthoritativeFacts {
            habitat: habitat.map(|member| member.habitat_entity),
            unlocked: true,
            affordable: true,
            topology_valid: path_valid && wall_valid,
            headroom_valid,
            surface_valid: true,
        },
    );
    matches!(validity, PlacementValidity::Valid { .. })
}

#[allow(
    clippy::too_many_arguments,
    reason = "one creation step borrows the placement owner's live state"
)]
fn create_or_refund(
    commands: &mut Commands,
    actor: Entity,
    recipient: Entity,
    root: Entity,
    pending: &BehaviorTargetPlacement,
    site: &BehaviorTargetPlacementSite,
    paid: Money,
    catalogue: WorldDefinitionsView<'_>,
    world: &BehaviorTargetPlacementWorld,
    creation: &mut BehaviorTargetPlacementCreation,
) -> bool {
    if create_behavior_target_placement_object(
        commands, actor, recipient, root, pending, site, catalogue, world, creation,
    )
    .is_some()
    {
        return true;
    }
    if paid.is_positive() {
        request_behavior_target_placement_refund(commands, &mut creation.payments, paid);
    }
    false
}

#[allow(
    clippy::too_many_arguments,
    reason = "one creation step borrows the placement owner's live state"
)]
fn create_behavior_target_placement_object(
    commands: &mut Commands,
    actor: Entity,
    recipient: Entity,
    root: Entity,
    pending: &BehaviorTargetPlacement,
    site: &BehaviorTargetPlacementSite,
    catalogue: WorldDefinitionsView<'_>,
    world: &BehaviorTargetPlacementWorld,
    creation: &mut BehaviorTargetPlacementCreation,
) -> Option<Entity> {
    if !site.uses_current_prefab(catalogue, pending.definition)
        || !site_is_still_valid(recipient, pending, site, catalogue, world, creation)
    {
        return None;
    }
    let definition = resolve_object_placeable_definition(catalogue, pending.definition)?;
    let geometry = world.footprints.resolve(definition, Some(&site.prefab));
    let cells = collect_occupied_placement_cells_for_transform(
        geometry,
        &site.transform,
        &mut creation.scratch.cells,
    )?
    .to_vec()
    .into_boxed_slice();
    let removals = prepare_stomped_object_removals(
        &cells,
        definition,
        catalogue,
        root,
        &creation.placement,
        &creation.stomp_targets,
        |_| false,
    )?;
    let stomped = removals
        .iter()
        .filter_map(|removal| removal.applied_entity)
        .collect::<Vec<_>>();
    let paths = world
        .ground_paths
        .overlaps(pending.root, definition, catalogue);
    let path_removals = cells
        .iter()
        .flat_map(|cell| paths.actions_at(*cell))
        .filter_map(|action| match action {
            GroundPathPlacementAction::Delete(entity, TopologySnapshot::Path { cell, .. }) => {
                Some((entity, cell))
            }
            _ => None,
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    let id = creation.allocators.get_mut(root).ok()?.allocate().ok()?;
    let mut edit = PreparedObjectPlacementMutation {
        entity: id,
        definition: pending.definition,
        transform: site.transform,
        cells,
        previous_transform: None,
        previous_cells: Box::new([]),
        mutation: PreparedObjectPlacementEditMutationKind::Create,
        applied_entity: None,
        preview: None,
        prefab: site.prefab.clone(),
    };
    let accepted = apply_prepared_placed_object_creation_edit(
        commands,
        actor,
        &mut edit,
        catalogue,
        root,
        &creation.placement,
        &mut creation.scratch,
        &world.footprints,
        &world.prefabs,
        &world.models,
        &creation.placed,
        &stomped,
        false,
        &mut ConstructionTool::Inspect,
        &mut creation.committed,
    );
    let entity = edit.applied_entity.filter(|_| accepted)?;
    for removal in removals {
        if let Some(target) = removal.applied_entity {
            commands.entity(target).try_despawn();
        }
    }
    let mut changed_path_bounds = None;
    for (path, cell) in path_removals {
        commands.entity(path).try_despawn();
        changed_path_bounds =
            merge_topology_changed_cell_bounds(changed_path_bounds, topology_cell_rectangle(cell));
    }
    if let Some(bounds) = changed_path_bounds {
        commands.write_message(TopologyChanged {
            transaction: None,
            bounds,
        });
    }
    Some(entity)
}

fn request_behavior_target_placement_refund(
    commands: &mut Commands,
    requests: &mut MessageWriter<TransactionRequest>,
    amount: Money,
) {
    let operation = commands.spawn(BehaviorTargetPlacementRefund).id();
    requests.write(TransactionRequest {
        operation,
        debit: Account::External,
        credit: Account::Zoo,
        amount,
        kind: TransactionKind::Refund,
        subject: None,
    });
}

fn authored_build_terms(
    catalogue: WorldDefinitionsView<'_>,
    definition: AssetId,
) -> Option<(
    &openzt2_game_data::world_definitions::world_objects::WorldObjectDefinition,
    Vec<&EconomyTransactionDefinition>,
)> {
    let object = catalogue.find_object(definition)?;
    let mut next = Some(AssetId::from_key("build"));
    let mut terms = Vec::new();
    for step in 0..=object.transactions.len() {
        let Some(name) = next else {
            return Some((object, terms));
        };
        let Some(transaction) = object
            .transactions
            .iter()
            .find(|transaction| transaction.name == name)
        else {
            return (step == 0).then_some((object, terms));
        };
        if transaction.kind != EconomyTransactionKind::Debit || transaction.target.is_some() {
            return None;
        }
        terms.push(transaction);
        next = transaction.next_transaction;
    }
    None
}

fn authored_build_charge(
    terms: &[&EconomyTransactionDefinition],
    definition_price: i64,
    mut random: impl FnMut() -> Option<u32>,
) -> Option<Money> {
    terms.iter().try_fold(Money::ZERO, |sum, transaction| {
        let amount = evaluate_authored_transaction_amount(
            transaction,
            None,
            definition_price,
            None,
            &mut random,
        )?;
        sum.checked_add(authored_currency_to_money(amount)?)
    })
}

/// The smallest turn that is both an authored rotation increment and a
/// footprint eighth turn.
fn authored_rotation_step_degrees(definition: &PlaceableDefinition) -> f32 {
    let increment = u32::from(definition.rotation_increment_degrees);
    if increment == 0 {
        return 45.0;
    }
    let (mut left, mut right) = (increment, 45);
    while right != 0 {
        (left, right) = (right, left % right);
    }
    #[allow(
        clippy::cast_precision_loss,
        reason = "least common multiple of a u16 and 45 is exact in f32"
    )]
    let step = (increment / left * 45) as f32;
    step
}

/// Ground height and whether water covers it.
fn sample_behavior_placement_terrain(
    world: &BehaviorTargetPlacementWorld,
    root: Entity,
    position: Vec2,
) -> Option<(f32, bool)> {
    let entity = terrain_chunk_at(world.terrain_indexes.get(root).ok()?, position)?;
    let (chunk, edited) = world.terrain_chunks.get(entity).ok()?;
    let point = sample_terrain(
        chunk,
        world.terrain_assets.get(&chunk.asset)?,
        edited,
        position,
    )?;
    Some((point.height_m, !world.surfaces.on_land(root, position)?))
}
