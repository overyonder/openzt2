//! A building a rampaging animal smashes is removed, and its authored ruin
//! (`entityRuinBinderName`) is put down where it stood.

use bevy::asset::{AssetLoadFailedEvent, LoadState};
use bevy::ecs::{entity::EntityHashSet, system::SystemParam};
use bevy::gltf::Gltf;
use bevy::prelude::*;
use openzt2_zoo_state::placement::object_placement_geometry::{ScenePlacementFootprintChanged, ScenePlacementFootprints};
use openzt2_world_state::world_spawn::prefab_model_readiness::first_missing_prefab_collider_model_asset_id;
use openzt2_game_assets::effect::SpawnCompleteParticleEffectDocument;
use openzt2_game_assets::scene_prefab::ScenePrefabAsset;
use openzt2_game_data::AssetId;
use openzt2_game_data::scene_prefab::PrefabColliderSource;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::{WorldDefinitions, WorldDefinitionsView};
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_world_state::world_spawn::persistent_id_types::{PersistentId, PersistentIdAllocator};
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::audio::audio_playback_message_types::PlayPositionedAudioCue;
use openzt2_zoo_state::construction::construction_tool_and_placement_policy_types::ConstructionTool;
use openzt2_zoo_state::placement::object_placement_definition_queries::{
    collect_occupied_placement_cells_for_transform, resolve_object_placeable_definition,
};
use openzt2_zoo_state::placement::placed_object_types::{
    PlacedObjectDefinitionReference, PlacedObjectFootprintOccupancy,
};
use openzt2_zoo_state::placement::placement_occupancy_index::PlacedObjectFootprintOccupancyIndex;
use openzt2_zoo_state::placement::placement_preview_types::ObjectPlacementCellCollectionScratch;
use openzt2_zoo_state::placement::placement_transaction_types::{
    DestroyPlacedObject, ObjectPlacementCommitted, PreparedObjectPlacementEditMutationKind,
    PreparedObjectPlacementMutation,
};

use crate::placement::placed_object_creation_edit_application::apply_prepared_placed_object_creation_edit;

type PlacedObjects<'w, 's> = Query<
    'w,
    's,
    (
        &'static PersistentId,
        &'static PlacedObjectDefinitionReference,
        &'static Transform,
        &'static PlacedObjectFootprintOccupancy,
    ),
>;

#[derive(Component)]
pub(super) struct BuildingDestructionRequest(usize);

#[derive(Component)]
pub(super) struct BuildingDestructionAssets {
    ruin: Option<(AssetId, Handle<ScenePrefabAsset>)>,
    rubble: Option<Handle<ScenePrefabAsset>>,
}

#[derive(Component)]
pub(super) struct BuildingDestructionReady;

#[derive(SystemParam)]
pub(super) struct DestructionAssetState<'w> {
    server: Res<'w, AssetServer>,
    prefabs: Res<'w, Assets<ScenePrefabAsset>>,
    models: Res<'w, Assets<Gltf>>,
    footprints: Res<'w, ScenePlacementFootprints>,
}

pub(super) fn mark_buildings_requested_for_destruction(
    mut commands: Commands,
    mut requests: MessageReader<DestroyPlacedObject>,
    buildings: Query<
        (),
        (
            With<PlacedObjectFootprintOccupancy>,
            Without<BuildingDestructionRequest>,
        ),
    >,
) {
    let mut seen = EntityHashSet::default();
    for (request, id) in requests.read_with_id() {
        if buildings.contains(request.entity) && seen.insert(request.entity) {
            commands
                .entity(request.entity)
                .insert(BuildingDestructionRequest(id.id));
        }
    }
}

