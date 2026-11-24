use bevy::{ecs::entity::EntityHashSet, prelude::*};
use openzt2_game_data::{world_definitions::world_objects::WorldObjectPropertyFlags, AssetId};

use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_zoo_state::economy::facility_economy_types::FacilityProfit;
use openzt2_zoo_state::economy::facility_economy_types::OperatingSinceDay;
use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_world_state::world_spawn::prefab_authored_attachment_identifier::PrefabAuthoredAttachmentIdentifier;
use openzt2_world_state::world_spawn::world_membership_types::DefinitionId;

use super::donation_opportunity_types::{DonationAcceptor, DonationUsePoint};

const DONATION_ACCEPTOR_DOCK_ATTACHMENT_NAME: &str = "adultdock";

#[derive(Component)]
pub(super) struct ProjectedDonationAcceptor;

/// Projects the source-owned acceptor flag and prefab-owned dock attachment
/// onto each ordinary spawned world-object entity.
#[allow(clippy::type_complexity)]
pub(super) fn project_donation_acceptors_and_docking_points(
    mut commands: Commands,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    mut candidates: Query<(
        Entity,
        &DefinitionId,
        &GlobalTransform,
        Has<DonationAcceptor>,
        Option<&mut DonationUsePoint>,
        Has<ProjectedDonationAcceptor>,
    )>,
    changed_candidates: Query<
        Entity,
        (
            With<DefinitionId>,
            Or<(
                Changed<DefinitionId>,
                Added<GlobalTransform>,
                (With<ProjectedDonationAcceptor>, Changed<GlobalTransform>),
            )>,
        ),
    >,
    changed_hierarchy: Query<
        Entity,
        Or<(
            Changed<Children>,
            Changed<PrefabAuthoredAttachmentIdentifier>,
            (
                With<PrefabAuthoredAttachmentIdentifier>,
                Changed<GlobalTransform>,
            ),
        )>,
    >,
    (mut removed_children, mut removed_attachments, mut removed_definitions): (
        RemovedComponents<Children>,
        RemovedComponents<PrefabAuthoredAttachmentIdentifier>,
        RemovedComponents<DefinitionId>,
    ),
    lost_definitions: Query<(), (With<ProjectedDonationAcceptor>, Without<DefinitionId>)>,
    attachments: Query<(&PrefabAuthoredAttachmentIdentifier, &GlobalTransform)>,
    children: Query<&Children>,
    parents: Query<&ChildOf>,
) {
    for entity in removed_definitions
        .read()
        .filter(|&entity| lost_definitions.contains(entity))
    {
        commands.entity(entity).remove::<(
            DonationAcceptor,
            DonationUsePoint,
            ProjectedDonationAcceptor,
        )>();
    }
    let refresh_all = world_definition_assets.is_changed() || active_world_definitions.is_changed();
    let Some(world_definitions) = active_world_definitions.get(&world_definition_assets) else {
        return;
    };
    if world_definitions.has_pending_assets() {
        return;
    }
    let hierarchy = changed_hierarchy
        .iter()
        .chain(removed_children.read())
        .chain(removed_attachments.read());
    let affected: EntityHashSet = if refresh_all {
        candidates.iter().map(|(entity, ..)| entity).collect()
    } else {
        changed_candidates
            .iter()
            .chain(hierarchy.flat_map(|entity| {
                std::iter::once(entity).chain(parents.iter_ancestors::<ChildOf>(entity))
            }))
            .collect()
    };
    let adult_dock = AssetId::from_key(DONATION_ACCEPTOR_DOCK_ATTACHMENT_NAME);
    for affected_entity in affected {
        let Ok((entity, definition, transform, accepts, point, owned)) =
            candidates.get_mut(affected_entity)
        else {
            continue;
        };
        let authored = world_definitions
            .find_object(definition.0)
            .is_some_and(|object| {
                object
                    .properties
                    .contains_all(WorldObjectPropertyFlags::DONATION_ACCEPTOR)
            });
        let dock = authored
            .then(|| {
                children.iter_descendants(entity).find_map(|child| {
                    let (name, pose) = attachments.get(child).ok()?;
                    (name.0 == adult_dock).then_some(pose)
                })
            })
            .flatten();
        let Some(dock_transform) = dock else {
            if owned {
                commands.entity(entity).remove::<(
                    DonationAcceptor,
                    DonationUsePoint,
                    ProjectedDonationAcceptor,
                )>();
            }
            continue;
        };
        let local = dock_transform.reparented_to(transform);
        let next = DonationUsePoint {
            local_point: local.translation,
            local_forward: local.rotation * Vec3::Z,
        };
        if let Some(mut point) = point {
            point.set_if_neq(next);
        } else {
            commands.entity(entity).insert(next);
        }
        if !accepts {
            commands.entity(entity).insert((
                DonationAcceptor { beneficiary: None },
                ProjectedDonationAcceptor,
            ));
        }
    }
}

pub(super) fn initialize_new_donation_acceptors_with_operating_age_and_profit(
    mut commands: Commands,
    clock: If<Single<&ZooClock>>,
    acceptors: Query<Entity, (With<DonationAcceptor>, Without<OperatingSinceDay>)>,
) {
    for acceptor in &acceptors {
        commands
            .entity(acceptor)
            .insert(OperatingSinceDay(clock.absolute_day))
            .insert_if_new(FacilityProfit::default());
    }
}
