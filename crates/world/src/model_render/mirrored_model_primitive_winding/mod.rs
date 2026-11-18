use bevy::asset::AssetId;
use bevy::mesh::{Indices, VertexAttributeValues};
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::render::render_resource::PrimitiveTopology;
use bevy::transform::helper::TransformHelper;
use openzt2_world_state::{
    model_render::ModelExpanded, world_spawn::prefab_model_tint::PrefabModelTint,
};

use super::authored_model_material_pass_projection::AdditionalAuthoredModelMaterialPass;
use super::configured_model_effect_materials::{
    AuthoredModelEffectMaterial, ModelMaterialInputOwner,
};

#[derive(Component)]
pub(super) struct AuthoredModelPrimitiveMesh {
    source: Handle<Mesh>,
    rendered_mesh: AssetId<Mesh>,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct ModelMeshVariant {
    source: AssetId<Mesh>,
    reflected: bool,
    srgb_tint: Option<[u32; 3]>,
}

pub(super) fn project_mirrored_and_tinted_model_primitive_meshes(
    mut commands: Commands,
    mut changes: super::model_projection_changes::ModelProjectionChanges,
    children: Query<&Children>,
    changed_branches: Query<Entity, Or<(Changed<Transform>, Changed<PrefabModelTint>)>>,
    mut removed_tints: RemovedComponents<PrefabModelTint>,
    changed_meshes: Query<
        Entity,
        (
            Without<AdditionalAuthoredModelMaterialPass>,
            With<Mesh3d>,
            Or<(
                Changed<Mesh3d>,
                Changed<ModelMaterialInputOwner<ModelExpanded>>,
                Changed<ModelMaterialInputOwner<PrefabModelTint>>,
                Added<AuthoredModelEffectMaterial>,
            )>,
        ),
    >,
    mut removed_effects: RemovedComponents<AuthoredModelEffectMaterial>,
    mut primitives: Query<
        (
            &mut Mesh3d,
            Option<&mut AuthoredModelPrimitiveMesh>,
            Has<ModelMaterialInputOwner<ModelExpanded>>,
            Has<AuthoredModelEffectMaterial>,
            Option<&ModelMaterialInputOwner<PrefabModelTint>>,
        ),
        Without<AdditionalAuthoredModelMaterialPass>,
    >,
    tints: Query<&PrefabModelTint>,
    mut additional_passes: Query<&mut Mesh3d, With<AdditionalAuthoredModelMaterialPass>>,
    transforms: TransformHelper,
    mut meshes: ResMut<Assets<Mesh>>,
    mut variants: Local<HashMap<ModelMeshVariant, AssetId<Mesh>>>,
    mut asset_events: MessageReader<AssetEvent<Mesh>>,
) {
    for event in asset_events.read() {
        match *event {
            AssetEvent::Modified { id } => {
                for (&variant, &output) in
                    variants.iter().filter(|(variant, _)| variant.source == id)
                {
                    if let Some(mesh) = meshes
                        .get(id)
                        .and_then(|source| build_model_mesh_variant(source, variant))
                    {
                        if let Some(mut existing) = meshes.get_mut(output) {
                            *existing = mesh;
                        }
                    }
                }
            }
            AssetEvent::Removed { id } => {
                variants.retain(|variant, output| variant.source != id && *output != id)
            }
            _ => {}
        }
    }

    changes.collect(
        changed_branches.iter().chain(removed_tints.read()),
        changed_meshes.iter().chain(removed_effects.read()),
    );
    let mut missing = Vec::new();
    for entity in changes.candidates() {
        let Ok((mut binding, authored, model, effect, tint_owner)) = primitives.get_mut(entity)
        else {
            continue;
        };
        if !model {
            if let Some(authored) = authored {
                if binding.id() == authored.rendered_mesh {
                    binding.set_if_neq(Mesh3d(authored.source.clone()));
                }
                commands
                    .entity(entity)
                    .remove::<AuthoredModelPrimitiveMesh>();
            }
            continue;
        }
        let source = authored
            .as_ref()
            .filter(|authored| authored.rendered_mesh == binding.id())
            .map_or_else(|| binding.0.clone(), |authored| authored.source.clone());
        let Ok(transform) = transforms.compute_global_transform(entity) else {
            missing.push(entity);
            continue;
        };
        let determinant = transform.affine().matrix3.determinant();
        if !determinant.is_finite() || determinant == 0.0 {
            continue;
        }
        let Some(mesh) = meshes.get(&source) else {
            missing.push(entity);
            continue;
        };
        let tint = if effect
            && matches!(
                mesh.attribute(Mesh::ATTRIBUTE_COLOR),
                Some(VertexAttributeValues::Float32x4(_))
            ) {
            tint_owner
                .and_then(|owner| tints.get(owner.0).ok())
                .filter(|tint| tint.0 != Color::WHITE)
                .map(|tint| {
                    tint.0
                        .to_srgba()
                        .to_vec4()
                        .truncate()
                        .to_array()
                        .map(f32::to_bits)
                })
        } else {
            None
        };
        let variant = ModelMeshVariant {
            source: source.id(),
            reflected: determinant < 0.0,
            srgb_tint: tint,
        };
        let target = if variant.reflected || tint.is_some() {
            if let Some(handle) = variants
                .get(&variant)
                .and_then(|&id| meshes.get_strong_handle(id))
            {
                handle
            } else {
                let Some(mesh) = meshes
                    .get(&source)
                    .and_then(|mesh| build_model_mesh_variant(mesh, variant))
                else {
                    warn!(?entity, "model mesh projection failed");
                    continue;
                };
                let handle = meshes.add(mesh);
                variants.insert(variant, handle.id());
                handle
            }
        } else {
            source.clone()
        };
        if let Some(mut authored) = authored {
            if authored.source != source || authored.rendered_mesh != target.id() {
                authored.source = source;
                authored.rendered_mesh = target.id();
            }
        } else {
            commands.entity(entity).insert(AuthoredModelPrimitiveMesh {
                source,
                rendered_mesh: target.id(),
            });
        }
        binding.set_if_neq(Mesh3d(target.clone()));
        if let Ok(children) = children.get(entity) {
            for child in children.iter() {
                if let Ok(mut pass) = additional_passes.get_mut(child) {
                    pass.set_if_neq(Mesh3d(target.clone()));
                }
            }
        }
    }
    changes.retry_missing(&mut missing);
}

fn build_model_mesh_variant(source: &Mesh, variant: ModelMeshVariant) -> Option<Mesh> {
    let mut mesh = source.clone();
    if variant.reflected {
        if mesh.primitive_topology() == PrimitiveTopology::TriangleList && mesh.indices().is_none()
        {
            let vertex_count = u32::try_from(mesh.count_vertices()).ok()?;
            mesh.insert_indices(Indices::U32((0..vertex_count).collect()));
        }
        mesh.invert_winding().ok()?;
    }
    if let (Some(tint), Some(VertexAttributeValues::Float32x4(colors))) =
        (variant.srgb_tint, mesh.attribute_mut(Mesh::ATTRIBUTE_COLOR))
    {
        let [red, green, blue] = tint.map(f32::from_bits);
        for color in colors {
            color[0] *= red;
            color[1] *= green;
            color[2] *= blue;
        }
    }
    Some(mesh)
}
