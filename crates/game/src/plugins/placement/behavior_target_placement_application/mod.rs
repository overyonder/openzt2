//! Siting, payment and creation of objects that behaviors put down.
//!
//! The site is the recipient's ground position, turned to face the subject.
//! The object is paid for through its authored `build` transaction when the
//! placement set returns, and only then enters the world. A behavior placement
//! never edits terrain, so definitions that flatten or need a path, wall or
//! headroom are rejected rather than placed without that policy.

use super::{
    ground_path_object_placement_collision_queries::GroundPathTilesUnderObjectPlacement,
    object_placement_definition_queries::{
        collect_occupied_placement_cells_for_transform, resolve_object_placeable_definition,
        select_authored_footprint_for_eighth_turns,
    },
    object_placement_validation::{
        calculate_authored_object_placement_eighth_turns,
        object_placement_cell_is_blocked_by_existing_object,
        validate_object_placement_footprint_cells, ObjectPlacementAuthoritativeFacts,
    },
    placed_object_creation_edit_application::apply_prepared_placed_object_creation_edit,
    ObjectPlacementCellCollectionScratch, ObjectPlacementCommitted,
    PlacedObjectDefinitionReference, PlacedObjectFootprintOccupancy,
    PlacedObjectFootprintOccupancyIndex, PreparedObjectPlacementEditMutationKind,
    PreparedObjectPlacementMutation,
};
use crate::plugins::{
    animal_behavior::behavior_target_placement_execution::types::{
        BehaviorTargetPlacementPaymentState, BehaviorTargetPlacementSite,
        BehaviorTargetPlacementStage, PendingBehaviorTargetPlacement,
    },
    behavior_task_execution_types::{BehaviorTaskExecutionState, PendingBehaviorTaskFailure},
    construction::{
        construction_interaction_types::PlacementValidity,
        construction_tool_and_placement_policy_types::ConstructionTool,
    },
    economy::{
        account_transaction_types::{
            Account, TransactionCompleted, TransactionKind, TransactionRejected, TransactionRequest,
        },
        money_types::Money,
    },
    habitat::habitat_types::HabitatMember,
    terrain::{
        terrain_chunk_types::{EditedTerrainSamples, TerrainChunk, TerrainIndex},
        terrain_world_sampling::{sample_terrain, terrain_chunk_at},
    },
    world_spawn::{
        persistent_id_types::{PersistentId, PersistentIdAllocator},
        prefab_model_readiness::first_missing_prefab_collider_model_asset_id,
        world_membership_types::WorldMember,
    },
};
use bevy::{
    asset::LoadState, ecs::system::SystemParam, gltf::Gltf, platform::collections::HashMap,
    prelude::*,
};
use openzt2_game_assets::{
    scene_prefab::ScenePrefabAsset,
    terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset,
    world_definitions::{
        world_definition_asset_set_state_and_borrowing_queries::{
            WorldDefinitions, WorldDefinitionsView,
        },
        world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
    },
};
use openzt2_game_data::{
    world_definitions::{
        economy_transactions::{EconomyTransactionCostBasis, EconomyTransactionKind},
        object_placement::{PlaceableDefinition, PlacementConstraints},
    },
    AssetId,
};

/// Payment for one behavior placement; the actor's pending placement owns it.
#[derive(Component)]
pub(super) struct BehaviorTargetPlacementPayment {
    actor: Entity,
}

/// Return of a payment whose placement could not be completed.
#[derive(Component)]
pub(super) struct BehaviorTargetPlacementRefund;

#[derive(SystemParam)]
pub(super) struct BehaviorTargetPlacementWorld<'w, 's> {
    server: Res<'w, AssetServer>,
    prefabs: Res<'w, Assets<ScenePrefabAsset>>,
    models: Res<'w, Assets<Gltf>>,
    terrain_index: Res<'w, TerrainIndex>,
    terrain_assets: Res<'w, Assets<TerrainAsset>>,
    terrain_chunks: Query<'w, 's, (&'static TerrainChunk, Option<&'static EditedTerrainSamples>)>,
    ground_paths: GroundPathTilesUnderObjectPlacement<'w, 's>,
    recipients: Query<
        'w,
        's,
        (
            &'static WorldMember,
            &'static GlobalTransform,
            Option<&'static HabitatMember>,
        ),
    >,
    actor_transforms: Query<'w, 's, &'static GlobalTransform>,
}