pub(super) fn resolve_requested_building_destruction_assets(
    mut commands: Commands,
    definitions: Res<WorldDefinitions>,
    world_assets: Res<Assets<WorldDefinitionAsset>>,
    asset_state: DestructionAssetState,
    buildings: Query<
        (Entity, &PlacedObjectDefinitionReference),
        (
            With<BuildingDestructionRequest>,
            Without<BuildingDestructionAssets>,
        ),
    >,
) {
    let Some(catalogue) = definitions.get(&world_assets) else {
        return;
    };
    for (building, reference) in &buildings {
        let destruction = catalogue
            .find_object(reference.definition)
            .and_then(|object| object.destruction.as_ref());
        let ruin = destruction.and_then(|destruction| {
            let placeable =
                |ruin: &AssetId| resolve_object_placeable_definition(catalogue, *ruin).is_some();
            let ruin = if placeable(&destruction.ruin) {
                destruction.ruin
            } else {
                let mut kinds = catalogue
                    .objects()
                    .filter(|object| object.behavior_type_identifiers.contains(&destruction.ruin))
                    .map(|object| object.id)
                    .filter(placeable)
                    .collect::<Vec<_>>();
                kinds.sort_unstable();
                *kinds.get(building.to_bits() as usize % kinds.len().max(1))?
            };
            Some((ruin, catalogue.scene(catalogue.find_object(ruin)?.prefab)?))
        });
        let rubble = destruction.and_then(|destruction| {
            catalogue.scene(catalogue.find_object(destruction.rubble)?.prefab)
        });
        let assets = BuildingDestructionAssets { ruin, rubble };
        let mut entity = commands.entity(building);
        if destruction_assets_are_ready(&assets, &asset_state, catalogue) {
            entity.insert(BuildingDestructionReady);
        }
        entity.insert(assets);
    }
}

fn destruction_assets_are_ready(
    assets: &BuildingDestructionAssets,
    state: &DestructionAssetState,
    catalogue: WorldDefinitionsView<'_>,
) -> bool {
    let prefab_finished = |handle: &Handle<ScenePrefabAsset>| {
        state.prefabs.contains(handle.id())
            || matches!(
                state.server.get_load_state(handle.id()),
                Some(LoadState::Failed(_))
            )
    };
    if assets
        .rubble
        .as_ref()
        .is_some_and(|handle| !prefab_finished(handle))
    {
        return false;
    }
    let Some((ruin, handle)) = &assets.ruin else {
        return true;
    };
    let Some(prefab) = state.prefabs.get(handle) else {
        return prefab_finished(handle);
    };
    if let Some(model) = first_missing_prefab_collider_model_asset_id(prefab, &state.models)
        .and_then(|model| prefab.loaded_model_asset_handle(model))
    {
        return matches!(
            state.server.get_load_state(model.id()),
            Some(LoadState::Failed(_))
        );
    }
    prefab
        .canonical_scene_prefab_document()
        .automatic_placement_bounds_xz
        .is_none()
        || !resolve_object_placeable_definition(catalogue, *ruin)
            .is_some_and(|placeable| placeable.automatic_footprint)
        || state.footprints.has_scene_bounds(handle)
}

pub(super) fn mark_waiting_building_destruction_assets_ready(
    mut commands: Commands,
    definitions: Res<WorldDefinitions>,
    world_assets: Res<Assets<WorldDefinitionAsset>>,
    asset_state: DestructionAssetState,
    mut prefab_events: MessageReader<AssetEvent<ScenePrefabAsset>>,
    mut prefab_failures: MessageReader<AssetLoadFailedEvent<ScenePrefabAsset>>,
    mut model_events: MessageReader<AssetEvent<Gltf>>,
    mut model_failures: MessageReader<AssetLoadFailedEvent<Gltf>>,
    mut footprint_events: MessageReader<ScenePlacementFootprintChanged>,
    buildings: Query<(Entity, &BuildingDestructionAssets), Without<BuildingDestructionReady>>,
) {
    let definitions_changed = definitions.is_changed();
    let mut prefabs = prefab_events
        .read()
        .filter_map(|event| match event {
            AssetEvent::Added { id }
            | AssetEvent::Modified { id }
            | AssetEvent::LoadedWithDependencies { id } => Some(*id),
            _ => None,
        })
        .chain(prefab_failures.read().map(|event| event.id))
        .collect::<bevy::platform::collections::HashSet<_>>();
    prefabs.extend(footprint_events.read().map(|event| event.0));
    let models = model_events
        .read()
        .filter_map(|event| match event {
            AssetEvent::Added { id }
            | AssetEvent::Modified { id }
            | AssetEvent::LoadedWithDependencies { id } => Some(*id),
            _ => None,
        })
        .chain(model_failures.read().map(|event| event.id))
        .collect::<bevy::platform::collections::HashSet<_>>();
    if !definitions_changed && prefabs.is_empty() && models.is_empty() {
        return;
    }
    let Some(catalogue) = definitions.get(&world_assets) else {
        return;
    };
    for (building, assets) in &buildings {
        let ruin_changed = assets.ruin.as_ref().is_some_and(|(_, handle)| {
            prefabs.contains(&handle.id())
                || asset_state.prefabs.get(handle).is_some_and(|prefab| {
                    prefab
                        .canonical_scene_prefab_document()
                        .entities
                        .iter()
                        .flat_map(|entity| &entity.colliders)
                        .filter_map(|collider| match &collider.source {
                            PrefabColliderSource::Model { model, .. } => Some(AssetId(model.0)),
                            _ => None,
                        })
                        .filter_map(|model| prefab.loaded_model_asset_handle(model))
                        .any(|handle| models.contains(&handle.id()))
                })
        });
        let rubble_changed = assets
            .rubble
            .as_ref()
            .is_some_and(|handle| prefabs.contains(&handle.id()));
        if (definitions_changed || ruin_changed || rubble_changed)
            && destruction_assets_are_ready(assets, &asset_state, catalogue)
        {
            commands.entity(building).insert(BuildingDestructionReady);
        }
    }
}

