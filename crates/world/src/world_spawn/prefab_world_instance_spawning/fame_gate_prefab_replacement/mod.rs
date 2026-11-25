use avian3d::prelude::{Collider, CollisionEventsEnabled, CollisionLayers, RigidBody, SweptCcd};
use bevy::{
    animation::AnimatedBy,
    asset::LoadState,
    ecs::{
        entity::EntityHashSet,
        query::Allow,
        relationship::RelationshipTarget,
        schedule::common_conditions::{any_match_filter, on_message},
    },
    gltf::Gltf,
    prelude::*,
};
use openzt2_game_assets::scene_prefab::transform_animation_lowering::{
    PrefabMotionRotation, PrefabTransformClocks,
};
use openzt2_game_assets::world_definitions::{
    world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
    world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
};
use openzt2_world_state::application_schedule::GameSet;
use openzt2_world_state::world_spawn::{
    prefab_authored_rotation_cycle_advancement::PrefabRealTimeRotationClock,
    prefab_model_readiness::first_missing_prefab_collider_model_asset_id,
    prefab_object_presentation_attachment_projection::PrefabObjectPresentationAttachmentProjectionHydrated,
    prefab_presentation_types::PrefabSkinRenderBound,
    zoo_entrance_anchor_synchronization::ZooEntrance,
};
use openzt2_zoo_state::progression::fame_types::{
    FameGateAppearanceRequested, PendingFameGatePrefab,
};

use super::super::{
    named_physical_presentation_projection::{
        NamedPhysicalPresentations, NamedPhysicalPresentationsHydrated,
        NamedPresentationNodeAttachment,
    },
    physical_presentation_state::ObjectPhysicalPresentationControllers,
};
use super::*;

pub(super) fn register_fame_gate_prefab_replacement(app: &mut App) {
    use super::super::{
        named_physical_presentation_projection::hydrate_required_named_physical_presentations,
        prefab_object_presentation_attachment_projection::initialize_authored_physical_presentation_controllers,
        zoo_entrance_anchor_synchronization::synchronize_zoo_entrance_anchor_after_transform_changes,
    };
    app.add_message::<FameGateAppearanceRequested>()
        .add_systems(
            Update,
            (
                request_fame_gate_prefab_replacement
                    .run_if(on_message::<FameGateAppearanceRequested>),
                replace_loaded_fame_gate_prefabs
                    .run_if(any_match_filter::<(With<PendingFameGatePrefab>, Allow<Disabled>)>)
                    .run_if(
                        any_match_filter::<(Changed<PendingFameGatePrefab>, Allow<Disabled>)>
                            .or_eager(on_message::<AssetEvent<ScenePrefabAsset>>)
                            .or_eager(on_message::<AssetEvent<Gltf>>)
                            .or_eager(on_message::<AssetLoadFailedEvent<ScenePrefabAsset>>)
                            .or_eager(on_message::<AssetLoadFailedEvent<Gltf>>)
                            .or_eager(|mut released: RemovedComponents<ModelExpanded>| {
                                released.read().count() != 0
                            }),
                    )
                    .before(initialize_authored_physical_presentation_controllers)
                    .before(hydrate_required_named_physical_presentations)
                    .before(synchronize_zoo_entrance_anchor_after_transform_changes),
            )
                .chain()
                .in_set(GameSet::Intent),
        );
}

