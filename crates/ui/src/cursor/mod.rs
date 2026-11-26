use bevy::{
    ecs::system::SystemParam,
    prelude::*,
    window::{CursorIcon, CustomCursor, CustomCursorImage, PrimaryWindow, SystemCursorIcon},
};
use openzt2_game_data::ui_document::document::*;
use openzt2_game_data::ui_document::gameplay_pointer_presentation::UiGameplayInteractionCursorDefinition;
use openzt2_game_data::AssetId;
use openzt2_interface_state::{
    persistence::save_slot_types::PersistenceBusy,
    settings::display_settings_types::DisplaySettings,
};
use std::collections::VecDeque;

use openzt2_game_assets::texture::interactive_texture_metadata_asset_and_borrowing_queries::InteractiveTextureMetadataAsset;
use openzt2_game_assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use openzt2_zoo_state::construction::construction_interaction_types::ConstructionPreview;
use openzt2_zoo_state::construction::construction_interaction_types::PlacementValidity;
use openzt2_zoo_state::construction::construction_tool_and_placement_policy_types::ConstructionTool;
use openzt2_zoo_state::placement::placement_preview_types::PlacementRotationRequest;
use openzt2_zoo_state::placement::placement_preview_types::RelocatingPlacedObject;
use openzt2_zoo_state::terrain::terrain_brush_types::TerrainBrushKind;

use super::authored_globe_presentation_types::{
    GlobeHoveredMarker, UiGlobeDrag, UiGlobePresentation,
};
use super::authored_ui_node_projection_components::UiDocumentOwner;
use super::authored_ui_node_projection_components::UiDocumentRoot;
use super::projection::ui_document_opening::PendingUiProjection;
use openzt2_interface_state::ui::pointer_capture_types::UiPointerCapture;

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct UiCursorStyle(pub(super) AssetId);

#[derive(Resource, Default, PartialEq)]
pub(super) struct CursorWindowScale(Option<CursorCanvas>);

#[derive(Clone, Copy, PartialEq)]
struct CursorCanvas {
    window: Entity,
    atlas_scale: f32,
    image_scale: f32,
}

#[derive(Resource, Default, PartialEq)]
pub(super) struct ResolvedCursorRequest(Option<(CursorCanvas, CursorIcon)>);

const MAX_SCALED_CURSOR_IMAGES: usize = 8;

#[derive(Resource, Default)]
pub(super) struct ScaledCursorImages(VecDeque<(CursorImageSize, Handle<Image>)>);

#[derive(Clone, Copy, PartialEq, Eq)]
struct CursorImageSize {
    atlas: bevy::asset::AssetId<Image>,
    rect: URect,
    extent: UVec2,
}

#[derive(Message)]
pub(super) struct SynthesizeCursorImage {
    window: Entity,
    source: CustomCursorImage,
    size: CursorImageSize,
    hotspot: (u16, u16),
}

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
    marker_cursor: Query<
        'w,
        's,
        (
            &'static GlobeHoveredMarker,
            &'static UiGlobePresentation,
            &'static UiDocumentOwner,
        ),
    >,
    roots: Query<'w, 's, &'static UiDocumentRoot>,
    relocating:
        Query<'w, 's, (Option<&'static PlacementRotationRequest>,), With<RelocatingPlacedObject>>,
    previews: Query<'w, 's, &'static ConstructionPreview>,
    pending_documents: Query<'w, 's, (), With<PendingUiProjection>>,
}

pub(super) fn project_cursor_window_scale(
    windows: Query<(Entity, &Window), With<PrimaryWindow>>,
    settings: Res<DisplaySettings>,
    mut canvas: ResMut<CursorWindowScale>,
) {
    let geometry = windows.single().ok().map(|(entity, window)| {
        let atlas_scale = window.scale_factor();
        // winit uses points for macOS cursor images and physical pixels elsewhere.
        let pixels_per_point = if cfg!(target_os = "macos") {
            1.0
        } else {
            atlas_scale
        };
        CursorCanvas {
            window: entity,
            atlas_scale,
            image_scale: window.height() / 768.0 * f32::from(settings.ui_scale_permille) / 1000.0
                * pixels_per_point,
        }
    });
    canvas.set_if_neq(CursorWindowScale(geometry));
}

