use bevy::{
    asset::RenderAssetUsages,
    light::NotShadowCaster,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};
use openzt2_game_assets::{
    scene_prefab::ScenePrefabAsset,
    terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset,
};
use openzt2_game_data::scene_prefab::PrefabEntity;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::environment::environment_presentation_types::EnvironmentTerrainAmbientLight;
use openzt2_zoo_state::terrain::{
    terrain_chunk_types::{EditedTerrainSamples, TerrainChunk, TerrainIndex},
    terrain_world_sampling::{sample_terrain, terrain_chunk_at},
};

#[derive(Component)]
struct AuthoredPrefabTerrainDecal {
    texture: Handle<Image>,
    radius: f32,
    alpha_blend: bool,
    lighting: bool,
    colors: [Option<[u8; 3]>; 4],
}

impl AuthoredPrefabTerrainDecal {
    fn emission(&self, terrain_ambient: Vec3) -> LinearRgba {
        let light = |color: Option<[u8; 3]>, default: Vec3| {
            color.map_or(default, |rgb| {
                Vec3::from_array(rgb.map(|channel| f32::from(channel) / 255.0))
            })
        };
        let ambient = light(self.colors[0], Vec3::ONE);
        let emissive = light(self.colors[3], Vec3::ZERO);
        let color = if self.lighting {
            emissive + ambient * terrain_ambient
        } else {
            emissive
        };
        LinearRgba::new(color.x, color.y, color.z, 1.0)
    }
}

pub(super) fn spawn_prefab_terrain_decal(
    commands: &mut Commands,
    prefab: &ScenePrefabAsset,
    row: &PrefabEntity,
    owner: Entity,
    world: Entity,
) {
    let Some(decal) = row
        .terrain_decal
        .filter(|decal| decal.radius_metres.is_finite() && decal.radius_metres > 0.0)
    else {
        return;
    };
    let Some(texture) = prefab.terrain_decal_texture(decal.texture).cloned() else {
        return;
    };
    commands.spawn((
        AuthoredPrefabTerrainDecal {
            texture,
            radius: decal.radius_metres,
            alpha_blend: decal.alpha_blend,
            lighting: decal.lighting,
            colors: prefab
                .terrain_decal_lighting(decal.texture)
                .copied()
                .unwrap_or_default(),
        },
        WorldMember { root: world },
        ChildOf(owner),
        Transform::IDENTITY,
        Visibility::Inherited,
        NotShadowCaster,
    ));
}

pub(super) fn register_prefab_terrain_decals(app: &mut App) {
    app.add_systems(
        PostUpdate,
        project_prefab_terrain_decal_meshes
            .after(bevy::transform::TransformSystems::Propagate)
            .before(bevy::camera::visibility::VisibilitySystems::CalculateBounds),
    )
    .add_systems(
        PostUpdate,
        project_prefab_terrain_decal_ambient.after(project_prefab_terrain_decal_meshes),
    );
}

