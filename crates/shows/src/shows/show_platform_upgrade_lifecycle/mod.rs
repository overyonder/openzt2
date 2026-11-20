use avian3d::prelude::RigidBody;
use bevy::{ecs::relationship::RelationshipTarget, gltf::Gltf, prelude::*};

use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use openzt2_zoo_state::construction::construction_transaction_types::EditApplication;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentIdAllocator;
use openzt2_world_state::world_spawn::prefab_authored_attachment_identifier::PrefabAuthoredAttachmentIdentifier;
use openzt2_world_state::world_spawn::prefab_model_readiness::first_missing_prefab_collider_model_asset_id;
use openzt2_world::world_spawn::prefab_world_instance_spawning::spawn_loaded_scene_prefab_as_world_instance;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;

use super::show_platform_upgrade_types::{
    CommitShowPlatformUpgradeEdit, InstalledShowPlatformUpgrade, InstalledShowPlatformUpgradeStage,
    PendingShowPlatformUpgradeRestore, RestoreShowPlatformUpgrade, RestoreShowPlatformUpgradeOf,
    ShowPlatformUpgradeEdit, ShowPlatformUpgradeEditAcknowledged, ShowPlatformUpgradeEditOperation,
    ShowPlatformUpgradeKind, ShowPlatformUpgrades,
};
use super::show_stage_types::ShowStage;

pub(super) fn apply_show_platform_upgrade_edits(
    mut commands: Commands,
    mut requests: MessageReader<CommitShowPlatformUpgradeEdit>,
    edits: Query<&ShowPlatformUpgradeEdit>,
    stages: Query<&WorldMember, With<ShowStage>>,
    installed: Query<&InstalledShowPlatformUpgrade>,
    stage_upgrades: Query<&ShowPlatformUpgrades>,
    sockets: Query<&PrefabAuthoredAttachmentIdentifier>,
    children: Query<&Children>,
    prefabs: Res<Assets<openzt2_game_assets::scene_prefab::ScenePrefabAsset>>,
    models: Res<Assets<Gltf>>,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    mut ids: ResMut<PersistentIdAllocator>,
) {
    let world_definitions = active_world_definitions.get(&world_definition_assets);
    for request in requests.read() {
        let Ok(upgrade) = edits.get(request.transaction) else {
            continue;
        };
        let Ok(member) = stages.get(upgrade.stage) else {
            publish_show_platform_upgrade_edit_acknowledgement(&mut commands, request, false);
            continue;
        };
        let Some(world_definitions) = world_definitions else {
            publish_show_platform_upgrade_edit_acknowledgement(&mut commands, request, false);
            continue;
        };
        let existing =
            find_stage_platform_upgrade(upgrade.stage, upgrade.kind, &stage_upgrades, &installed);
        let application_moves_edit_forward = matches!(
            request.application,
            EditApplication::InitialCommit | EditApplication::Redo
        );
        let should_install = matches!(
            (upgrade.operation, application_moves_edit_forward),
            (ShowPlatformUpgradeEditOperation::Install, true)
                | (ShowPlatformUpgradeEditOperation::Remove, false)
        );
        if !should_install {
            if let Some(existing) = existing {
                commands.entity(existing).despawn();
            }
            publish_show_platform_upgrade_edit_acknowledgement(&mut commands, request, true);
            continue;
        }
        if existing.is_some() {
            publish_show_platform_upgrade_edit_acknowledgement(&mut commands, request, true);
            continue;
        }
        let Ok(persistent_id) = ids.allocate(member.root) else {
            publish_show_platform_upgrade_edit_acknowledgement(&mut commands, request, false);
            continue;
        };
        if spawn_show_platform_upgrade_prefab_at_authored_stage_attachment(
            &mut commands,
            upgrade.stage,
            upgrade.kind,
            world_definitions,
            member.root,
            persistent_id,
            &prefabs,
            &models,
            &sockets,
            &children,
        )
        .is_none()
        {
            publish_show_platform_upgrade_edit_acknowledgement(&mut commands, request, false);
            continue;
        }
        publish_show_platform_upgrade_edit_acknowledgement(&mut commands, request, true);
    }
}

pub(super) fn queue_show_platform_upgrade_restores(
    mut commands: Commands,
    mut requests: MessageReader<RestoreShowPlatformUpgrade>,
) {
    for request in requests.read() {
        commands.spawn((
            RestoreShowPlatformUpgradeOf(request.stage),
            PendingShowPlatformUpgradeRestore {
                kind: request.kind,
                persistent_id: request.persistent_id,
            },
        ));
    }
}