pub(super) fn ruin_destroyed_placed_objects(
    mut commands: Commands,
    ready: Query<
        (
            Entity,
            &BuildingDestructionRequest,
            &BuildingDestructionAssets,
        ),
        With<BuildingDestructionReady>,
    >,
) {
    let mut ready = ready.iter().collect::<Vec<_>>();
    ready.sort_unstable_by_key(|(_, request, _)| request.0);
    for (building, _, _) in ready {
        commands.run_system_cached_with(ruin_destroyed_placed_object, building);
    }
}

fn ruin_destroyed_placed_object(
    In(building): In<Entity>,
    mut commands: Commands,
    ready: Query<&BuildingDestructionAssets, With<BuildingDestructionReady>>,
    world_assets: Res<Assets<WorldDefinitionAsset>>,
    world_definitions: Res<WorldDefinitions>,
    server: Res<AssetServer>,
    (prefabs, models, footprints): (
        Res<Assets<ScenePrefabAsset>>,
        Res<Assets<Gltf>>,
        Res<ScenePlacementFootprints>,
    ),
    members: Query<&WorldMember>,
    (placement, mut scratch, mut allocator): (
        Res<PlacedObjectFootprintOccupancyIndex>,
        ResMut<ObjectPlacementCellCollectionScratch>,
        ResMut<PersistentIdAllocator>,
    ),
    placed: PlacedObjects,
    mut committed: MessageWriter<ObjectPlacementCommitted>,
    (mut effects, mut sounds): (
        MessageWriter<SpawnCompleteParticleEffectDocument>,
        MessageWriter<PlayPositionedAudioCue>,
    ),
) {
    let Ok(assets) = ready.get(building) else {
        return;
    };
    let Some(catalogue) = world_definitions.get(&world_assets) else {
        return;
    };
    let (Ok((_, reference, transform, _)), Ok(member)) =
        (placed.get(building), members.get(building))
    else {
        commands.entity(building).remove::<(
            BuildingDestructionRequest,
            BuildingDestructionAssets,
            BuildingDestructionReady,
        )>();
        return;
    };
    let (definition, transform, root) = (reference.definition, *transform, member.root);
    if resolve_object_placeable_definition(catalogue, definition).is_none() {
        commands.entity(building).remove::<(
            BuildingDestructionRequest,
            BuildingDestructionAssets,
            BuildingDestructionReady,
        )>();
        return;
    }
    let ruin = assets
        .ruin
        .as_ref()
        .filter(|(_, handle)| prefabs.contains(handle.id()))
        .cloned();
    commands.entity(building).try_despawn();
    if let Some(destruction) = catalogue
        .find_object(definition)
        .and_then(|object| object.destruction.as_ref())
    {
        let fragments = assets
            .rubble
            .as_ref()
            .and_then(|handle| prefabs.get(handle));
        debug!(?building, fragments = destruction.rubble_count, loaded = fragments.is_some(),
                effect = %destruction.effect_path, cue = ?destruction.audio_cue,
                "destroyed object's effect, sound and fragments");
        if let Some(prefab) = fragments {
            throw_fragments(
                &mut commands,
                prefab,
                destruction,
                &transform,
                building.to_bits(),
            );
        }
        effects.write(SpawnCompleteParticleEffectDocument {
            particle_effect_asset: server.load(destruction.effect_path.clone()),
            parent_entity: None,
            inherit_parent_rotation: false,
            effect_transform: Transform::from_translation(transform.translation),
            manual_particle_count: 0,
        });
        sounds.write(PlayPositionedAudioCue {
            cue: destruction.audio_cue,
            world_position: transform.translation,
            selection: building.to_bits(),
            priority: 192,
            force_looped: false,
            listener_above_water: true,
            listener_below_water: true,
            admission: None,
        });
    }
    debug!(?building, ?definition, ruin = ?ruin.as_ref().map(|(ruin, _)| *ruin),
            "placed object destroyed");
    let Some((ruin, handle)) = ruin else {
        return;
    };
    let Some(ruin_placeable) = resolve_object_placeable_definition(catalogue, ruin) else {
        return;
    };
    let Some(cells) = collect_occupied_placement_cells_for_transform(
        footprints.resolve(ruin_placeable, Some(&handle)),
        &transform,
        &mut scratch.cells,
    )
    .map(|cells| cells.to_vec().into_boxed_slice()) else {
        warn!(
            ?building,
            ?ruin,
            "ruin of a destroyed object has no footprint"
        );
        return;
    };
    let Ok(id) = allocator.allocate(root) else {
        warn!(
            ?building,
            ?ruin,
            "ruin of a destroyed object has no persistent id"
        );
        return;
    };
    let mut edit = PreparedObjectPlacementMutation {
        entity: id,
        definition: ruin,
        transform,
        cells,
        previous_transform: None,
        previous_cells: Box::new([]),
        mutation: PreparedObjectPlacementEditMutationKind::Create,
        applied_entity: None,
        preview: None,
        prefab: handle,
    };
    if !apply_prepared_placed_object_creation_edit(
        &mut commands,
        building,
        &mut edit,
        catalogue,
        Some(root),
        &placement,
        &mut scratch,
        &footprints,
        &prefabs,
        &models,
        &placed,
        &[building],
        false,
        &mut ConstructionTool::Inspect,
        &mut committed,
    ) {
        warn!(
            ?building,
            ?ruin,
            "ruin of a destroyed object could not be put down"
        );
    }
}

