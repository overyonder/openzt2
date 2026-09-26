use bevy::{
    ecs::system::SystemParam,
    prelude::*,
    window::{CursorIcon, CustomCursor, CustomCursorImage, PrimaryWindow, SystemCursorIcon},
};
use openzt2_game_data::ui_document::document::*;
use openzt2_game_data::ui_document::gameplay_pointer_presentation::UiGameplayInteractionCursorDefinition;
use openzt2_game_data::AssetId;

use crate::assets::{
    texture::interactive_texture_metadata_asset_and_borrowing_queries::InteractiveTextureMetadataAsset,
    ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset,
};
use crate::plugins::{
    construction::{
        construction_interaction_types::{ConstructionPreview, PlacementValidity},
        construction_tool_and_placement_policy_types::ConstructionTool,
    },
    placement::placement_preview_types::{PlacementRotationRequest, RelocatingPlacedObject},
    terrain::terrain_brush_types::TerrainBrushKind,
};

use super::authored_globe_presentation_types::{UiGlobeDrag, UiGlobePresentation};
use super::{
    authored_ui_node_projection_components::UiDocumentOwner,
    authored_ui_node_projection_components::UiDocumentRoot, picking::UiPointerCapture,
    projection::PendingUiProjection,
};

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct UiCursorStyle(pub(super) AssetId);

/// Requests the original main-mode busy cursor presentation.
///
/// The operation that is busy remains the producer's state. UI owns only the
/// resulting window presentation and therefore does not retain an operation
/// registry or a second application mode.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SetWaitCursor(pub bool);

#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct WaitCursor(bool);

