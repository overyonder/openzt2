//! A building a rampaging animal smashes is removed, and its authored ruin
//! (`entityRuinBinderName`) is put down where it stood.

use bevy::asset::LoadState;
use bevy::gltf::Gltf;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use openzt2_game_assets::effect::SpawnCompleteParticleEffectDocument;
use openzt2_game_assets::scene_prefab::ScenePrefabAsset;
use openzt2_game_data::AssetId;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
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
        &'static mut Transform,
        &'static mut PlacedObjectFootprintOccupancy,
    ),
>;

#[allow(clippy::too_many_arguments)]
pub(super) fn ruin_destroyed_placed_objects(
    mut commands: Commands,
    mut requests: MessageReader<DestroyPlacedObject>,
    // Smashed objects whose ruin prefab is still loading.
    mut pending: Local<Vec<(Entity, Option<(AssetId, Handle<ScenePrefabAsset>)>)>>,
    mut world_assets: ResMut<Assets<WorldDefinitionAsset>>,
    world_definitions: Res<WorldDefinitions>,
    server: Res<AssetServer>,
    (prefabs, models): (Res<Assets<ScenePrefabAsset>>, Res<Assets<Gltf>>),
    members: Query<&WorldMember>,
    (mut placement, mut scratch, mut allocator): (
        ResMut<PlacedObjectFootprintOccupancyIndex>,
        ResMut<ObjectPlacementCellCollectionScratch>,
        ResMut<PersistentIdAllocator>,
    ),
    placed: PlacedObjects,
    mut committed: MessageWriter<ObjectPlacementCommitted>,
    (mut effects, mut sounds): (
        MessageWriter<SpawnCompleteParticleEffectDocument>,
        MessageWriter<PlayPositionedAudioCue>,
    ),
    // Fragment prefabs, requested as a building starts to fall.
    mut fragment_prefabs: Local<HashMap<AssetId, Handle<ScenePrefabAsset>>>,
) {
    pending.extend(requests.read().map(|request| (request.entity, None)));
    // A ruin's footprint comes from its model's bounds once that has loaded;
    // nothing else hydrates it before the ruin is put down.
    for (ruin, handle) in pending.iter().filter_map(|(_, prefab)| prefab.as_ref()) {
        if let Some(bounds) = prefabs.get(handle).and_then(|prefab| {
            prefab
                .canonical_scene_prefab_document()
                .automatic_placement_bounds_xz
        }) {
            world_definitions.apply_source_lowered_automatic_placement_bounds(
                &mut world_assets,
                *ruin,
                bounds,
            );
        }
    }
    let Some(catalogue) = world_definitions.get(&world_assets) else {
        return;
    };
    let created = HashMap::new();
    pending.retain_mut(|(building, prefab)| {
        let Ok((_, reference, transform, _)) = placed.get(*building) else {
            return false;
        };
        let (definition, transform) = (reference.definition, *transform);
        if let Some(fragments) = catalogue
            .find_object(definition)
            .and_then(|object| object.destruction.as_ref())
            .map(|destruction| destruction.rubble)
        {
            if let Some(scene) = catalogue.find_object(fragments).map(|object| object.prefab) {
                if let Some(handle) = catalogue.scene(scene) {
                    fragment_prefabs.entry(fragments).or_insert(handle);
                }
            }
        }
        let Ok(root) = members.get(*building).map(|member| member.root) else {
            return false;
        };
        let ruin = catalogue
            .find_object(definition)
            .and_then(|object| object.destruction.as_ref())
            .and_then(|destruction| {
                // An abstract ruin (`rubble_xt`) stands for its concrete kinds
                // (`rubble_xt_1`, `rubble_xt_2`), one picked per building.
                let placeable = |ruin: &AssetId| {
                    resolve_object_placeable_definition(catalogue, *ruin).is_some()
                };
                let ruin = if placeable(&destruction.ruin) {
                    destruction.ruin
                } else {
                    let mut kinds = catalogue
                        .objects()
                        .filter(|object| {
                            object.behavior_type_identifiers.contains(&destruction.ruin)
                        })
                        .map(|object| object.id)
                        .filter(placeable)
                        .collect::<Vec<_>>();
                    kinds.sort_unstable();
                    // ponytail: picked by the building's entity, not a seeded
                    // random stream; it stays the same while the ruin loads.
                    *kinds.get(building.to_bits() as usize % kinds.len().max(1))?
                };
                Some((ruin, catalogue.find_object(ruin)?.prefab))
            });
        if let (Some((ruin, scene)), None) = (ruin, prefab.as_ref()) {
            *prefab = catalogue.scene(scene).map(|handle| (ruin, handle));
        }
        // Wait for the ruin's prefab, and for its footprint from the prefab's
        // bounds; if either fails, the building still falls.
        if ruin.is_some()
            && prefab
                .as_ref()
                .is_some_and(|(ruin, handle)| match prefabs.get(handle) {
                    None => !matches!(
                        server.get_load_state(handle.id()),
                        Some(LoadState::Failed(_))
                    ),
                    Some(loaded) => {
                        loaded
                            .canonical_scene_prefab_document()
                            .automatic_placement_bounds_xz
                            .is_some()
                            && resolve_object_placeable_definition(catalogue, *ruin)
                                .is_some_and(|placeable| placeable.footprint.is_empty())
                    }
                })
        {
            return true;
        }
        // The fragments it throws load with it; wait for them too.
        if catalogue
            .find_object(definition)
            .and_then(|object| object.destruction.as_ref())
            .and_then(|destruction| fragment_prefabs.get(&destruction.rubble))
            .is_some_and(|handle| {
                !prefabs.contains(handle.id())
                    && !matches!(
                        server.get_load_state(handle.id()),
                        Some(LoadState::Failed(_))
                    )
            })
        {
            return true;
        }
        let ruin = ruin
            .zip(prefab.clone())
            .and_then(|((ruin, _), (_, handle))| {
                prefabs.contains(handle.id()).then_some((ruin, handle))
            });
        let Some(placeable) = resolve_object_placeable_definition(catalogue, definition) else {
            return false;
        };
        if let Some(cells) = collect_occupied_placement_cells_for_transform(
            placeable,
            &transform,
            &mut scratch.cells,
        ) {
            for cell in cells.to_vec() {
                placement.remove_entity_from_cell(cell, *building);
            }
        }
        commands.entity(*building).despawn();
        if let Some(destruction) = catalogue
            .find_object(definition)
            .and_then(|object| object.destruction.as_ref())
        {
            let fragments = fragment_prefabs
                .get(&destruction.rubble)
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
            return false;
        };
        let Some(ruin_placeable) = resolve_object_placeable_definition(catalogue, ruin) else {
            return false;
        };
        let Some(cells) = collect_occupied_placement_cells_for_transform(
            ruin_placeable,
            &transform,
            &mut scratch.cells,
        )
        .map(|cells| cells.to_vec().into_boxed_slice()) else {
            warn!(
                ?building,
                ?ruin,
                "ruin of a destroyed object has no footprint"
            );
            return false;
        };
        let Ok(id) = allocator.allocate(root) else {
            warn!(
                ?building,
                ?ruin,
                "ruin of a destroyed object has no persistent id"
            );
            return false;
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
            *building,
            &mut edit,
            catalogue,
            Some(root),
            &mut placement,
            &mut scratch,
            &prefabs,
            &models,
            &placed,
            &created,
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
        false
    });
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

// ponytail: the fragments fly on their own, without the physics engine or
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
