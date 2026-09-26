use super::developer_diagnostics_view_types::DeveloperDiagnosticsViewState;
use crate::plugins::terrain::terrain_chunk_presentation_types::{
    TerrainRenderChunk, TerrainRenderRevision,
};
use avian3d::debug_render::DebugRender;
use bevy::{
    mesh::{Indices, VertexAttributeValues},
    prelude::*,
};
use std::collections::HashSet;

#[derive(Component)]
pub(super) struct RetainedTerrainWireframeGizmo {
    gizmo_entity: Entity,
    gizmo_asset_handle: Handle<GizmoAsset>,
}

/// Terrain collision follows the rendered heightfield exactly, so rebuilding
/// every triangle as transient Avian gizmo lines duplicates the existing mesh
/// and allocates tens of thousands of temporary collider shapes each frame. A
/// retained GizmoAsset uploads the same unique edges only when the terrain mesh
/// changes. Avian diagnostics remain responsible for ordinary colliders.
pub(super) fn synchronize_retained_terrain_wireframe_gizmo_assets(
    mut commands: Commands,
    views: Res<DeveloperDiagnosticsViewState>,
    mut gizmo_assets: ResMut<Assets<GizmoAsset>>,
    meshes: Res<Assets<Mesh>>,
    new_terrain: Query<
        (Entity, &Mesh3d),
        (
            With<TerrainRenderChunk>,
            Without<RetainedTerrainWireframeGizmo>,
        ),
    >,
    changed_terrain: Query<
        (&Mesh3d, &RetainedTerrainWireframeGizmo),
        Changed<TerrainRenderRevision>,
    >,
) {
    let enabled = views.physics_wireframes_are_visible();

    for (entity, mesh) in &new_terrain {
        let Some(mesh) = meshes.get(&mesh.0) else {
            continue;
        };
        let Some(asset) = create_terrain_wireframe_gizmo_asset(mesh) else {
            continue;
        };
        let asset = gizmo_assets.add(asset);
        let mut gizmo_commands = commands.spawn(ChildOf(entity));
        if enabled {
            gizmo_commands.insert(retained_terrain_wireframe_gizmo(asset.clone()));
        }
        let gizmo_entity = gizmo_commands.id();
        commands.entity(entity).insert((
            DebugRender::none(),
            RetainedTerrainWireframeGizmo {
                gizmo_entity,
                gizmo_asset_handle: asset,
            },
        ));
    }

    for (mesh, retained) in &changed_terrain {
        if let (Some(mesh), Some(mut asset)) = (
            meshes.get(&mesh.0),
            gizmo_assets.get_mut(&retained.gizmo_asset_handle),
        ) {
            if let Some(rebuilt) = create_terrain_wireframe_gizmo_asset(mesh) {
                *asset = rebuilt;
            }
        }
    }
}

pub(super) fn synchronize_retained_terrain_wireframe_gizmo_visibility(
    mut commands: Commands,
    views: Res<DeveloperDiagnosticsViewState>,
    terrain: Query<&RetainedTerrainWireframeGizmo>,
    gizmos: Query<(), With<Gizmo>>,
) {
    if !views.is_changed() {
        return;
    }
    let enabled = views.physics_wireframes_are_visible();
    for retained in &terrain {
        let is_rendered = gizmos.contains(retained.gizmo_entity);
        if enabled && !is_rendered {
            commands
                .entity(retained.gizmo_entity)
                .insert(retained_terrain_wireframe_gizmo(
                    retained.gizmo_asset_handle.clone(),
                ));
        } else if !enabled && is_rendered {
            commands.entity(retained.gizmo_entity).remove::<Gizmo>();
        }
    }
}

fn retained_terrain_wireframe_gizmo(asset: Handle<GizmoAsset>) -> Gizmo {
    Gizmo {
        handle: asset,
        line_config: GizmoLineConfig {
            width: 1.0,
            ..default()
        },
        depth_bias: -0.001,
    }
}

fn create_terrain_wireframe_gizmo_asset(mesh: &Mesh) -> Option<GizmoAsset> {
    let VertexAttributeValues::Float32x3(positions) = mesh.attribute(Mesh::ATTRIBUTE_POSITION)?
    else {
        return None;
    };
    let mut edges = HashSet::with_capacity(mesh.indices()?.len());
    match mesh.indices()? {
        Indices::U16(indices) => {
            for triangle in indices.chunks_exact(3) {
                insert_undirected_triangle_edges(
                    &mut edges,
                    [
                        u32::from(triangle[0]),
                        u32::from(triangle[1]),
                        u32::from(triangle[2]),
                    ],
                );
            }
        }
        Indices::U32(indices) => {
            for triangle in indices.chunks_exact(3) {
                insert_undirected_triangle_edges(
                    &mut edges,
                    [triangle[0], triangle[1], triangle[2]],
                );
            }
        }
    }

    let mut gizmo = GizmoAsset::new();
    for (start, end) in edges {
        gizmo.line(
            Vec3::from_array(*positions.get(start as usize)?),
            Vec3::from_array(*positions.get(end as usize)?),
            Color::WHITE,
        );
    }
    Some(gizmo)
}

fn insert_undirected_triangle_edges(edges: &mut HashSet<(u32, u32)>, triangle: [u32; 3]) {
    for (start, end) in [
        (triangle[0], triangle[1]),
        (triangle[1], triangle[2]),
        (triangle[2], triangle[0]),
    ] {
        edges.insert(if start < end {
            (start, end)
        } else {
            (end, start)
        });
    }
}
