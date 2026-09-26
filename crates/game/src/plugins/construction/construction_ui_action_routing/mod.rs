use bevy::{ecs::system::SystemParam, prelude::*};
use openzt2_game_data::terrain::TerrainWaterDepth;
use openzt2_game_data::ui_document::action::construction::{
    UiBiomePaintClass, UiBiomeSurface, UiConstructionAction, UiConstructionConfirmation,
    UiFencePlacementMode, UiTankEditMode, UiTerrainEditMode,
};

use crate::assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::information::entity_selection_types::SelectedEntity;
use crate::plugins::placement::placed_object_types::PlacedObjectDefinitionReference;
use crate::plugins::placement::placement_preview_types::PlacementRotationRequest;
use crate::plugins::placement::placement_preview_types::RelocatingPlacedObject;
use crate::plugins::terrain::terrain_brush_types::TerrainBrushKind;
use crate::plugins::terrain::terrain_brush_types::TerrainBrushPreview;
use crate::plugins::ui::authored_ui_node_projection_components::UiDocumentOwner;
use crate::plugins::ui::authored_ui_node_projection_components::UiDocumentRoot;
use crate::plugins::world_spawn::world_entity_crating::RemoveWorldEntityFromCrate;
use crate::plugins::world_spawn::world_entity_crating::WorldEntityIsCrated;

use super::construction_edit_history_types::{
    ApplyConstructionTransaction, ConstructionEditDirection, ConstructionEditHistory,
};
use super::{
    construction_interaction_types::{
        CancelConstruction, CommitConstruction, ConstructionPreview, DeleteEntity,
    },
    construction_tool_and_placement_policy_types::{
        BiomeSurface, ConstructionPlacementPolicy, ConstructionTool, ElevatedPathGrade,
        FencePlacementShape, SelectConstructionTool, TankEditKind,
    },
};
use crate::plugins::ui::authored_ui_action_projection_components::UiConstructionActions;
use crate::plugins::ui::authored_ui_activation_contracts::UiNodeActivated;