fn request_fame_gate_prefab_replacement(
    mut requests: MessageReader<FameGateAppearanceRequested>,
    mut commands: Commands,
    definitions: Res<WorldDefinitions>,
    definition_assets: Res<Assets<WorldDefinitionAsset>>,
    mut gates: Query<
        (
            &DefinitionId,
            Option<&mut PendingFameGatePrefab>,
            Option<&mut PrefabModel>,
        ),
        (With<ZooEntrance>, Allow<Disabled>),
    >,
) {
    let definitions = definitions.get(&definition_assets);
    let mut seen = EntityHashSet::default();
    let requests = requests.read().collect::<Vec<_>>();
    for request in requests.into_iter().rev() {
        if !seen.insert(request.gate) {
            continue;
        }
        let Ok((current, pending, model)) = gates.get_mut(request.gate) else {
            continue;
        };
        if current.0 == request.definition {
            if let Some(pending) = pending {
                cancel_fame_gate_prefab_replacement(&mut commands, request.gate, &pending, model);
            }
            continue;
        }
        if pending
            .as_ref()
            .is_some_and(|pending| pending.definition == request.definition)
        {
            continue;
        }
        let Some(prefab) = definitions.and_then(|definitions| {
            definitions
                .find_object(request.definition)
                .and_then(|object| definitions.scene(object.prefab))
        }) else {
            continue;
        };
        let replacement = PendingFameGatePrefab {
            definition: request.definition,
            prefab,
            releasing_root_renderable: pending
                .as_ref()
                .is_some_and(|pending| pending.releasing_root_renderable),
        };
        if let Some(mut pending) = pending {
            pending.set_if_neq(replacement);
        } else {
            commands.entity(request.gate).try_insert(replacement);
        }
    }
}

fn cancel_fame_gate_prefab_replacement(
    commands: &mut Commands,
    gate: Entity,
    pending: &PendingFameGatePrefab,
    model: Option<Mut<PrefabModel>>,
) {
    let mut root = commands.entity(gate);
    root.remove::<PendingFameGatePrefab>();
    if pending.releasing_root_renderable {
        root.remove::<ReleasePrefabRenderable>();
        if let Some(mut model) = model {
            model.set_changed();
        }
    }
}

#[derive(bevy::ecs::query::QueryData)]
#[query_data(mutable)]
struct FameGatePrefabRoot {
    entity: Entity,
    pending: &'static mut PendingFameGatePrefab,
    transform: &'static mut Transform,
    visibility: &'static mut Visibility,
    member: &'static WorldMember,
    definition: &'static mut DefinitionId,
    inspectable: &'static mut Inspectable,
    attachment: &'static mut PrefabAuthoredAttachmentIdentifier,
    projection: &'static mut PrefabObjectPresentationAttachmentProjection,
    source: &'static mut PrefabSourceAssetHandle,
    rows: Option<&'static PrefabInstanceRows>,
    children: Option<&'static Children>,
    named: Option<&'static NamedPhysicalPresentations>,
    controllers: Option<&'static ObjectPhysicalPresentationControllers>,
    model: Option<&'static mut PrefabModel>,
    expanded: Has<ModelExpanded>,
    released: Has<ReleasePrefabRenderable>,
}

type AuxiliaryFameGatePrefabChild = (
    Or<(
        With<PrefabRowRenderableOf>,
        With<super::super::prefab_effect_trigger_hydration::PrefabEffectTrigger>,
        With<ModelRenderVisibilityGate>,
        With<Collider>,
        With<NamedPresentationNodeAttachment>,
    )>,
    Allow<Disabled>,
);

