use bevy::asset::AssetId;
use bevy::mesh::VertexAttributeValues;
use bevy::prelude::*;

use openzt2_game_assets::material::runtime::effect_pass_gpu_data::EffectPassMaterial;
use openzt2_world_state::world_spawn::prefab_model_tint::PrefabModelTint;

use openzt2_world_state::model_render::ModelExpanded;

#[derive(Component)]
pub(super) struct ProjectedPrefabTintMaterial {
    handle: Handle<StandardMaterial>,
    authored_base_color: Color,
}

type EffectTintAssets<'w> = (
    ResMut<'w, Assets<EffectPassMaterial>>,
    ResMut<'w, Assets<bevy::render::storage::ShaderBuffer>>,
);

/// The material a model without vertex colours draws with untinted.
#[derive(Component)]
pub(super) struct ProjectedPrefabTintEffectMaterial {
    original: Handle<EffectPassMaterial>,
}

#[derive(Component)]
pub struct ProjectedPrefabTintMesh {
    handle: Handle<Mesh>,
    authored_vertex_colors: Box<[[f32; 4]]>,
}

impl ProjectedPrefabTintMesh {
    /// Borrows the retained projection inputs without retaining the source asset.
    pub fn mesh_and_authored_colors(&self) -> (AssetId<Mesh>, &[[f32; 4]]) {
        (self.handle.id(), &self.authored_vertex_colors)
    }
}

pub(super) fn project_prefab_model_tints_onto_standard_materials_and_effect_pass_vertex_colors(
    mut commands: Commands,
    roots: Query<(Entity, Ref<ModelExpanded>, Option<Ref<PrefabModelTint>>)>,
    hierarchy: Query<(Option<&PrefabModelTint>, Option<&ChildOf>)>,
    children: Query<&Children>,
    primitives: Query<(
        Entity,
        Option<&Mesh3d>,
        Option<&MeshMaterial3d<StandardMaterial>>,
        Option<&MeshMaterial3d<EffectPassMaterial>>,
        Option<&ProjectedPrefabTintMaterial>,
        Option<&ProjectedPrefabTintMesh>,
        Option<&ProjectedPrefabTintEffectMaterial>,
    )>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    (mut effect_materials, mut shader_buffers): EffectTintAssets,
) {
    for (root, expanded, root_tint) in &roots {
        if !expanded.is_added() && root_tint.as_ref().is_none_or(|tint| !tint.is_changed()) {
            continue;
        }
        let Some(tint) = find_nearest_prefab_model_tint_in_ancestor_hierarchy(root, &hierarchy)
        else {
            continue;
        };
        for entity in
            std::iter::once(root).chain(children.iter_descendants_depth_first::<Children>(root))
        {
            let Ok((
                entity,
                mesh,
                standard,
                effect,
                projected_material,
                projected_mesh,
                projected_effect,
            )) = primitives.get(entity)
            else {
                continue;
            };
            if let Some(effect) = effect {
                // A sky station or tower has no vertex colours to multiply.
                if projected_mesh.is_none() && !mesh_has_vertex_colors(mesh, &meshes) {
                    tint_effect_material(
                        &mut commands,
                        entity,
                        projected_effect.map_or_else(|| effect.0.clone(), |p| p.original.clone()),
                        tint.0,
                        &mut effect_materials,
                        &mut shader_buffers,
                    );
                    continue;
                }
                if let Some(projected_mesh) = projected_mesh {
                    if let Some(mut mesh) = meshes.get_mut(&projected_mesh.handle) {
                        multiply_authored_mesh_vertex_colors_by_rgb_tint(
                            &mut mesh,
                            &projected_mesh.authored_vertex_colors,
                            tint.0,
                        );
                    }
                } else if let Some(mut mesh) =
                    mesh.and_then(|source| meshes.get(&source.0)).cloned()
                {
                    let Some(VertexAttributeValues::Float32x4(authored_vertex_colors)) =
                        mesh.attribute(Mesh::ATTRIBUTE_COLOR)
                    else {
                        continue;
                    };
                    let authored_vertex_colors = authored_vertex_colors.clone().into_boxed_slice();
                    multiply_authored_mesh_vertex_colors_by_rgb_tint(
                        &mut mesh,
                        &authored_vertex_colors,
                        tint.0,
                    );
                    let handle = meshes.add(mesh);
                    commands.entity(entity).insert((
                        Mesh3d(handle.clone()),
                        ProjectedPrefabTintMesh {
                            handle,
                            authored_vertex_colors,
                        },
                    ));
                }
                continue;
            }
            if let Some(projected_material) = projected_material {
                if let Some(mut material) = materials.get_mut(&projected_material.handle) {
                    multiply_authored_standard_material_base_color_by_rgb_tint(
                        &mut material,
                        projected_material.authored_base_color,
                        tint.0,
                    );
                }
            } else if let Some(mut material) = standard
                .and_then(|source| materials.get(&source.0))
                .cloned()
            {
                let authored_base_color = material.base_color;
                multiply_authored_standard_material_base_color_by_rgb_tint(
                    &mut material,
                    authored_base_color,
                    tint.0,
                );
                let handle = materials.add(material);
                commands.entity(entity).insert((
                    MeshMaterial3d(handle.clone()),
                    ProjectedPrefabTintMaterial {
                        handle,
                        authored_base_color,
                    },
                ));
            }
        }
    }
}