#[derive(SystemParam)]
pub(super) struct BehaviorTargetPlacementCreation<'w, 's> {
    placement: ResMut<'w, PlacedObjectFootprintOccupancyIndex>,
    placed: Query<
        'w,
        's,
        (
            &'static PersistentId,
            &'static PlacedObjectDefinitionReference,
            &'static mut Transform,
            &'static mut PlacedObjectFootprintOccupancy,
        ),
    >,
    scratch: ResMut<'w, ObjectPlacementCellCollectionScratch>,
    allocator: ResMut<'w, PersistentIdAllocator>,
    committed: MessageWriter<'w, ObjectPlacementCommitted>,
    payments: MessageWriter<'w, TransactionRequest>,
}

pub(super) fn apply_behavior_target_placements(
    mut commands: Commands,
    definitions: Res<WorldDefinitions>,
    definition_assets: Res<Assets<WorldDefinitionAsset>>,
    world: BehaviorTargetPlacementWorld,
    mut creation: BehaviorTargetPlacementCreation,
    mut actors: Query<
        (
            Entity,
            &BehaviorTaskExecutionState,
            &WorldMember,
            &mut PendingBehaviorTargetPlacement,
        ),
        Without<PendingBehaviorTaskFailure>,
    >,
) {
    let Some(catalogue) = definitions.get(&definition_assets) else {
        return;
    };
    let mut created = HashMap::new();
    for (actor, task, actor_world, mut pending) in &mut actors {
        if !pending.owns(task, actor_world)
            || !pending.is_current(task)
            || pending.definition_revision != definitions.catalogue_revision()
        {
            continue;
        }
        let next = match &pending.stage {
            BehaviorTargetPlacementStage::Siting => {
                match choose_behavior_target_placement_site(
                    actor,
                    &pending,
                    catalogue,
                    definitions.is_complete(),
                    &world,
                    &creation,
                    &created,
                ) {
                    SiteChoice::Pending => continue,
                    SiteChoice::Chosen(site) => BehaviorTargetPlacementStage::Sited(site),
                    SiteChoice::Rejected => BehaviorTargetPlacementStage::Rejected,
                }
            }
            BehaviorTargetPlacementStage::PuttingDown {
                site,
                marker,
                payment: BehaviorTargetPlacementPaymentState::Unrequested,
            } => {
                let charge = authored_build_charge(catalogue, pending.definition).filter(|_| {
                    site_is_still_valid(&pending, site, catalogue, &world, &creation, &created)
                });
                if let Some(amount) = charge.filter(|amount| amount.is_positive()) {
                    let operation = commands
                        .spawn(BehaviorTargetPlacementPayment { actor })
                        .id();
                    creation.payments.write(TransactionRequest {
                        operation,
                        debit: Account::Zoo,
                        credit: Account::External,
                        amount,
                        kind: TransactionKind::Service,
                        subject: Some(actor),
                    });
                    BehaviorTargetPlacementStage::PuttingDown {
                        site: site.clone(),
                        marker: *marker,
                        payment: BehaviorTargetPlacementPaymentState::Requested(operation),
                    }
                } else if charge.is_some() {
                    create_or_refund(
                        &mut commands,
                        actor,
                        actor_world.root,
                        &pending,
                        site,
                        Money::ZERO,
                        catalogue,
                        &world,
                        &mut creation,
                        &mut created,
                    )
                } else {
                    BehaviorTargetPlacementStage::Rejected
                }
            }
            _ => continue,
        };
        pending.stage = next;
    }
}