#[derive(SystemParam)]
pub(super) struct ChangedCursorSources<'w, 's> {
    authored: Query<
        'w,
        's,
        (),
        (
            With<UiDocumentOwner>,
            Or<(
                Changed<UiCursorStyle>,
                Changed<UiGlobeDrag>,
                Changed<GlobeHoveredMarker>,
                Changed<UiDocumentOwner>,
                Changed<ChildOf>,
            )>,
        ),
    >,
    tool: Query<
        'w,
        's,
        (),
        Or<(
            Changed<ConstructionPreview>,
            Added<RelocatingPlacedObject>,
            Added<PlacementRotationRequest>,
            Changed<UiDocumentRoot>,
            Added<PendingUiProjection>,
        )>,
    >,
    removed_styles: RemovedComponents<'w, 's, UiCursorStyle>,
    removed_roots: RemovedComponents<'w, 's, UiDocumentRoot>,
    loaded_documents: RemovedComponents<'w, 's, PendingUiProjection>,
    removed_drags: RemovedComponents<'w, 's, UiGlobeDrag>,
    removed_hovers: RemovedComponents<'w, 's, GlobeHoveredMarker>,
    ended_relocation: RemovedComponents<'w, 's, RelocatingPlacedObject>,
    ended_rotation: RemovedComponents<'w, 's, PlacementRotationRequest>,
    removed_previews: RemovedComponents<'w, 's, ConstructionPreview>,
}

impl ChangedCursorSources<'_, '_> {
    fn changed(&mut self) -> bool {
        let mut changed = !self.authored.is_empty() || !self.tool.is_empty();
        macro_rules! consume_removals {
            ($($reader:ident),*) => {$(
                changed |= !self.$reader.is_empty();
                self.$reader.clear();
            )*};
        }
        consume_removals!(
            removed_styles,
            removed_roots,
            loaded_documents,
            removed_drags,
            removed_hovers,
            ended_relocation,
            ended_rotation,
            removed_previews
        );
        changed
    }
}

/// Projects the depth-resolved authored cursor directly onto Bevy's primary
/// window. The image handle remains owned by the loaded UI document.
pub(super) fn resolve_requested_window_cursor(
    phase: Res<State<openzt2_game_assets::application_lifecycle::GamePhase>>,
    busy: Query<(), With<PersistenceBusy>>,
    added_busy: Query<(), Added<PersistenceBusy>>,
    mut ended_busy: RemovedComponents<PersistenceBusy>,
    capture: Res<UiPointerCapture>,
    tool: Res<ConstructionTool>,
    sources: CursorPresentation,
    canvas: Res<CursorWindowScale>,
    mut requested: ResMut<ResolvedCursorRequest>,
    mut changed_sources: ChangedCursorSources,
) {
    let changed =
        changed_sources.changed() | (ended_busy.read().count() != 0) | !added_busy.is_empty();
    if !changed
        && !phase.is_changed()
        && !capture.is_changed()
        && !tool.is_changed()
        && !canvas.is_changed()
        && !sources.documents.is_changed()
        && !sources.texture_metadata.is_changed()
    {
        return;
    }
    let Some(window) = canvas.0 else {
        requested.set_if_neq(ResolvedCursorRequest::default());
        return;
    };
    let scale_factor = window.atlas_scale;
    let next = if !busy.is_empty()
        || *phase.get() == openzt2_game_assets::application_lifecycle::GamePhase::Loading
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
                let (hover, globe, owner) = capture
                    .target
                    .and_then(|target| sources.marker_cursor.get(target).ok())?;
                hover.0?;
                cursor(
                    &UiCursorStyle(globe.dot_highlight_cursor),
                    owner,
                    &sources.roots,
                    &sources.documents,
                    &sources.texture_metadata,
                    scale_factor,
                )
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
    requested.set_if_neq(ResolvedCursorRequest(Some((window, next))));
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
        | ConstructionTool::ElevatedCurbPlacement => policy.path.0,
        // `ZTGroundTrackMode` and `ZTSkyTrackMode` author no cursor, so the
        // overhead pointer stays; the path mode's is the blank `blank.cur`.
        ConstructionTool::SkyTrackPlacement(_) | ConstructionTool::GroundTrackPlacement(_) => {
            policy.overhead.0
        }
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
    app.init_resource::<CursorWindowScale>()
        .init_resource::<ResolvedCursorRequest>()
        .init_resource::<ScaledCursorImages>()
        .add_message::<SynthesizeCursorImage>();
}

