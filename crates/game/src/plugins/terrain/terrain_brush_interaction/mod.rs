use bevy::prelude::*;
use openzt2_game_data::AssetId;

use crate::plugins::{
    construction::{
        construction_interaction_types::{
            CommitConstruction, ConstructionCursor, ConstructionPreview, PlacementValidity,
        },
        construction_tool_and_placement_policy_types::{
            ConstructionPlacementPolicy, ConstructionTool,
        },
    },
    economy::money_types::Money,
    ui::picking::UiPointerCapture,
    world_spawn::{world_membership_types::WorldMember, world_membership_types::WorldRoot},
};

use super::{
    terrain_brush_interaction_state::{ActiveTerrainBrush, PendingTerrainBrushCommit},
    terrain_brush_types::{
        TerrainBrushFalloff, TerrainBrushKind, TerrainBrushPreview, TerrainStroke,
    },
    terrain_edit_types::{TerrainEditPreparationRejected, TerrainEditPrepared},
};

/// Tracks the brush preview and pointer stroke. Held dabs are applied live;
/// releasing the pointer commits the accumulated edit as one undo transaction.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn update_terrain_brush_preview_and_commit_completed_pointer_strokes(
    mut commands: Commands,
    tool: Res<ConstructionTool>,
    policy: Res<ConstructionPlacementPolicy>,
    time: Res<Time>,
    primary_pointer: Res<crate::plugins::input::input_types::PrimaryPointerInputState>,
    ui_capture: Res<UiPointerCapture>,
    cursors: Query<&ConstructionCursor>,
    roots: Query<Entity, With<WorldRoot>>,
    mut previews: Query<
        (
            Entity,
            &mut TerrainBrushPreview,
            &mut ConstructionPreview,
            &mut Transform,
            Option<&ActiveTerrainBrush>,
            Option<&PendingTerrainBrushCommit>,
        ),
        With<ConstructionPreview>,
    >,
    mut strokes: MessageWriter<TerrainStroke>,
    mut commits: MessageWriter<CommitConstruction>,
) {
    let ConstructionTool::Terrain(kind) = *tool else {
        return;
    };
    let (Ok(cursor), Ok(root)) = (cursors.single(), roots.single()) else {
        return;
    };
    let Some(strength_per_s) = policy.selected_terrain_brush_strength_metres_per_second(*tool)
    else {
        return;
    };
    let Some(radius_m) = policy.selected_terrain_brush_radius_metres(*tool) else {
        return;
    };
    let paint_trees = policy.biome_paint_trees && matches!(kind, TerrainBrushKind::Paint { .. });
    let paint_foliage =
        policy.biome_paint_foliage && matches!(kind, TerrainBrushKind::Paint { .. });
    let paint_rocks = policy.biome_paint_rocks && matches!(kind, TerrainBrushKind::Paint { .. });
    let falloff = match kind {
        TerrainBrushKind::Raise | TerrainBrushKind::Lower => TerrainBrushFalloff::Cosine,
        TerrainBrushKind::Smooth
        | TerrainBrushKind::Flatten { .. }
        | TerrainBrushKind::Paint { .. }
        | TerrainBrushKind::PaintWater { .. }
        | TerrainBrushKind::AddWater
        | TerrainBrushKind::RemoveWater => TerrainBrushFalloff::Constant,
    };

    let Ok((entity, mut brush, mut preview, mut transform, active, pending)) =
        previews.single_mut()
    else {
        if previews.is_empty() {
            let transform = Transform::from_translation(cursor.world);
            commands.spawn((
                TerrainBrushPreview {
                    center: cursor.world.xz(),
                    radius_m,
                    strength_per_s,
                    seconds: 0.0,
                    kind,
                    falloff,
                    paint_trees,
                    paint_foliage,
                    paint_rocks,
                    automatic_placement_variation: policy.biome_automatic_placement_variation,
                },
                ConstructionPreview {
                    definition: match kind {
                        TerrainBrushKind::Paint { biome, .. }
                        | TerrainBrushKind::PaintWater { biome, .. } => biome,
                        _ => AssetId::default(),
                    },
                    transform,
                    validity: PlacementValidity::Valid { cost: Money(0) },
                },
                transform,
                WorldMember { root },
            ));
        }
        return;
    };
    if pending.is_some() {
        return;
    }

    brush.center = cursor.world.xz();
    brush.radius_m = radius_m;
    brush.strength_per_s = strength_per_s;
    brush.kind = kind;
    brush.falloff = falloff;
    brush.paint_trees = paint_trees;
    brush.paint_foliage = paint_foliage;
    brush.paint_rocks = paint_rocks;
    brush.automatic_placement_variation = policy.biome_automatic_placement_variation;
    preview.transform.translation = cursor.world;
    transform.translation = cursor.world;

    let began = primary_pointer.just_pressed && !ui_capture.over_ui;
    let is_active = active.is_some() || began;
    if began {
        commands.entity(entity).insert(ActiveTerrainBrush);
    }
    if is_active && primary_pointer.pressed {
        let seconds = time.delta_secs();
        if seconds.is_finite() && seconds > 0.0 {
            strokes.write(TerrainStroke {
                preview: entity,
                center: cursor.world.xz(),
                radius_m,
                strength_per_s,
                seconds,
                kind,
                falloff: brush.falloff,
                paint_trees,
                paint_foliage,
                paint_rocks,
                automatic_placement_variation: policy.biome_automatic_placement_variation,
            });
        }
    }
    if is_active && primary_pointer.just_released {
        commands
            .entity(entity)
            .remove::<ActiveTerrainBrush>()
            .insert(PendingTerrainBrushCommit);
        commits.write(CommitConstruction { preview: entity });
    }
}

/// Once terrain has copied the brush path into the transaction's exact before/after
/// delta, the transient path and preview can be discarded.
pub(super) fn despawn_terrain_brush_previews_after_transaction_preparation(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut prepared: MessageReader<TerrainEditPrepared>,
    mut rejected: MessageReader<TerrainEditPreparationRejected>,
    pending: Query<
        (
            Entity,
            Option<&Mesh3d>,
            Option<&MeshMaterial3d<StandardMaterial>>,
        ),
        With<PendingTerrainBrushCommit>,
    >,
) {
    if prepared.read().next().is_none() && rejected.read().next().is_none() {
        return;
    }
    for (preview, mesh, material) in &pending {
        if let Some(mesh) = mesh {
            meshes.remove(&mesh.0);
        }
        if let Some(material) = material {
            materials.remove(&material.0);
        }
        commands.entity(preview).despawn();
    }
}
