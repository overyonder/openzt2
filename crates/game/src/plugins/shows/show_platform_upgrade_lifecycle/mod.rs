use avian3d::prelude::RigidBody;
use bevy::{gltf::Gltf, prelude::*};

use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use crate::plugins::{
    construction::construction_transaction_types::EditApplication,
    world_spawn::{
        persistent_id_types::PersistentIdAllocator,
        prefab_authored_attachment_identifier::PrefabAuthoredAttachmentIdentifier,
        prefab_model_readiness::first_missing_prefab_collider_model_asset_id,
        prefab_world_instance_spawning::spawn_loaded_scene_prefab_as_world_instance,
        world_membership_types::WorldMember,
    },
};

use super::show_platform_upgrade_types::{
    CommitShowPlatformUpgradeEdit, InstalledShowPlatformUpgrade, InstalledShowPlatformUpgradeStage,
    PendingShowPlatformUpgradeRestore, RestoreShowPlatformUpgrade, ShowPlatformUpgradeEdit,
    ShowPlatformUpgradeEditAcknowledged, ShowPlatformUpgradeEditOperation, ShowPlatformUpgradeKind,
};
use super::show_stage_types::ShowStage;

pub(super) fn apply_show_platform_upgrade_edits(
    mut commands: Commands,
    mut requests: MessageReader<CommitShowPlatformUpgradeEdit>,
    edits: Query<&ShowPlatformUpgradeEdit>,
    stages: Query<&WorldMember, With<ShowStage>>,
    installed: Query<(
        Entity,
        &InstalledShowPlatformUpgrade,
        &InstalledShowPlatformUpgradeStage,
    )>,
    sockets: Query<(
        Entity,
        &PrefabAuthoredAttachmentIdentifier,
        Option<&ChildOf>,
    )>,
    prefabs: Res<Assets<crate::assets::scene_prefab::ScenePrefabAsset>>,
    models: Res<Assets<Gltf>>,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    mut ids: ResMut<PersistentIdAllocator>,
    mut acknowledged: MessageWriter<ShowPlatformUpgradeEditAcknowledged>,
) {
    let world_definitions = active_world_definitions.get(&world_definition_assets);
    for request in requests.read() {
        let Ok(upgrade) = edits.get(request.transaction) else {
            continue;
        };
        let Ok(member) = stages.get(upgrade.stage) else {
            publish_show_platform_upgrade_edit_acknowledgement(&mut acknowledged, request, false);
            continue;
        };
        let Some(world_definitions) = world_definitions else {
            publish_show_platform_upgrade_edit_acknowledgement(&mut acknowledged, request, false);
            continue;
        };
        let existing = installed.iter().find_map(|(entity, kind, owner)| {
            (kind.0 == upgrade.kind && owner.0 == upgrade.stage).then_some(entity)
        });
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
            publish_show_platform_upgrade_edit_acknowledgement(&mut acknowledged, request, true);
            continue;
        }
        if existing.is_some() {
            publish_show_platform_upgrade_edit_acknowledgement(&mut acknowledged, request, true);
            continue;
        }
        let Ok(persistent_id) = ids.allocate(member.root) else {
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
        )
        .is_none()
        {
            publish_show_platform_upgrade_edit_acknowledgement(&mut acknowledged, request, false);
            continue;
        }
        publish_show_platform_upgrade_edit_acknowledgement(&mut acknowledged, request, true);
    }
}

pub(super) fn queue_show_platform_upgrade_restores(
    mut commands: Commands,
    mut requests: MessageReader<RestoreShowPlatformUpgrade>,
) {
    for request in requests.read() {
        commands.spawn(PendingShowPlatformUpgradeRestore {
            stage: request.stage,
            kind: request.kind,
            persistent_id: request.persistent_id,
        });
    }
}

pub(super) fn restore_show_platform_upgrades(
    mut commands: Commands,
    pending: Query<(Entity, &PendingShowPlatformUpgradeRestore)>,
    stages: Query<&WorldMember, With<ShowStage>>,
    installed: Query<(
        &InstalledShowPlatformUpgrade,
        &InstalledShowPlatformUpgradeStage,
    )>,
    sockets: Query<(
        Entity,
        &PrefabAuthoredAttachmentIdentifier,
        Option<&ChildOf>,
    )>,
    prefabs: Res<Assets<crate::assets::scene_prefab::ScenePrefabAsset>>,
    models: Res<Assets<Gltf>>,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
) {
    let Some(world_definitions) = active_world_definitions.get(&world_definition_assets) else {
        return;
    };
    for (operation, request) in &pending {
        if installed
            .iter()
            .any(|(kind, owner)| kind.0 == request.kind && owner.0 == request.stage)
        {
            commands.entity(operation).despawn();
            continue;
        }
        let Ok(member) = stages.get(request.stage) else {
            continue;
        };
        commands.entity(operation).insert(ChildOf(request.stage));
        if spawn_show_platform_upgrade_prefab_at_authored_stage_attachment(
            &mut commands,
            request.stage,
            request.kind,
            world_definitions,
            member.root,
            request.persistent_id,
            &prefabs,
            &models,
            &sockets,
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
    persistent_id: crate::plugins::world_spawn::persistent_id_types::PersistentId,
    prefabs: &Assets<crate::assets::scene_prefab::ScenePrefabAsset>,
    models: &Assets<Gltf>,
    sockets: &Query<(
        Entity,
        &PrefabAuthoredAttachmentIdentifier,
        Option<&ChildOf>,
    )>,
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
        sockets.iter().find_map(|(entity, socket, _)| {
            (socket.0 == stage_attachment
                && prefab_attachment_is_descendant_of_show_stage(entity, stage, sockets))
            .then_some(entity)
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

fn prefab_attachment_is_descendant_of_show_stage(
    mut entity: Entity,
    ancestor: Entity,
    hierarchy: &Query<(
        Entity,
        &PrefabAuthoredAttachmentIdentifier,
        Option<&ChildOf>,
    )>,
) -> bool {
    loop {
        if entity == ancestor {
            return true;
        }
        let Ok((_, _, Some(parent))) = hierarchy.get(entity) else {
            return false;
        };
        entity = parent.parent();
    }
}

fn publish_show_platform_upgrade_edit_acknowledgement(
    acknowledged: &mut MessageWriter<ShowPlatformUpgradeEditAcknowledged>,
    request: &CommitShowPlatformUpgradeEdit,
    accepted: bool,
) {
    acknowledged.write(ShowPlatformUpgradeEditAcknowledged {
        transaction: request.transaction,
        application: request.application,
        accepted,
    });
}