pub(super) fn apply_cached_or_unscaled_window_cursor(
    request: Res<ResolvedCursorRequest>,
    mut events: MessageReader<AssetEvent<Image>>,
    mut cache: ResMut<ScaledCursorImages>,
    mut resize: MessageWriter<SynthesizeCursorImage>,
    mut windows: Query<Option<&mut CursorIcon>, With<PrimaryWindow>>,
    mut commands: Commands,
) {
    let source = request.0.as_ref().and_then(|(_, icon)| match icon {
        CursorIcon::Custom(CustomCursor::Image(image)) => Some(image),
        _ => None,
    });
    let mut source_changed = false;
    for event in events.read() {
        let id = match *event {
            AssetEvent::Removed { id } | AssetEvent::Modified { id } => {
                if cache.0.iter().any(|(size, _)| size.atlas == id) {
                    cache.0.retain(|(size, _)| size.atlas != id);
                }
                id
            }
            AssetEvent::Added { id } | AssetEvent::LoadedWithDependencies { id } => id,
            _ => continue,
        };
        source_changed |= source.is_some_and(|source| source.handle.id() == id);
    }
    let Some((canvas, cursor)) = &request.0 else {
        return;
    };
    if !request.is_changed() && !source_changed {
        return;
    }

    if let Some(source) = source {
        if let Some((size, hotspot)) = cursor_image_size(source, canvas.image_scale) {
            if let Some((_, image)) = cache.0.iter().find(|(cached, _)| *cached == size) {
                let icon = scaled_cursor_icon(source, image.clone(), hotspot);
                write_window_cursor(canvas.window, icon, &mut windows, &mut commands);
            } else {
                resize.write(SynthesizeCursorImage {
                    window: canvas.window,
                    source: source.clone(),
                    size,
                    hotspot,
                });
            }
            return;
        }
    }
    write_window_cursor(canvas.window, cursor.clone(), &mut windows, &mut commands);
}

fn cursor_image_size(
    source: &CustomCursorImage,
    scale: f32,
) -> Option<(CursorImageSize, (u16, u16))> {
    let rect = source.rect?;
    if !scale.is_finite() || scale <= 0.0 || (scale - 1.0).abs() < 0.01 {
        return None;
    }
    let extent = (rect.size().max(UVec2::ONE).as_vec2() * scale)
        .round()
        .max(Vec2::ONE)
        .as_uvec2();
    let scaled_hotspot = |coordinate: u16| {
        (f32::from(coordinate) * scale)
            .round()
            .min(f32::from(u16::MAX)) as u16
    };
    Some((
        CursorImageSize {
            atlas: source.handle.id(),
            rect,
            extent,
        },
        (
            scaled_hotspot(source.hotspot.0),
            scaled_hotspot(source.hotspot.1),
        ),
    ))
}

pub(super) fn synthesize_requested_cursor_image(
    mut requests: MessageReader<SynthesizeCursorImage>,
    mut images: ResMut<Assets<Image>>,
    mut cache: ResMut<ScaledCursorImages>,
    mut windows: Query<Option<&mut CursorIcon>, With<PrimaryWindow>>,
    mut commands: Commands,
) {
    for request in requests.read() {
        let size = request.size;
        let icon = if let Some(dynamic) = images
            .get(size.atlas)
            .and_then(|image| image.clone().try_into_dynamic().ok())
        {
            let rect = size.rect;
            let converted = dynamic
                .crop_imm(rect.min.x, rect.min.y, rect.width(), rect.height())
                .resize_exact(
                    size.extent.x,
                    size.extent.y,
                    image::imageops::FilterType::Triangle,
                );
            let handle = images.add(Image::from_dynamic(
                converted,
                true,
                bevy::asset::RenderAssetUsages::all(),
            ));
            if cache.0.len() == MAX_SCALED_CURSOR_IMAGES {
                cache.0.pop_front();
            }
            cache.0.push_back((size, handle.clone()));
            scaled_cursor_icon(&request.source, handle, request.hotspot)
        } else {
            CustomCursor::Image(request.source.clone()).into()
        };
        write_window_cursor(request.window, icon, &mut windows, &mut commands);
    }
}

fn scaled_cursor_icon(
    source: &CustomCursorImage,
    handle: Handle<Image>,
    hotspot: (u16, u16),
) -> CursorIcon {
    CustomCursor::Image(CustomCursorImage {
        handle,
        rect: None,
        hotspot,
        ..source.clone()
    })
    .into()
}

fn write_window_cursor(
    window: Entity,
    icon: CursorIcon,
    windows: &mut Query<Option<&mut CursorIcon>, With<PrimaryWindow>>,
    commands: &mut Commands,
) {
    match windows.get_mut(window) {
        Ok(Some(mut current)) => {
            current.set_if_neq(icon);
        }
        Ok(None) => {
            commands.entity(window).insert(icon);
        }
        Err(_) => {}
    }
}
