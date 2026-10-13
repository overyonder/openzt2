use std::any::TypeId;

use bevy::{
    camera::visibility::VisibleEntities, ecs::system::SystemParam, prelude::*,
    window::PrimaryWindow,
};
use openzt2_game_data::AssetId;

use openzt2_information::catalogue::catalogue_preview_scene_projection::{
    CataloguePreviewOwner, CataloguePreviewScene,
};

use openzt2_interface_state::information::catalogue_types::CatalogueRowEntry;
use openzt2_interface_state::information::catalogue_types::SelectedCatalogueEntry;
use openzt2_interface_state::ui::authored_reusable_list_and_table_runtime_types::UiListRow;
use openzt2_interface_state::ui::authored_ui_selection_state::UiSelected;
use openzt2_zoo_state::construction::construction_interaction_types::ConstructionPreview;

use super::{
    verification_catalogue_wheel_input::VerificationCatalogueWheelInput,
    verification_journey_state_types::{
        VerificationFactCollectionState, VerificationJourneyReport, VerificationJourneySession,
    },
};

type SelectedCatalogueRowQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static CatalogueRowEntry,
        &'static UiListRow,
        &'static UiSelected,
        &'static ComputedNode,
        &'static UiGlobalTransform,
        &'static InheritedVisibility,
        Option<&'static CalculatedClip>,
    ),
>;

#[derive(SystemParam)]
pub(super) struct CatalogueSelectionEvidenceQueries<'w, 's> {
    selected: Res<'w, SelectedCatalogueEntry>,
    rows: SelectedCatalogueRowQuery<'w, 's>,
    placements: Query<'w, 's, &'static ConstructionPreview>,
    previews: Query<'w, 's, (&'static CataloguePreviewScene, &'static InheritedVisibility)>,
    scenes: Query<'w, 's, (), (With<CataloguePreviewOwner>, Without<Camera>)>,
    cameras: Query<'w, 's, (&'static ChildOf, &'static Camera, &'static VisibleEntities)>,
    children: Query<'w, 's, &'static Children>,
    rendered_meshes: Query<'w, 's, &'static Mesh3d>,
    meshes: Res<'w, Assets<Mesh>>,
    windows: Query<'w, 's, &'static Window, With<PrimaryWindow>>,
    catalogue: VerificationCatalogueWheelInput<'w, 's>,
}

pub(super) fn collect_catalogue_selection_verification_facts(
    session: Res<VerificationJourneySession>,
    mut report: ResMut<VerificationJourneyReport>,
    queries: CatalogueSelectionEvidenceQueries,
) {
    if session.fact_collection != VerificationFactCollectionState::Collected {
        return;
    }
    let CatalogueSelectionEvidenceQueries {
        selected,
        rows,
        placements,
        previews,
        scenes,
        cameras,
        children,
        rendered_meshes,
        meshes,
        windows,
        catalogue,
    } = queries;
    let viewport = windows
        .single()
        .ok()
        .map(|window| Rect::from_corners(Vec2::ZERO, window.physical_size().as_vec2()));
    let mut selected_visible_first_rows = 0_usize;
    let enrichment_list = catalogue
        .resolve_active_hud_list("Enrichment Typelist")
        .ok();
    let mut enrichment_selected_ball_visible_rows = 0_usize;
    let selected_visible_rows = rows
        .iter()
        .filter(
            |(entry, row, highlighted, node, transform, visible, clip)| {
                let bounds = Rect::from_corners(
                    transform.affine().transform_point2(-node.size() * 0.5),
                    transform.affine().transform_point2(node.size() * 0.5),
                );
                let matching_visible_row = selected.0 == Some(entry.definition)
                    && highlighted.0
                    && visible.get()
                    && !node.is_empty()
                    && viewport.is_some_and(|viewport| {
                        viewport.contains(bounds.min) && viewport.contains(bounds.max)
                    })
                    && clip.is_none_or(|clip| {
                        clip.clip.contains(bounds.min) && clip.clip.contains(bounds.max)
                    });
                if matching_visible_row && row.index == 0 {
                    selected_visible_first_rows += 1;
                }
                if matching_visible_row
                    && Some(row.list) == enrichment_list
                    && entry.definition == AssetId::from_key("ball")
                {
                    enrichment_selected_ball_visible_rows += 1;
                }
                matching_visible_row
            },
        )
        .count();
    let matching_placements = placements
        .iter()
        .filter(|preview| selected.0 == Some(preview.definition))
        .count();
    let mut matching_preview_meshes = 0_u32;
    for (preview, visible) in &previews {
        if !visible.get() || selected.0 != Some(preview.definition()) {
            continue;
        }
        for (parent, camera, visible_entities) in &cameras {
            if parent.parent() != preview.scene() || !camera.is_active {
                continue;
            }
            for entity in children.iter_descendants_depth_first::<Children>(preview.scene()) {
                if visible_entities
                    .get(TypeId::of::<Mesh3d>())
                    .contains(&entity)
                    && rendered_meshes.get(entity).is_ok_and(|mesh| {
                        meshes
                            .get(&mesh.0)
                            .is_some_and(|mesh| mesh.count_vertices() > 0)
                    })
                {
                    matching_preview_meshes += 1;
                }
            }
        }
    }
    let count = |value| f64::from(u32::try_from(value).unwrap_or(u32::MAX));
    for (name, value) in [
        (
            "catalogue_selected_is_aardvark",
            f64::from(u8::from(selected.0 == Some(AssetId::from_key("aardvark")))),
        ),
        (
            "catalogue_selected_is_ibex",
            f64::from(u8::from(selected.0 == Some(AssetId::from_key("ibex")))),
        ),
        (
            "catalogue_selected_is_bamboo_trough",
            f64::from(u8::from(
                selected.0 == Some(AssetId::from_key("trough_bamboo")),
            )),
        ),
        (
            "catalogue_selected_is_ball",
            f64::from(u8::from(selected.0 == Some(AssetId::from_key("ball")))),
        ),
        (
            "catalogue_selected_visible_rows",
            count(selected_visible_rows),
        ),
        (
            "catalogue_enrichment_selected_ball_visible_rows",
            count(enrichment_selected_ball_visible_rows),
        ),
        (
            "catalogue_selected_visible_first_rows",
            count(selected_visible_first_rows),
        ),
        (
            "catalogue_matching_placement_previews",
            count(matching_placements),
        ),
        (
            "catalogue_detail_preview_scenes",
            count(scenes.iter().count()),
        ),
        (
            "catalogue_detail_preview_matching_visible_meshes",
            f64::from(matching_preview_meshes),
        ),
    ] {
        report.current_facts.insert(name.to_owned(), value);
    }
    // These observe canonical subject and renderer readiness. Captures still
    // decide whether the portrait/model and selection highlight are readable.
}