pub(super) fn restore_show_platform_upgrades(
    mut commands: Commands,
    pending: Query<(
        Entity,
        &RestoreShowPlatformUpgradeOf,
        &PendingShowPlatformUpgradeRestore,
    )>,
    stages: Query<&WorldMember, With<ShowStage>>,
    installed: Query<&InstalledShowPlatformUpgrade>,
    stage_upgrades: Query<&ShowPlatformUpgrades>,
    sockets: Query<&PrefabAuthoredAttachmentIdentifier>,
    children: Query<&Children>,
    prefabs: Res<Assets<openzt2_game_assets::scene_prefab::ScenePrefabAsset>>,
    models: Res<Assets<Gltf>>,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
) {
    let Some(world_definitions) = active_world_definitions.get(&world_definition_assets) else {
        return;
    };
    for (operation, owner, request) in &pending {
        let stage = owner.0;
        if find_stage_platform_upgrade(stage, request.kind, &stage_upgrades, &installed).is_some() {
            commands.entity(operation).despawn();
            continue;
        }
        let Ok(member) = stages.get(stage) else {
            continue;
        };
        if spawn_show_platform_upgrade_prefab_at_authored_stage_attachment(
            &mut commands,
            stage,
            request.kind,
            world_definitions,
            member.root,
            request.persistent_id,
            &prefabs,
            &models,
            &sockets,
            &children,
        )
        .is_some()
        {
            commands.entity(operation).despawn();
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_show_platform_upgrade_prefab_at_authored_stage_attachment(
    commands: &mut Commands,
    stage: Entity,
    kind: ShowPlatformUpgradeKind,
    world_definitions: WorldDefinitionsView<'_>,
    world_root: Entity,
    persistent_id: openzt2_world_state::world_spawn::persistent_id_types::PersistentId,
    prefabs: &Assets<openzt2_game_assets::scene_prefab::ScenePrefabAsset>,
    models: &Assets<Gltf>,
    sockets: &Query<&PrefabAuthoredAttachmentIdentifier>,
    children: &Query<&Children>,
) -> Option<Entity> {
    let platform_upgrade_policy = world_definitions.show_platform_upgrades()?;
    let (object_definition, stage_attachment) = match kind {
        ShowPlatformUpgradeKind::Canopy => (
            platform_upgrade_policy.canopy_definition,
            platform_upgrade_policy.canopy_attachment,
        ),
        ShowPlatformUpgradeKind::Television => (
            platform_upgrade_policy.television_definition,
            platform_upgrade_policy.television_attachment,
        ),
    };
    let object = world_definitions.find_object(object_definition)?;
    let scene_prefab = world_definitions.scene(object.prefab)?;
    let prefab = prefabs.get(&scene_prefab)?;
    if first_missing_prefab_collider_model_asset_id(prefab, models).is_some() {
        return None;
    }
    let parent = if stage_attachment == openzt2_game_data::AssetId::default() {
        stage
    } else {
        children
            .iter_descendants::<Children>(stage)
            .find(|&entity| {
                sockets
                    .get(entity)
                    .is_ok_and(|socket| socket.0 == stage_attachment)
            })?
    };
    let child = spawn_loaded_scene_prefab_as_world_instance(
        commands,
        prefab,
        scene_prefab,
        world_root,
        object_definition,
        persistent_id,
        Transform::IDENTITY,
        true,
        Some(parent),
        RigidBody::Static,
    );
    commands.entity(child).insert((
        InstalledShowPlatformUpgrade(kind),
        InstalledShowPlatformUpgradeStage(stage),
    ));
    Some(child)
}

pub(super) fn find_stage_platform_upgrade(
    stage: Entity,
    kind: ShowPlatformUpgradeKind,
    stages: &Query<&ShowPlatformUpgrades>,
    installed: &Query<&InstalledShowPlatformUpgrade>,
) -> Option<Entity> {
    stages.get(stage).ok()?.iter().find(|&entity| {
        installed
            .get(entity)
            .is_ok_and(|installed| installed.0 == kind)
    })
}

fn publish_show_platform_upgrade_edit_acknowledgement(
    commands: &mut Commands,
    request: &CommitShowPlatformUpgradeEdit,
    accepted: bool,
) {
    commands.write_message(ShowPlatformUpgradeEditAcknowledged {
        transaction: request.transaction,
        application: request.application,
        accepted,
    });
}