fn find_nearest_prefab_model_tint_in_ancestor_hierarchy(
    mut entity: Entity,
    hierarchy: &Query<(Option<&PrefabModelTint>, Option<&ChildOf>)>,
) -> Option<PrefabModelTint> {
    loop {
        let Ok((tint, parent)) = hierarchy.get(entity) else {
            return None;
        };
        if tint.is_some() {
            return tint.copied();
        }
        entity = parent?.parent();
    }
}

fn multiply_authored_mesh_vertex_colors_by_rgb_tint(
    mesh: &mut Mesh,
    authored_vertex_colors: &[[f32; 4]],
    tint: Color,
) {
    let Some(VertexAttributeValues::Float32x4(colors)) = mesh.attribute_mut(Mesh::ATTRIBUTE_COLOR)
    else {
        return;
    };
    let tint = tint.to_srgba();
    for (color, authored) in colors.iter_mut().zip(authored_vertex_colors) {
        color[0] = authored[0] * tint.red;
        color[1] = authored[1] * tint.green;
        color[2] = authored[2] * tint.blue;
        color[3] = authored[3];
    }
}

fn multiply_authored_standard_material_base_color_by_rgb_tint(
    material: &mut StandardMaterial,
    authored_base_color: Color,
    tint: Color,
) {
    let authored = authored_base_color.to_srgba();
    let tint = tint.to_srgba();
    material.base_color = Color::srgba(
        authored.red * tint.red,
        authored.green * tint.green,
        authored.blue * tint.blue,
        authored.alpha,
    );
}

/// Draws a model without vertex colours with a copy of its material tinted
/// `tint`, or with its own material again for white.
fn tint_effect_material(
    commands: &mut Commands,
    entity: Entity,
    original: Handle<EffectPassMaterial>,
    tint: Color,
    effect_materials: &mut Assets<EffectPassMaterial>,
    shader_buffers: &mut Assets<bevy::render::storage::ShaderBuffer>,
) {
    let material = if tint == Color::WHITE {
        Some(original.clone())
    } else {
        effect_materials.get(&original).cloned().and_then(|source| {
            source
                .clone_tinted(tint.to_srgba().to_vec4(), shader_buffers)
                .map(|tinted| effect_materials.add(tinted))
        })
    };
    if let Some(material) = material {
        commands.entity(entity).insert((
            MeshMaterial3d(material),
            ProjectedPrefabTintEffectMaterial { original },
        ));
    }
}

fn mesh_has_vertex_colors(mesh: Option<&Mesh3d>, meshes: &Assets<Mesh>) -> bool {
    mesh.and_then(|source| meshes.get(&source.0))
        .is_some_and(|mesh| mesh.attribute(Mesh::ATTRIBUTE_COLOR).is_some())
}