fn project_prefab_terrain_decal_meshes(
    mut commands: Commands,
    worlds: Query<Ref<TerrainIndex>>,
    chunks: Query<(&TerrainChunk, Option<&EditedTerrainSamples>)>,
    changed_chunks: Query<
        &WorldMember,
        (
            With<TerrainChunk>,
            Or<(Changed<EditedTerrainSamples>, Changed<TerrainChunk>)>,
        ),
    >,
    terrains: Res<Assets<TerrainAsset>>,
    mut terrain_events: MessageReader<AssetEvent<TerrainAsset>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut decals: Query<(
        Entity,
        &WorldMember,
        Ref<GlobalTransform>,
        Ref<AuthoredPrefabTerrainDecal>,
        Option<&Mesh3d>,
        &mut Visibility,
    )>,
) {
    let dirty_worlds = changed_chunks
        .iter()
        .map(|world| world.root)
        .collect::<bevy::ecs::entity::EntityHashSet>();
    let changed_terrains = terrain_events
        .read()
        .filter_map(|event| match *event {
            AssetEvent::Added { id }
            | AssetEvent::Modified { id }
            | AssetEvent::Removed { id }
            | AssetEvent::LoadedWithDependencies { id } => Some(id),
            _ => None,
        })
        .collect::<bevy::platform::collections::HashSet<_>>();
    for (entity, world, pose, decal, mesh, mut visibility) in &mut decals {
        let Ok(index) = worlds.get(world.root) else {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        };
        let changed = index.is_changed()
            || pose.is_changed()
            || decal.is_changed()
            || dirty_worlds.contains(&world.root);
        if !changed && changed_terrains.is_empty() {
            continue;
        }
        let center = pose.translation().xz();
        let Some(chunk) =
            terrain_chunk_at(&index, center).and_then(|entity| chunks.get(entity).ok())
        else {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        };
        if !changed && !changed_terrains.contains(&chunk.0.asset.id()) {
            continue;
        }
        let sample = |position| {
            let chunk = terrain_chunk_at(&index, position)?;
            let (chunk, edited) = chunks.get(chunk).ok()?;
            sample_terrain(chunk, terrains.get(&chunk.asset)?, edited, position)
        };
        let radius = decal.radius;
        let diameter = 2.0 * radius;
        let original_corner = center - Vec2::splat(radius);
        let Some((min_cell, max_cell)) = index.chunks.keys().copied().fold(None, |bounds, cell| {
            Some(bounds.map_or((cell, cell), |(min, max): (IVec2, IVec2)| {
                (min.min(cell), max.max(cell))
            }))
        }) else {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        };
        let world_min = index.origin + min_cell.as_vec2() * index.chunk_span_m;
        let world_max = index.origin + (max_cell.as_vec2() + Vec2::ONE) * index.chunk_span_m;
        let corner = original_corner.max(world_min);
        let last_sample = Vec2::new(world_max.x.next_down(), world_max.y.next_down());
        let limit = (center + Vec2::splat(radius)).min(last_sample);
        let span = limit - corner;
        if span.min_element() <= 0.0 || !diameter.is_finite() || chunk.0.spacing_m <= 0.0 {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        }
        let divisions = (span / chunk.0.spacing_m).ceil().max(Vec2::ONE).as_uvec2();
        let step = span / divisions.as_vec2();
        let inverse = pose.affine().inverse();
        let mut positions = Vec::new();
        let mut normals = Vec::new();
        let mut uvs = Vec::new();
        let mut indices = Vec::new();
        let mut previous_row = vec![None; divisions.x as usize + 1];
        let mut current_row = previous_row.clone();
        for z in 0..=divisions.y {
            for x in 0..=divisions.x as usize {
                let position = corner + Vec2::new(x as f32, z as f32) * step;
                current_row[x] = sample(position).map(|terrain| {
                    let index = positions.len() as u32;
                    let lifted = Vec3::new(position.x, terrain.height_m, position.y)
                        + terrain.normal * 0.002;
                    positions.push(inverse.transform_point3(lifted).to_array());
                    normals.push((pose.rotation().inverse() * terrain.normal).to_array());
                    uvs.push(((position - original_corner) / diameter).to_array());
                    index
                });
                if x > 0 && z > 0 {
                    for triangle in [
                        [previous_row[x - 1], current_row[x - 1], previous_row[x]],
                        [previous_row[x], current_row[x - 1], current_row[x]],
                    ] {
                        if let [Some(a), Some(b), Some(c)] = triangle {
                            indices.extend([a, b, c]);
                        }
                    }
                }
            }
            std::mem::swap(&mut current_row, &mut previous_row);
        }
        if indices.is_empty() {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        }
        visibility.set_if_neq(Visibility::Inherited);
        let mut next = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::all());
        next.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
        next.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
        next.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
        next.insert_indices(Indices::U32(indices));
        if let Some(mesh) = mesh {
            if let Some(mut current) = meshes.get_mut(&mesh.0) {
                *current = next;
            }
        } else {
            let material = materials.add(StandardMaterial {
                base_color_texture: Some(decal.texture.clone()),
                base_color: decal.colors[1]
                    .map_or(Color::WHITE, |[r, g, b]| Color::srgb_u8(r, g, b)),
                specular_tint: decal.colors[2]
                    .map_or(Color::WHITE, |[r, g, b]| Color::srgb_u8(r, g, b)),
                emissive: decal.emission(Vec3::ZERO),
                emissive_texture: Some(decal.texture.clone()),
                unlit: !decal.lighting,
                alpha_mode: if decal.alpha_blend {
                    AlphaMode::Blend
                } else {
                    AlphaMode::Opaque
                },
                ..default()
            });
            commands
                .entity(entity)
                .try_insert((Mesh3d(meshes.add(next)), MeshMaterial3d(material)));
        }
    }
}

fn project_prefab_terrain_decal_ambient(
    decals: Query<(
        &WorldMember,
        &AuthoredPrefabTerrainDecal,
        Ref<MeshMaterial3d<StandardMaterial>>,
    )>,
    lighting: Query<(&WorldMember, Ref<EnvironmentTerrainAmbientLight>)>,
    mut removed: RemovedComponents<EnvironmentTerrainAmbientLight>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mut changed = bevy::ecs::entity::EntityHashMap::default();
    let refresh = removed.read().count() != 0;
    for (member, light) in &lighting {
        if refresh || light.is_changed() {
            changed.insert(member.root, light.0.unwrap_or_default());
        }
    }
    for (member, decal, binding) in &decals {
        if !decal.lighting {
            continue;
        }
        let Some(ambient) = changed.get(&member.root).copied().or_else(|| {
            (binding.is_added() || refresh).then(|| {
                lighting
                    .iter()
                    .find(|(world, _)| world.root == member.root)
                    .and_then(|(_, light)| light.0)
                    .unwrap_or_default()
            })
        }) else {
            continue;
        };
        let emission = decal.emission(ambient);
        if materials
            .get(&binding.0)
            .is_some_and(|material| material.emissive != emission)
        {
            if let Some(mut material) = materials.get_mut(&binding.0) {
                material.emissive = emission;
            }
        }
    }
}