fn replace_loaded_fame_gate_prefabs(
    mut commands: Commands,
    mut gates: Query<FameGatePrefabRoot, (With<ZooEntrance>, Allow<Disabled>)>,
    prefabs: Res<Assets<ScenePrefabAsset>>,
    models: Res<Assets<Gltf>>,
    server: Res<AssetServer>,
    auxiliary_root_children: Query<(), AuxiliaryFameGatePrefabChild>,
) {
    for mut gate in &mut gates {
        let mut root = commands.entity(gate.entity);
        let Some(prefab) = prefabs.get(&gate.pending.prefab) else {
            if matches!(
                server.load_state(gate.pending.prefab.id()),
                LoadState::Failed(_)
            ) {
                cancel_fame_gate_prefab_replacement(
                    &mut commands,
                    gate.entity,
                    &gate.pending,
                    gate.model,
                );
            }
            continue;
        };
        if let Some(model) = first_missing_prefab_collider_model_asset_id(prefab, &models) {
            if prefab
                .loaded_model_asset_handle(model)
                .is_none_or(|handle| matches!(server.load_state(handle.id()), LoadState::Failed(_)))
            {
                cancel_fame_gate_prefab_replacement(
                    &mut commands,
                    gate.entity,
                    &gate.pending,
                    gate.model,
                );
            }
            continue;
        }
        let Some(old_prefab) = prefabs.get(&gate.source.0) else {
            continue;
        };
        let Some(old_row) = old_prefab
            .canonical_scene_prefab_document()
            .entities
            .first()
        else {
            cancel_fame_gate_prefab_replacement(
                &mut commands,
                gate.entity,
                &gate.pending,
                gate.model,
            );
            continue;
        };
        let Some(new_row) = prefab.canonical_scene_prefab_document().entities.first() else {
            cancel_fame_gate_prefab_replacement(
                &mut commands,
                gate.entity,
                &gate.pending,
                gate.model,
            );
            continue;
        };
        if gate.expanded {
            if !gate.released {
                root.insert(ReleasePrefabRenderable { despawn: false });
                gate.pending.releasing_root_renderable = true;
            }
            continue;
        }

        let instance = prefab_instance_transform(*gate.transform, old_row);
        let visible = *gate.visibility != Visibility::Hidden
            || old_row.flags.0 & ScenePrefabEntityFlags::VISIBLE.0 == 0;
        *gate.transform = instance.mul_transform(transform_from_authored(&new_row.transform));
        *gate.visibility = if visible && new_row.flags.0 & ScenePrefabEntityFlags::VISIBLE.0 != 0 {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        gate.definition.0 = gate.pending.definition;
        gate.inspectable.definition = gate.pending.definition;
        gate.attachment.0 = AssetId(new_row.attachment_id.0);
        gate.projection.0 = gate.pending.definition;
        gate.source.0 = gate.pending.prefab.clone();

        retire_fame_gate_prefab(
            &mut commands,
            &gate,
            old_prefab,
            old_row,
            new_row,
            &auxiliary_root_children,
        );
        hydrate_world_instance_prefab_rows(
            &mut commands,
            gate.entity,
            prefab,
            gate.member.root,
            instance,
            visible,
            RigidBody::Static,
            None,
        );
    }
}

fn retire_fame_gate_prefab(
    commands: &mut Commands,
    gate: &FameGatePrefabRootItem<'_, '_>,
    old_prefab: &ScenePrefabAsset,
    old_row: &PrefabEntity,
    new_row: &PrefabEntity,
    auxiliary_root_children: &Query<(), AuxiliaryFameGatePrefabChild>,
) {
    let mut root = commands.entity(gate.entity);
    root.remove::<(
        PendingFameGatePrefab,
        PrefabLocalRenderBound,
        PrefabSkinRenderBound,
        PrefabAuthoredBillboardOrientationMode,
        ReleasePrefabRenderable,
        NamedPhysicalPresentationsHydrated,
        PrefabObjectPresentationAttachmentProjectionHydrated,
    )>();
    if gate.model.is_some() {
        root.remove::<(
            PrefabRenderable,
            PrefabModel,
            PrefabMaterialOverrides,
            PrefabShadowPolicy,
        )>();
    }
    if old_prefab.motion_for_row(0).is_some() {
        root.remove::<(
            AnimationPlayer,
            AnimationGraphHandle,
            bevy::animation::AnimationTargetId,
            AnimatedBy,
            PrefabMotionRotation,
            PrefabRealTimeRotationClock,
            PrefabTransformClocks,
        )>();
    }
    if !old_row.colliders.is_empty() {
        root.remove::<(
            Collider,
            CollisionLayers,
            CollisionEventsEnabled,
            RigidBody,
            SweptCcd,
        )>();
    }
    if new_row.flags.0 & ScenePrefabEntityFlags::ACTIVE.0 != 0 {
        root.remove::<Disabled>();
    }

    // Retire prefab-owned children while retaining unrelated children of the gate.
    let prefab_children = gate
        .rows
        .into_iter()
        .flat_map(RelationshipTarget::iter)
        .chain(gate.named.into_iter().flat_map(RelationshipTarget::iter))
        .chain(
            gate.controllers
                .into_iter()
                .flat_map(RelationshipTarget::iter),
        )
        .chain(
            gate.children
                .into_iter()
                .flat_map(Children::iter)
                .filter(|&child| auxiliary_root_children.contains(child)),
        );
    for child in prefab_children {
        commands.entity(child).try_despawn();
    }
}