/// A fragment a destroyed building throws up; it falls under gravity and is
/// removed `secsToRemove` after it lands.
#[derive(Component)]
pub(super) struct FlyingRubbleFragment {
    velocity: Vec3,
    spin: Vec3,
    ground: f32,
    landed_for: f32,
    remove_after: f32,
}

// The fragments fly on their own, without the physics engine or
// terrain; `rubblePitchAngle` is taken as their highest launch elevation.
fn throw_fragments(
    commands: &mut Commands,
    prefab: &ScenePrefabAsset,
    destruction: &openzt2_game_data::world_definitions::world_objects::WorldObjectDestructionDefinition,
    transform: &Transform,
    seed: u64,
) {
    // A small hash keeps each building's throw different without a random stream.
    let mut state = seed | 1;
    let mut next = || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        (state >> 40) as f32 / (1u64 << 24) as f32
    };
    let [force_min, force_max] = destruction.force_mps;
    let start = transform.translation + Vec3::Y * destruction.start_height_cm as f32 / 100.0;
    for _ in 0..destruction.rubble_count {
        let yaw = next() * std::f32::consts::TAU;
        let elevation = (45.0 + next() * (destruction.pitch_degrees - 45.0).max(0.0)).to_radians();
        let speed = force_min + next() * (force_max - force_min).max(0.0);
        let velocity = Vec3::new(
            yaw.cos() * elevation.cos(),
            elevation.sin(),
            yaw.sin() * elevation.cos(),
        ) * speed;
        let fragment = commands
            .spawn((
                FlyingRubbleFragment {
                    velocity,
                    spin: Vec3::new(next(), next(), next()) * 8.0,
                    ground: transform.translation.y,
                    landed_for: 0.0,
                    remove_after: destruction.delay_ns as f32 / 1e9,
                },
                Transform::from_translation(start),
                Visibility::Inherited,
            ))
            .id();
        openzt2_world::world_spawn::prefab_presentation_render_tree::spawn_prefab_render_tree(
            commands, prefab, fragment, false,
        );
    }
}

pub(super) fn fly_rubble_fragments(
    mut commands: Commands,
    time: Res<Time>,
    mut fragments: Query<(Entity, &mut FlyingRubbleFragment, &mut Transform)>,
) {
    let delta = time.delta_secs();
    for (entity, mut fragment, mut transform) in &mut fragments {
        if transform.translation.y > fragment.ground {
            fragment.velocity.y -= 9.81 * delta;
            transform.translation += fragment.velocity * delta;
            let spin = fragment.spin * delta;
            transform.rotate(Quat::from_euler(EulerRot::XYZ, spin.x, spin.y, spin.z));
            continue;
        }
        transform.translation.y = fragment.ground;
        fragment.landed_for += delta;
        if fragment.landed_for >= fragment.remove_after {
            commands.entity(entity).despawn();
        }
    }
}
