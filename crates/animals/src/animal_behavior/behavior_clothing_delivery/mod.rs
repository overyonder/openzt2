//! Commits clothing script output only while its originating action still owns the guest.

pub(super) mod script_context;
pub(super) mod state;

use self::state::PendingClothingDelivery;
use bevy::{
    asset::{LoadState, RecursiveDependencyLoadState},
    prelude::*,
};
use openzt2_game_assets::material::material_asset_types::MaterialAsset;
use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_world_state::world_spawn::prefab_presentation_types::PrefabClothingTextures;
use openzt2_world_state::world_spawn::prefab_presentation_types::PrefabMaterialOverrides;
use openzt2_world_state::world_spawn::prefab_presentation_types::PrefabPresentation;
use openzt2_zoo_state::behavior_task_execution_types::advance_behavior_task_to_next_action;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionState;
use openzt2_zoo_state::behavior_task_execution_types::PendingBehaviorTaskFailure;

pub(super) fn finish_clothing_delivery(
    mut commands: Commands,
    clock: Res<ZooClock>,
    server: Res<AssetServer>,
    images: Res<Assets<Image>>,
    materials: Res<Assets<MaterialAsset>>,
    mut actors: Query<(
        Entity,
        &PendingClothingDelivery,
        Option<&mut BehaviorTaskExecutionState>,
        Has<PendingBehaviorTaskFailure>,
    )>,
    children: Query<&Children>,
    presentations: Query<&PrefabPresentation>,
    renderables: Query<(&PrefabMaterialOverrides, Option<&PrefabClothingTextures>)>,
    entities: Query<()>,
) {
    for (actor, delivery, task, failed) in &mut actors {
        let Some(mut task) = task.filter(|task| !failed && delivery.owns(task)) else {
            commands.entity(actor).remove::<PendingClothingDelivery>();
            continue;
        };
        if delivery
            .target
            .is_some_and(|target| !entities.contains(target))
        {
            fail_delivery(actor, &mut commands);
            continue;
        }
        if delivery.textures.iter().any(|(_, image)| {
            matches!(
                server.get_load_state(image.id()),
                Some(LoadState::Failed(_))
            )
        }) {
            fail_delivery(actor, &mut commands);
            continue;
        }
        if delivery
            .textures
            .iter()
            .any(|(_, image)| images.get(image).is_none())
        {
            continue;
        }
        // The owner's primary prefab is mainObj. Nested prefab presentations
        // belong to named heads, carried objects or other physical objects.
        let mut descendants = vec![actor];
        let mut roots = Vec::new();
        while let Some(entity) = descendants.pop() {
            if entity != actor && presentations.contains(entity) {
                continue;
            }
            if renderables.contains(entity) {
                roots.push(entity);
            }
            if let Ok(children) = children.get(entity) {
                descendants.extend(children.iter());
            }
        }
        if presentations.get(actor).is_ok_and(|prefab| {
            matches!(
                server.get_load_state(prefab.0.id()),
                Some(LoadState::Failed(_))
            ) || matches!(
                server.get_recursive_dependency_load_state(prefab.0.id()),
                Some(RecursiveDependencyLoadState::Failed(_))
            )
        }) {
            fail_delivery(actor, &mut commands);
            continue;
        }
        if roots.is_empty() {
            continue;
        }
        let mut waiting = false;
        let mut invalid = false;
        let mut matched = vec![false; delivery.textures.len()];
        let mut updates = Vec::new();
        for root in roots {
            let Ok((overrides, existing)) = renderables.get(root) else {
                continue;
            };
            let mut clothing = existing.cloned().unwrap_or_default();
            let mut changed = false;
            for material_override in &overrides.0 {
                let Some(handle) = material_override.handle.as_ref() else {
                    waiting = true;
                    continue;
                };
                let Some(material) = materials.get(handle) else {
                    invalid |= matches!(
                        server.get_load_state(handle.id()),
                        Some(LoadState::Failed(_))
                    );
                    waiting = true;
                    continue;
                };
                let Some(binding) = &material.netimmerse_texture_binding else {
                    continue;
                };
                for (index, (name, image)) in delivery.textures.iter().enumerate() {
                    if !binding.name.eq_ignore_ascii_case(name) {
                        continue;
                    }
                    matched[index] = true;
                    clothing
                        .0
                        .retain(|(id, _)| *id != material_override.material);
                    clothing.0.push((material_override.material, image.clone()));
                    changed = true;
                }
            }
            if changed {
                updates.push((root, clothing));
            }
        }
        if invalid {
            fail_delivery(actor, &mut commands);
            continue;
        }
        if waiting {
            continue;
        }
        if matched.contains(&false) {
            fail_delivery(actor, &mut commands);
            continue;
        }
        for (root, clothing) in updates {
            commands.entity(root).insert(clothing);
        }
        advance_behavior_task_to_next_action(&mut task, clock.tick);
        commands.entity(actor).remove::<PendingClothingDelivery>();
    }
}

fn fail_delivery(actor: Entity, commands: &mut Commands) {
    super::behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation(
        actor, commands,
    );
    commands.entity(actor).remove::<PendingClothingDelivery>();
}