#[derive(SystemParam)]
pub(super) struct CursorPresentation<'w, 's> {
    documents: Res<'w, Assets<UiDocumentAsset>>,
    texture_metadata: Res<'w, Assets<InteractiveTextureMetadataAsset>>,
    styled_nodes: Query<'w, 's, (Entity, &'static UiCursorStyle, &'static UiDocumentOwner)>,
    parents: Query<'w, 's, &'static ChildOf>,
    dragging_globes: Query<
        'w,
        's,
        (
            &'static UiCursorStyle,
            &'static UiDocumentOwner,
            &'static UiGlobeDrag,
        ),
        With<UiGlobePresentation>,
    >,
    roots: Query<'w, 's, &'static UiDocumentRoot>,
    relocating:
        Query<'w, 's, (Option<&'static PlacementRotationRequest>,), With<RelocatingPlacedObject>>,
    previews: Query<'w, 's, &'static ConstructionPreview>,
    pending_documents: Query<'w, 's, (), With<PendingUiProjection>>,
}

pub(super) fn apply_wait_cursor_requests(
    mut requests: MessageReader<SetWaitCursor>,
    mut wait: ResMut<WaitCursor>,
) {
    for request in requests.read() {
        wait.0 = request.0;
    }
}

/// Projects the depth-resolved authored cursor directly onto Bevy's primary
/// window. The image handle remains owned by the loaded UI document.
pub(super) fn present_cursor(
    phase: Res<State<crate::application_lifecycle::GamePhase>>,
    wait: Res<WaitCursor>,
    capture: Res<UiPointerCapture>,
    tool: Res<ConstructionTool>,
    sources: CursorPresentation,
    window: Single<(Entity, &Window, Option<&CursorIcon>), With<PrimaryWindow>>,
    mut commands: Commands,
) {
    let scale_factor = window.1.scale_factor();
    let next = if wait.0
        || *phase.get() == crate::application_lifecycle::GamePhase::Loading
        || !sources.pending_documents.is_empty()
    {
        CursorIcon::System(SystemCursorIcon::Wait)
    } else {
        sources
            .dragging_globes
            .iter()
            .find_map(|(style, owner, drag)| {
                drag.active
                    .then(|| {
                        cursor(
                            style,
                            owner,
                            &sources.roots,
                            &sources.documents,
                            &sources.texture_metadata,
                            scale_factor,
                        )
                    })
                    .flatten()
            })
            .or_else(|| {
                authored_cursor(
                    capture.target,
                    &sources.styled_nodes,
                    &sources.parents,
                    &sources.roots,
                    &sources.documents,
                    &sources.texture_metadata,
                    scale_factor,
                )
            })
            .or_else(|| {
                capture.over_ui.then(|| {
                    application_root_cursor(
                        &sources.styled_nodes,
                        &sources.roots,
                        &sources.documents,
                        &sources.texture_metadata,
                        scale_factor,
                    )
                })?
            })
            .or_else(|| {
                interaction_cursor(
                    &tool,
                    capture.over_ui,
                    &sources.relocating,
                    &sources.previews,
                    &sources.roots,
                    &sources.documents,
                    &sources.texture_metadata,
                    scale_factor,
                )
            })
            .or_else(|| {
                application_root_cursor(
                    &sources.styled_nodes,
                    &sources.roots,
                    &sources.documents,
                    &sources.texture_metadata,
                    scale_factor,
                )
            })
            .unwrap_or(CursorIcon::System(SystemCursorIcon::Default))
    };
    let (window, _, current) = *window;
    if current != Some(&next) {
        commands.entity(window).insert(next);
    }
}

fn interaction_cursor(
    tool: &ConstructionTool,
    over_ui: bool,
    relocating: &Query<(Option<&PlacementRotationRequest>,), With<RelocatingPlacedObject>>,
    previews: &Query<&ConstructionPreview>,
    roots: &Query<&UiDocumentRoot>,
    documents: &Assets<UiDocumentAsset>,
    texture_metadata: &Assets<InteractiveTextureMetadataAsset>,
    scale_factor: f32,
) -> Option<CursorIcon> {
    let document = roots.iter().find_map(|root| {
        let document = documents.get(&root.document)?;
        matches!(
            &document.canonical_ui_document().role,
            UiDocumentRole::InGameHud
        )
        .then_some(document)
    })?;
    let policy = &&document
        .canonical_ui_document()
        .gameplay_interaction_cursors;
    let id = if over_ui {
        AssetId(policy.overhead.0)
    } else {
        interaction_cursor_id(tool, policy, relocating, previews)
    };
    cursor_from_document(id, document, texture_metadata, scale_factor)
}

fn interaction_cursor_id(
    tool: &ConstructionTool,
    policy: &UiGameplayInteractionCursorDefinition,
    relocating: &Query<(Option<&PlacementRotationRequest>,), With<RelocatingPlacedObject>>,
    previews: &Query<&ConstructionPreview>,
) -> AssetId {
    let id = match tool {
        ConstructionTool::Inspect => {
            relocating
                .iter()
                .next()
                .map_or(policy.selection_default.0, |(rotation,)| {
                    if rotation.is_some() {
                        policy.selection_rotate.0
                    } else {
                        policy.selection_pickup.0
                    }
                })
        }
        ConstructionTool::Placement => policy.placement_default.0,
        ConstructionTool::Biome => policy.biome_default.0,
        ConstructionTool::TankEdit => policy.tank.0,
        ConstructionTool::FencePlacement | ConstructionTool::Fence(_) => policy.fence_default.0,
        ConstructionTool::PathPlacement
        | ConstructionTool::Path(_)
        | ConstructionTool::ElevatedCurbPlacement
        | ConstructionTool::SkyTrackPlacement(_)
        | ConstructionTool::GroundTrackPlacement(_) => policy.path.0,
        ConstructionTool::ElevatedPathPlacement => policy.elevated_path.0,
        ConstructionTool::Place(_) => {
            relocating
                .iter()
                .next()
                .map_or(policy.placement_default.0, |(rotation,)| {
                    if rotation.is_some() {
                        policy.placement_rotate.0
                    } else {
                        policy.placement_pickup.0
                    }
                })
        }
        ConstructionTool::Terrain(kind) => {
            if matches!(
                kind,
                TerrainBrushKind::Paint { .. } | TerrainBrushKind::PaintWater { .. }
            ) {
                if previews
                    .iter()
                    .any(|preview| matches!(preview.validity, PlacementValidity::Invalid(_)))
                {
                    policy.biome_invalid.0
                } else {
                    policy.biome_paint.0
                }
            } else {
                policy.terrain.0
            }
        }
        ConstructionTool::Delete => policy.delete.0,
    };
    AssetId(id)
}

fn authored_cursor(
    target: Option<Entity>,
    styled_nodes: &Query<(Entity, &UiCursorStyle, &UiDocumentOwner)>,
    parents: &Query<&ChildOf>,
    roots: &Query<&UiDocumentRoot>,
    documents: &Assets<UiDocumentAsset>,
    texture_metadata: &Assets<InteractiveTextureMetadataAsset>,
    scale_factor: f32,
) -> Option<CursorIcon> {
    let mut entity = target?;
    loop {
        if let Ok((_, style, owner)) = styled_nodes.get(entity) {
            return cursor(
                style,
                owner,
                roots,
                documents,
                texture_metadata,
                scale_factor,
            );
        }
        entity = parents.get(entity).ok()?.parent();
    }
}

fn application_root_cursor(
    styled_nodes: &Query<(Entity, &UiCursorStyle, &UiDocumentOwner)>,
    roots: &Query<&UiDocumentRoot>,
    documents: &Assets<UiDocumentAsset>,
    texture_metadata: &Assets<InteractiveTextureMetadataAsset>,
    scale_factor: f32,
) -> Option<CursorIcon> {
    styled_nodes.iter().find_map(|(entity, style, owner)| {
        (entity == owner.0)
            .then(|| roots.get(entity).ok())
            .flatten()
            .and_then(|root| documents.get(&root.document))
            .filter(|document| {
                matches!(
                    document.canonical_ui_document().role,
                    UiDocumentRole::ApplicationRoot
                )
            })
            .and_then(|document| {
                cursor_from_document(style.0, document, texture_metadata, scale_factor)
            })
    })
}

fn cursor(
    style: &UiCursorStyle,
    owner: &UiDocumentOwner,
    roots: &Query<&UiDocumentRoot>,
    documents: &Assets<UiDocumentAsset>,
    texture_metadata: &Assets<InteractiveTextureMetadataAsset>,
    scale_factor: f32,
) -> Option<CursorIcon> {
    let document = documents.get(&roots.get(owner.0).ok()?.document)?;
    cursor_from_document(style.0, document, texture_metadata, scale_factor)
}

fn cursor_from_document(
    id: AssetId,
    document: &UiDocumentAsset,
    texture_metadata: &Assets<InteractiveTextureMetadataAsset>,
    scale_factor: f32,
) -> Option<CursorIcon> {
    let (handle, entry) = document
        .cloned_interactive_texture_metadata_handle(id)
        .and_then(|metadata| texture_metadata.get(&metadata))
        .and_then(|metadata| {
            metadata.cursor_atlas_image_and_nearest_entry_for_window_scale_factor(scale_factor)
        })?;
    Some(
        CustomCursor::Image(CustomCursorImage {
            handle: handle.clone(),
            rect: Some(bevy::math::URect::from_corners(
                bevy::math::UVec2::from_array(entry.origin),
                bevy::math::UVec2::from_array([
                    entry.origin[0] + entry.size[0],
                    entry.origin[1] + entry.size[1],
                ]),
            )),
            hotspot: (entry.hotspot[0], entry.hotspot[1]),
            ..Default::default()
        })
        .into(),
    )
}

pub(super) fn initialize(app: &mut App) {
    app.init_resource::<WaitCursor>();
}
