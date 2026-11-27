//! Commits clothing script output only while its originating action still owns the guest.

pub(super) mod script_context;
pub(super) mod state;

use self::state::PendingClothingDelivery;
use bevy::{
    asset::{AssetLoadFailedEvent, LoadState, RecursiveDependencyLoadState},
    ecs::{entity::EntityHashSet, system::SystemParam},
    platform::collections::HashSet,
    prelude::*,
};
use openzt2_game_assets::material::material_asset_types::MaterialAsset;
use openzt2_game_assets::scene_prefab::ScenePrefabAsset;
use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_world_state::world_spawn::prefab_presentation_types::PrefabClothingTextures;
use openzt2_world_state::world_spawn::prefab_presentation_types::PrefabMaterialOverrides;
use openzt2_world_state::world_spawn::prefab_presentation_types::PrefabPresentation;
use openzt2_zoo_state::behavior_task_execution_types::running_actions::{
    RunBehaviorScript, RunningBehaviorInstruction,
};
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionState;
use openzt2_zoo_state::behavior_task_execution_types::PendingBehaviorTaskFailure;

#[derive(SystemParam)]
pub(super) struct ClothingLoadChanges<'w, 's> {
    image_events: MessageReader<'w, 's, AssetEvent<Image>>,
    image_failures: MessageReader<'w, 's, AssetLoadFailedEvent<Image>>,
    material_events: MessageReader<'w, 's, AssetEvent<MaterialAsset>>,
    material_failures: MessageReader<'w, 's, AssetLoadFailedEvent<MaterialAsset>>,
    prefab_events: MessageReader<'w, 's, AssetEvent<ScenePrefabAsset>>,
    prefab_failures: MessageReader<'w, 's, AssetLoadFailedEvent<ScenePrefabAsset>>,
    structure: Query<
        'w,
        's,
        Entity,
        Or<(
            Changed<Children>,
            Changed<PrefabMaterialOverrides>,
            Changed<PrefabPresentation>,
        )>,
    >,
    parents: Query<'w, 's, &'static ChildOf>,
}

fn changed_asset<A: Asset>(event: &AssetEvent<A>) -> Option<bevy::asset::AssetId<A>> {
    match *event {
        AssetEvent::Added { id }
        | AssetEvent::Modified { id }
        | AssetEvent::LoadedWithDependencies { id } => Some(id),
        _ => None,
    }
}

pub(super) fn cancel_clothing_delivery(
    removed: On<Remove<RunBehaviorScript>>,
    mut commands: Commands,
) {
    commands
        .entity(removed.entity)
        .try_remove::<PendingClothingDelivery>();
}

#[allow(clippy::too_many_arguments)]
pub(super) fn finish_clothing_delivery(
    mut commands: Commands,
    clock: Single<&ZooClock>,
    server: Res<AssetServer>,
    images: Res<Assets<Image>>,
    materials: Res<Assets<MaterialAsset>>,
    mut changed: ClothingLoadChanges,
    actors: Query<(
        Entity,
        Ref<PendingClothingDelivery>,
        Option<(&BehaviorTaskExecutionState, &RunningBehaviorInstruction)>,
        Has<PendingBehaviorTaskFailure>,
    )>,
    children: Query<&Children>,
    presentations: Query<&PrefabPresentation>,
    mut renderables: Query<(
        &PrefabMaterialOverrides,
        Option<&mut PrefabClothingTextures>,
    )>,
    entities: Query<()>,
) {
    let images_changed = changed
        .image_events
        .read()
        .filter_map(changed_asset)
        .chain(changed.image_failures.read().map(|event| event.id))
        .collect::<HashSet<_>>();
    let materials_changed = changed
        .material_events
        .read()
        .filter_map(changed_asset)
        .chain(changed.material_failures.read().map(|event| event.id))
        .collect::<HashSet<_>>();
    let prefabs_changed = changed
        .prefab_events
        .read()
        .filter_map(changed_asset)
        .chain(changed.prefab_failures.read().map(|event| event.id))
        .collect::<HashSet<_>>();
    let mut changed_actors = EntityHashSet::default();
    if !actors.is_empty() {
        for entity in &changed.structure {
            changed_actors.extend(
                std::iter::once(entity)
                    .chain(changed.parents.iter_ancestors::<ChildOf>(entity))
                    .filter(|&owner| actors.contains(owner)),
            );
        }
    }
    for (actor, delivery, task, failed) in &actors {
        let Some((_, instruction)) =
            task.filter(|(task, instruction)| !failed && delivery.owns(task, instruction))
        else {
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
        if !delivery.is_added()
            && !images.is_changed()
            && !materials.is_changed()
            && !delivery
                .textures
                .iter()
                .any(|(_, image)| images_changed.contains(&image.id()))
            && materials_changed.is_empty()
            && !presentations
                .get(actor)
                .is_ok_and(|prefab| prefabs_changed.contains(&prefab.0.id()))
            && !changed_actors.contains(&actor)
        {
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
            let Ok((_, existing)) = renderables.get_mut(root) else {
                continue;
            };
            if let Some(mut existing) = existing {
                if existing.0 != clothing.0 {
                    *existing = clothing;
                }
            } else {
                commands.entity(root).insert(clothing);
            }
        }
        commands.entity(actor).remove::<PendingClothingDelivery>();
        instruction.complete(actor, clock.tick, &mut commands);
    }
}

fn fail_delivery(actor: Entity, commands: &mut Commands) {
    commands.entity(actor).remove::<PendingClothingDelivery>();
    super::behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation(
        actor, commands,
    );
}