#[derive(SystemParam)]
pub(super) struct ConstructionUiIntents<'w> {
    tools: MessageWriter<'w, SelectConstructionTool>,
    deletions: MessageWriter<'w, DeleteEntity>,
    commits: MessageWriter<'w, CommitConstruction>,
    cancellations: MessageWriter<'w, CancelConstruction>,
    history: MessageWriter<'w, ApplyConstructionTransaction>,
    uncrate: MessageWriter<'w, RemoveWorldEntityFromCrate>,
}
pub(super) fn route_construction_ui_actions(
    mut activations: MessageReader<UiNodeActivated>,
    selected_tool: Res<ConstructionTool>,
    documents: Res<Assets<UiDocumentAsset>>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    action_nodes: Query<(&UiConstructionActions, &UiDocumentOwner)>,
    roots: Query<&UiDocumentRoot>,
    selected: Res<SelectedEntity>,
    mut intents: ConstructionUiIntents,
    history: Res<ConstructionEditHistory>,
    mut policy: ResMut<ConstructionPlacementPolicy>,
    placeables: Query<
        &PlacedObjectDefinitionReference,
        Without<crate::plugins::animal_lifecycle::types::CratingRestricted>,
    >,
    relocating: Query<Entity, With<RelocatingPlacedObject>>,
    mut previews: Query<(Entity, Option<&mut TerrainBrushPreview>), With<ConstructionPreview>>,
    mut commands: Commands,
) {
    for activation in activations.read() {
        let Ok((range, owner)) = action_nodes.get(activation.node) else {
            continue;
        };
        let Ok(root) = roots.get(owner.0) else {
            continue;
        };
        let Some(document) = documents.get(&root.document) else {
            continue;
        };
        let records = range.authored_action_records(document);
        for record in records {
            if activation.trigger != record.trigger {
                continue;
            }
            debug!(
                node = ?activation.node,
                trigger = ?activation.trigger,
                action = ?std::mem::discriminant(&record.action),
                "applying typed construction UI action"
            );
            match &record.action {
                UiConstructionAction::SelectInspectionTool => {
                    intents
                        .tools
                        .write(SelectConstructionTool(ConstructionTool::Inspect));
                }
                UiConstructionAction::SellSelected => {
                    if let Some(entity) = selected.0 {
                        intents.deletions.write(DeleteEntity(entity));
                    }
                }
                UiConstructionAction::SellPhysicalObject { definition } => {
                    if let Some(entity) = selected.0 {
                        if placeables
                            .get(entity)
                            .is_ok_and(|placeable| placeable.definition.0 == definition.0)
                        {
                            intents.deletions.write(DeleteEntity(entity));
                        }
                    }
                }
                UiConstructionAction::MoveSelected => {
                    if let Some((entity, placeable)) = selected.0.and_then(|entity| {
                        placeables
                            .get(entity)
                            .ok()
                            .map(|placeable| (entity, placeable))
                    }) {
                        for previous in &relocating {
                            if previous != entity {
                                commands
                                    .entity(previous)
                                    .remove::<(RelocatingPlacedObject, PlacementRotationRequest)>();
                            }
                        }
                        commands.entity(entity).insert(RelocatingPlacedObject);
                        intents
                            .tools
                            .write(SelectConstructionTool(ConstructionTool::Place(
                                placeable.definition,
                            )));
                    }
                }
                UiConstructionAction::RotateSelected { steps } => {
                    if let Some((entity, placeable)) = selected.0.and_then(|entity| {
                        placeables
                            .get(entity)
                            .ok()
                            .map(|placeable| (entity, placeable))
                    }) {
                        for previous in &relocating {
                            if previous != entity {
                                commands
                                    .entity(previous)
                                    .remove::<(RelocatingPlacedObject, PlacementRotationRequest)>();
                            }
                        }
                        commands
                            .entity(entity)
                            .insert((RelocatingPlacedObject, PlacementRotationRequest(*steps)));
                        intents
                            .tools
                            .write(SelectConstructionTool(ConstructionTool::Place(
                                placeable.definition,
                            )));
                    }
                }
                UiConstructionAction::SetTerrainCursorSize { direction } => {
                    let Some(radius_m) = policy
                        .adjust_selected_terrain_brush_radius_by_authored_half_metre_step(
                            *selected_tool,
                            *direction,
                        )
                    else {
                        continue;
                    };
                    for (_, preview) in &mut previews {
                        if let Some(mut preview) = preview {
                            preview.radius_m = radius_m;
                        }
                    }
                }
                UiConstructionAction::SetPlacementObject { definition } => {
                    let definition = openzt2_game_data::AssetId(definition.0);
                    let Some(tool) = active_definitions.get(&definitions).and_then(|catalogue| {
                        if catalogue.find_fence(definition).is_some() {
                            Some(ConstructionTool::Fence(definition))
                        } else if catalogue.find_path(definition).is_some() {
                            Some(ConstructionTool::Path(definition))
                        } else if catalogue.find_placeable(definition).is_some() {
                            Some(ConstructionTool::Place(definition))
                        } else {
                            None
                        }
                    }) else {
                        warn!(
                            ?definition,
                            "ignoring placement selection absent from authored definitions"
                        );
                        continue;
                    };
                    intents.tools.write(SelectConstructionTool(tool));
                }
                UiConstructionAction::Undo => {
                    if let Some(transaction) = history
                        .cursor
                        .checked_sub(1)
                        .and_then(|index| history.entries.get(index).copied())
                    {
                        intents.history.write(ApplyConstructionTransaction {
                            transaction,
                            direction: ConstructionEditDirection::Undo,
                        });
                    }
                }
                UiConstructionAction::SetSingleObjectPlacement => {
                    policy.repeat_placement = false;
                }
                UiConstructionAction::CrateSelected => {
                    if let Some(entity) =
                        selected.0.filter(|entity| placeables.get(*entity).is_ok())
                    {
                        commands.entity(entity).insert(WorldEntityIsCrated);
                    }
                }
                UiConstructionAction::SetBiome { biome } => {
                    policy.selected_biome = openzt2_game_data::AssetId(biome.0);
                    intents
                        .tools
                        .write(SelectConstructionTool(ConstructionTool::Terrain(
                            TerrainBrushKind::Paint {
                                biome: openzt2_game_data::AssetId(biome.0),
                                ground_cover: match policy.biome_surface {
                                    BiomeSurface::Ground => Some(false),
                                    BiomeSurface::GroundCover => Some(true),
                                    _ => None,
                                },
                            },
                        )));
                }
                UiConstructionAction::SetBiomeSurface { surface } => {
                    policy.biome_surface = match surface {
                        UiBiomeSurface::DeepWater => BiomeSurface::DeepWater,
                        UiBiomeSurface::ShallowWater => BiomeSurface::ShallowWater,
                        UiBiomeSurface::Ground => BiomeSurface::Ground,
                        UiBiomeSurface::MixedGround => BiomeSurface::MixedGround,
                        UiBiomeSurface::GroundCover => BiomeSurface::GroundCover,
                        UiBiomeSurface::FoliageMix => BiomeSurface::FoliageMix,
                    };
                    intents
                        .tools
                        .write(SelectConstructionTool(ConstructionTool::Terrain(
                            match policy.biome_surface {
                                BiomeSurface::DeepWater => TerrainBrushKind::PaintWater {
                                    biome: policy.selected_biome,
                                    depth: TerrainWaterDepth::Deep,
                                },
                                BiomeSurface::ShallowWater => TerrainBrushKind::PaintWater {
                                    biome: policy.selected_biome,
                                    depth: TerrainWaterDepth::Shallow,
                                },
                                BiomeSurface::Ground
                                | BiomeSurface::MixedGround
                                | BiomeSurface::GroundCover
                                | BiomeSurface::FoliageMix => TerrainBrushKind::Paint {
                                    biome: policy.selected_biome,
                                    ground_cover: match policy.biome_surface {
                                        BiomeSurface::Ground => Some(false),
                                        BiomeSurface::GroundCover => Some(true),
                                        _ => None,
                                    },
                                },
                            },
                        )));
                }
                UiConstructionAction::SetBiomePaintTrees { enabled } => {
                    policy.biome_paint_trees = *enabled;
                }
                UiConstructionAction::SetBiomeAutomaticPlacementVariation { variation } => {
                    policy.biome_automatic_placement_variation = *variation;
                    policy.biome_paint_trees = true;
                    policy.biome_paint_foliage = true;
                    policy.biome_paint_rocks = true;
                }
                UiConstructionAction::SetFencePlacementMode { mode } => {
                    policy.fence_shape = match mode {
                        UiFencePlacementMode::RubberBand => FencePlacementShape::Freeform,
                        UiFencePlacementMode::Rectangle => FencePlacementShape::Rectangle,
                        UiFencePlacementMode::Octagon => FencePlacementShape::Octagon,
                    };
                }
                UiConstructionAction::SetTankEditMode { mode } => {
                    policy.tank_edit = match mode {
                        UiTankEditMode::RaiseWall => TankEditKind::RaiseWall,
                        UiTankEditMode::LowerWall => TankEditKind::LowerWall,
                        UiTankEditMode::RaiseFloor => TankEditKind::RaiseFloor,
                        UiTankEditMode::LowerFloor => TankEditKind::LowerFloor,
                    };
                }
                UiConstructionAction::SetTerrainEditMode { mode } => {
                    let kind = match mode {
                        UiTerrainEditMode::Hill => TerrainBrushKind::Raise,
                        UiTerrainEditMode::Valley => TerrainBrushKind::Lower,
                        UiTerrainEditMode::Flatten => TerrainBrushKind::Flatten {
                            height_cm: policy.terrain_flatten_height_cm,
                        },
                        UiTerrainEditMode::Smooth => TerrainBrushKind::Smooth,
                    };
                    intents
                        .tools
                        .write(SelectConstructionTool(ConstructionTool::Terrain(kind)));
                    for (_, preview) in &mut previews {
                        if let Some(mut preview) = preview {
                            preview.kind = kind;
                        }
                    }
                }
                UiConstructionAction::SetTerrainFlattenHeight { height_decimetres } => {
                    let centimetres = height_decimetres.saturating_mul(10);
                    policy.terrain_flatten_height_cm =
                        centimetres.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16;
                    for (_, preview) in &mut previews {
                        if let Some(mut preview) = preview {
                            if matches!(preview.kind, TerrainBrushKind::Flatten { .. }) {
                                preview.kind = TerrainBrushKind::Flatten {
                                    height_cm: policy.terrain_flatten_height_cm,
                                };
                            }
                        }
                    }
                }
                UiConstructionAction::SetTerrainFlattenSpeed {
                    strength_tenths_per_second,
                } => {
                    policy.terrain_flatten_strength_tenths_per_second = strength_tenths_per_second
                        .unsigned_abs()
                        .min(u32::from(u16::MAX))
                        as u16;
                    let strength =
                        f32::from(policy.terrain_flatten_strength_tenths_per_second) / 10.0;
                    for (_, preview) in &mut previews {
                        if let Some(mut preview) = preview {
                            if matches!(preview.kind, TerrainBrushKind::Flatten { .. }) {
                                preview.strength_per_s = strength;
                            }
                        }
                    }
                }
                UiConstructionAction::SetElevatedPathPlacementMode { grade_steps } => {
                    policy.elevated_path_grade = match grade_steps.signum() {
                        -1 => ElevatedPathGrade::RampDown,
                        1 => ElevatedPathGrade::RampUp,
                        _ => ElevatedPathGrade::Flat,
                    };
                }
                UiConstructionAction::AdjustElevatedPathHeight { steps } => {
                    policy.elevated_path_height_steps = policy
                        .elevated_path_height_steps
                        .saturating_add(i16::from(*steps));
                }
                UiConstructionAction::AdjustSkyTowerHeight { steps } => {
                    policy.sky_tower_height_steps = policy
                        .sky_tower_height_steps
                        .saturating_add(i16::from(*steps));
                }
                UiConstructionAction::SetBiomePaintClass {
                    detail_class,
                    enabled,
                } => match detail_class {
                    UiBiomePaintClass::Foliage => policy.biome_paint_foliage = *enabled,
                    UiBiomePaintClass::Rocks => policy.biome_paint_rocks = *enabled,
                    UiBiomePaintClass::All => {
                        policy.biome_paint_trees = *enabled;
                        policy.biome_paint_foliage = *enabled;
                        policy.biome_paint_rocks = *enabled;
                    }
                },
                UiConstructionAction::Eyedropper => {
                    if let Some(definition) = selected
                        .0
                        .and_then(|entity| placeables.get(entity).ok())
                        .map(|placeable| placeable.definition)
                    {
                        intents
                            .tools
                            .write(SelectConstructionTool(ConstructionTool::Place(definition)));
                    }
                }
                UiConstructionAction::UncrateSelected => {
                    if let Some(entity) = selected.0 {
                        intents.uncrate.write(RemoveWorldEntityFromCrate(entity));
                    }
                }
                UiConstructionAction::EnterPlacementMode => {
                    intents
                        .tools
                        .write(SelectConstructionTool(ConstructionTool::Placement));
                }
                UiConstructionAction::EnterBiomeEditing => {
                    intents
                        .tools
                        .write(SelectConstructionTool(ConstructionTool::Biome));
                }
                UiConstructionAction::EnterTerrainEditing => {
                    intents
                        .tools
                        .write(SelectConstructionTool(ConstructionTool::Terrain(
                            TerrainBrushKind::Raise,
                        )));
                }
                UiConstructionAction::EnterDeletionMode => {
                    intents
                        .tools
                        .write(SelectConstructionTool(ConstructionTool::Delete));
                }
                UiConstructionAction::EnterTankEditing => {
                    intents
                        .tools
                        .write(SelectConstructionTool(ConstructionTool::TankEdit));
                }
                UiConstructionAction::EnterFencePlacementMode => {
                    intents
                        .tools
                        .write(SelectConstructionTool(ConstructionTool::FencePlacement));
                }
                UiConstructionAction::EnterPathPlacementMode => {
                    intents
                        .tools
                        .write(SelectConstructionTool(ConstructionTool::PathPlacement));
                }
                UiConstructionAction::EnterElevatedPathPlacementMode => {
                    intents.tools.write(SelectConstructionTool(
                        ConstructionTool::ElevatedPathPlacement,
                    ));
                }
                UiConstructionAction::EnterElevatedCurbPlacementMode => {
                    intents.tools.write(SelectConstructionTool(
                        ConstructionTool::ElevatedCurbPlacement,
                    ));
                }
                UiConstructionAction::EnterSkyTrackPlacementMode => {
                    intents.tools.write(SelectConstructionTool(
                        ConstructionTool::SkyTrackPlacement(None),
                    ));
                }
                UiConstructionAction::EnterGroundTrackPlacementMode => {
                    intents.tools.write(SelectConstructionTool(
                        ConstructionTool::GroundTrackPlacement(None),
                    ));
                }
                UiConstructionAction::ConfirmPlacement { confirmed } => {
                    if *confirmed {
                        if let Some((preview, _)) = previews.iter().next() {
                            // The ordinary transaction pipeline remains the sole placement owner.
                            intents.commits.write(CommitConstruction { preview });
                        }
                    } else {
                        intents.cancellations.write(CancelConstruction);
                    }
                }
                UiConstructionAction::ResolveConfirmation { kind, confirmed } => {
                    match kind {
                        UiConstructionConfirmation::ZooGatePathDeletion
                        | UiConstructionConfirmation::TankDeletion => {
                            if *confirmed {
                                if let Some(entity) = selected.0 {
                                    intents.deletions.write(DeleteEntity(entity));
                                }
                            }
                        }
                        UiConstructionConfirmation::TankMerge => {
                            // A merge requires the two tank operands retained by the
                            // overlap operation. Reject rather than inventing a target.
                            if *confirmed {
                                intents.cancellations.write(CancelConstruction);
                            }
                        }
                        UiConstructionConfirmation::TankSplit => {
                            if *confirmed {
                                if let Some((preview, _)) = previews.iter().next() {
                                    intents.commits.write(CommitConstruction { preview });
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