/// Settles placement payments and puts paid objects down in the same step,
/// so no paid state outlives the instruction. A payment whose placement was
/// interrupted, or whose site became invalid, is returned: the zoo is never
/// charged for an object that was not put down.
#[allow(
    clippy::too_many_arguments,
    reason = "settlement borrows the placement owner's live state"
)]
pub(super) fn settle_behavior_target_placement_payments(
    mut commands: Commands,
    definitions: Res<WorldDefinitions>,
    definition_assets: Res<Assets<WorldDefinitionAsset>>,
    world: BehaviorTargetPlacementWorld,
    mut creation: BehaviorTargetPlacementCreation,
    mut completed: MessageReader<TransactionCompleted>,
    mut rejected: MessageReader<TransactionRejected>,
    payments: Query<&BehaviorTargetPlacementPayment>,
    refunds: Query<(), With<BehaviorTargetPlacementRefund>>,
    mut placements: Query<
        (
            &BehaviorTaskExecutionState,
            &WorldMember,
            &mut PendingBehaviorTargetPlacement,
        ),
        Without<PendingBehaviorTaskFailure>,
    >,
) {
    let catalogue = definitions.get(&definition_assets);
    let mut created = HashMap::new();
    for payment in completed.read() {
        if refunds.contains(payment.operation) {
            commands.entity(payment.operation).despawn();
            continue;
        }
        let Ok(owner) = payments.get(payment.operation) else {
            continue;
        };
        commands.entity(payment.operation).despawn();
        let owned = placements
            .get_mut(owner.actor)
            .ok()
            .filter(|(task, actor_world, pending)| {
                pending.owns(task, actor_world)
                    && pending.is_current(task)
                    && matches!(
                        pending.stage,
                        BehaviorTargetPlacementStage::PuttingDown {
                            payment: BehaviorTargetPlacementPaymentState::Requested(operation),
                            ..
                        } if operation == payment.operation
                    )
            });
        let (Some((_, actor_world, mut pending)), Some(catalogue)) = (owned, catalogue) else {
            request_behavior_target_placement_refund(
                &mut commands,
                &mut creation.payments,
                payment.amount,
            );
            continue;
        };
        let BehaviorTargetPlacementStage::PuttingDown { site, .. } = &pending.stage else {
            continue;
        };
        let site = site.clone();
        pending.stage = create_or_refund(
            &mut commands,
            owner.actor,
            actor_world.root,
            &pending,
            &site,
            payment.amount,
            catalogue,
            &world,
            &mut creation,
            &mut created,
        );
    }
    for payment in rejected.read() {
        if refunds.contains(payment.operation) {
            warn!(reason = ?payment.reason, "behavior placement refund was rejected");
            commands.entity(payment.operation).despawn();
            continue;
        }
        let Ok(owner) = payments.get(payment.operation) else {
            continue;
        };
        commands.entity(payment.operation).despawn();
        if let Ok((_, _, mut pending)) = placements.get_mut(owner.actor) {
            if matches!(
                pending.stage,
                BehaviorTargetPlacementStage::PuttingDown {
                    payment: BehaviorTargetPlacementPaymentState::Requested(operation),
                    ..
                } if operation == payment.operation
            ) {
                pending.stage = BehaviorTargetPlacementStage::Rejected;
            }
        }
    }
}

enum SiteChoice {
    Pending,
    Chosen(BehaviorTargetPlacementSite),
    Rejected,
}

fn choose_behavior_target_placement_site(
    actor: Entity,
    pending: &PendingBehaviorTargetPlacement,
    catalogue: WorldDefinitionsView<'_>,
    catalogue_is_complete: bool,
    world: &BehaviorTargetPlacementWorld,
    creation: &BehaviorTargetPlacementCreation,
    created: &HashMap<Entity, AssetId>,
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
    if !behavior_placement_supports_definition(definition) {
        return SiteChoice::Rejected;
    }
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
    let (Ok((recipient_world, recipient, _)), Ok(subject)) = (
        world.recipients.get(pending.recipient),
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
    let position = recipient.translation().xz();
    let Some(point) = sample_behavior_placement_terrain(world, position) else {
        return SiteChoice::Rejected;
    };
    // Water placement needs the floating policy some dishes author.
    if point.1 {
        return SiteChoice::Rejected;
    }
    let toward_subject = (subject.translation().xz() - position)
        .try_normalize()
        .unwrap_or(Vec2::Y);
    let step = authored_rotation_step_degrees(definition);
    let yaw = (toward_subject.x.atan2(toward_subject.y).to_degrees() / step).round() * step;
    let transform = Transform {
        translation: Vec3::new(position.x, point.0, position.y),
        rotation: Quat::from_rotation_y(yaw.to_radians()),
        scale: Vec3::splat(object.prefab_scale),
    };
    let site = BehaviorTargetPlacementSite {
        transform,
        prefab: handle,
    };
    if site_is_still_valid(pending, &site, catalogue, world, creation, created) {
        SiteChoice::Chosen(site)
    } else {
        SiteChoice::Rejected
    }
}

fn site_is_still_valid(
    pending: &PendingBehaviorTargetPlacement,
    site: &BehaviorTargetPlacementSite,
    catalogue: WorldDefinitionsView<'_>,
    world: &BehaviorTargetPlacementWorld,
    creation: &BehaviorTargetPlacementCreation,
    created: &HashMap<Entity, AssetId>,
) -> bool {
    let Some(definition) = resolve_object_placeable_definition(catalogue, pending.definition)
    else {
        return false;
    };
    let Ok((recipient_world, _, habitat)) = world.recipients.get(pending.recipient) else {
        return false;
    };
    if recipient_world.root != pending.root {
        return false;
    }
    let Some(turns) = calculate_authored_object_placement_eighth_turns(definition, &site.transform)
    else {
        return false;
    };
    let paths_block = world
        .ground_paths
        .object_placement_cell_collides_with_ground_path(definition, catalogue);
    let validity = validate_object_placement_footprint_cells(
        definition,
        select_authored_footprint_for_eighth_turns(definition, turns),
        &site.transform,
        |position| {
            let entity = terrain_chunk_at(&world.terrain_index, position)?;
            let (chunk, edited) = world.terrain_chunks.get(entity).ok()?;
            sample_terrain(
                chunk,
                world.terrain_assets.get(&chunk.asset)?,
                edited,
                position,
            )
        },
        |cell| {
            paths_block(cell)
                || object_placement_cell_is_blocked_by_existing_object(
                    &creation.placement,
                    cell,
                    None,
                    definition,
                    catalogue,
                    |entity| {
                        creation
                            .placed
                            .get(entity)
                            .ok()
                            .map(|(_, definition, _, _)| definition.definition)
                            .or_else(|| created.get(&entity).copied())
                    },
                )
        },
        ObjectPlacementAuthoritativeFacts {
            habitat: habitat.map(|member| member.habitat_entity),
            unlocked: true,
            affordable: true,
            topology_valid: false,
            headroom_valid: false,
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
    root: Entity,
    pending: &PendingBehaviorTargetPlacement,
    site: &BehaviorTargetPlacementSite,
    paid: Money,
    catalogue: WorldDefinitionsView<'_>,
    world: &BehaviorTargetPlacementWorld,
    creation: &mut BehaviorTargetPlacementCreation,
    created: &mut HashMap<Entity, AssetId>,
) -> BehaviorTargetPlacementStage {
    if create_behavior_target_placement_object(
        commands, actor, root, pending, site, catalogue, world, creation, created,
    )
    .is_some()
    {
        return BehaviorTargetPlacementStage::Placed;
    }
    if paid.is_positive() {
        request_behavior_target_placement_refund(commands, &mut creation.payments, paid);
    }
    BehaviorTargetPlacementStage::Rejected
}

#[allow(
    clippy::too_many_arguments,
    reason = "one creation step borrows the placement owner's live state"
)]
fn create_behavior_target_placement_object(
    commands: &mut Commands,
    actor: Entity,
    root: Entity,
    pending: &PendingBehaviorTargetPlacement,
    site: &BehaviorTargetPlacementSite,
    catalogue: WorldDefinitionsView<'_>,
    world: &BehaviorTargetPlacementWorld,
    creation: &mut BehaviorTargetPlacementCreation,
    created: &mut HashMap<Entity, AssetId>,
) -> Option<Entity> {
    if !site_is_still_valid(pending, site, catalogue, world, creation, created) {
        return None;
    }
    let definition = resolve_object_placeable_definition(catalogue, pending.definition)?;
    let cells = collect_occupied_placement_cells_for_transform(
        definition,
        &site.transform,
        &mut creation.scratch.cells,
    )?
    .to_vec()
    .into_boxed_slice();
    let id = creation.allocator.allocate(root).ok()?;
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
        Some(root),
        &mut creation.placement,
        &mut creation.scratch,
        &world.prefabs,
        &world.models,
        &creation.placed,
        created,
        false,
        &mut ConstructionTool::Inspect,
        &mut creation.committed,
    );
    let entity = edit.applied_entity.filter(|_| accepted)?;
    created.insert(entity, pending.definition);
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

/// The object's authored `build` debit. Keeper food authors it against its
/// own price (`costType="parent"`); other cost forms are not mapped here.
fn authored_build_charge(
    catalogue: WorldDefinitionsView<'_>,
    definition: AssetId,
) -> Option<Money> {
    let object = catalogue.find_object(definition)?;
    let Some(build) = object
        .transactions
        .iter()
        .find(|transaction| transaction.name == AssetId::from_key("build"))
    else {
        return Some(Money::ZERO);
    };
    (build.kind == EconomyTransactionKind::Debit
        && build.cost_basis == EconomyTransactionCostBasis::Parent
        && build.next_transaction.is_none()
        && object.price_cents >= 0)
        .then_some(Money(object.price_cents))
}

fn behavior_placement_supports_definition(definition: &PlaceableDefinition) -> bool {
    !definition.moving_footprint
        && definition.minimum_headroom_metres <= 0.0
        && !definition.constraints.contains_any(
            PlacementConstraints::FLATTEN_TERRAIN_TO_PLACEMENT_HEIGHT
                .with_additional_flags(PlacementConstraints::REQUIRE_PATH)
                .with_additional_flags(PlacementConstraints::REQUIRE_WALL),
        )
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
    position: Vec2,
) -> Option<(f32, bool)> {
    let entity = terrain_chunk_at(&world.terrain_index, position)?;
    let (chunk, edited) = world.terrain_chunks.get(entity).ok()?;
    let point = sample_terrain(
        chunk,
        world.terrain_assets.get(&chunk.asset)?,
        edited,
        position,
    )?;
    Some((point.height_m, point.water_height_m.is_some()))
}
