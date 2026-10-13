use bevy::{ecs::system::SystemParam, prelude::*, window::PrimaryWindow};
use openzt2_game_data::{ui_document::document::UiDocumentRole, AssetId};

use openzt2_game_assets::application_lifecycle::GamePhase;
use openzt2_game_assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use openzt2_interface_state::information::catalogue_types::CatalogueRowEntry;
use openzt2_interface_state::ui::authored_reusable_list_and_table_runtime_types::UiListRow;
use openzt2_interface_state::ui::authored_reusable_list_and_table_runtime_types::UiTypeListPolicy;
use openzt2_interface_state::ui::authored_ui_interaction_enabled_state::UiInteractionEnabled;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentOwner;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentRoot;
use openzt2_interface_state::ui::pointer_capture_types::UiPointerCapture;
use openzt2_interface_state::ui::pointer_capture_types::UiWheelCapture;
use openzt2_interface_state::ui::ui_document_lifecycle_contracts::InGameUiOwner;
use openzt2_world_state::world_spawn::world_membership_types::WorldRoot;

use super::verification_pointer_reach::VerificationPointerReach;

use super::verification_journey_state_types::{
    VerificationFactCollectionState, VerificationJourneyReport, VerificationJourneySession,
};

#[derive(Clone, Copy)]
pub(super) struct CatalogueWheelInputPlan {
    pub(super) row: Entity,
    pub(super) list: Entity,
    pub(super) point: Vec2,
    pub(super) pixels: f32,
}

#[derive(SystemParam)]
pub(super) struct VerificationCatalogueWheelInput<'w, 's> {
    lists: Query<
        'w,
        's,
        (
            Entity,
            &'static ComputedNode,
            &'static UiGlobalTransform,
            &'static Node,
            &'static ScrollPosition,
            &'static InheritedVisibility,
            Option<&'static CalculatedClip>,
        ),
        With<UiTypeListPolicy>,
    >,
    rows: Query<
        'w,
        's,
        (
            Entity,
            &'static UiListRow,
            &'static CatalogueRowEntry,
            &'static ComputedNode,
            &'static UiGlobalTransform,
            &'static InheritedVisibility,
            Option<&'static CalculatedClip>,
            Option<&'static UiDocumentOwner>,
            Option<&'static UiInteractionEnabled>,
        ),
    >,
    windows: Query<'w, 's, &'static Window, With<PrimaryWindow>>,
    list_owners: Query<'w, 's, (&'static Name, &'static UiDocumentOwner)>,
    roots: Query<
        'w,
        's,
        (
            &'static UiDocumentRoot,
            &'static ChildOf,
            &'static InheritedVisibility,
        ),
    >,
    worlds: Query<'w, 's, Entity, With<WorldRoot>>,
    game_views: Query<'w, 's, Entity, With<InGameUiOwner>>,
    documents: Res<'w, Assets<UiDocumentAsset>>,
    phase: Res<'w, State<GamePhase>>,
    capture: Res<'w, UiPointerCapture>,
}

impl VerificationCatalogueWheelInput<'_, '_> {
    pub(super) fn current_world(&self) -> Option<Entity> {
        self.worlds.single().ok()
    }

    pub(super) fn resolve_active_hud_list(&self, list_name: &str) -> Result<Entity, String> {
        if *self.phase.get() != GamePhase::InGame {
            return Err("catalogue list scope requires the in-game HUD".to_owned());
        }
        let game_view = self.game_views.single().map_err(|_| {
            "catalogue list scope requires exactly one current game-view owner".to_owned()
        })?;
        let mut matching_list = None;
        for (list, computed, transform, _, _, visible, clip) in &self.lists {
            let Ok((name, owner)) = self.list_owners.get(list) else {
                continue;
            };
            if name.as_str() != list_name {
                continue;
            }
            let Ok((root, parent, root_visible)) = self.roots.get(owner.0) else {
                continue;
            };
            if parent.parent() != game_view
                || !root_visible.get()
                || !self.documents.get(&root.document).is_some_and(|document| {
                    document.canonical_ui_document().role == UiDocumentRole::InGameHud
                })
                || self
                    .visible_list_viewport(computed, transform, visible, clip)
                    .is_none()
            {
                continue;
            }
            if matching_list.replace(list).is_some() {
                return Err(format!(
                    "multiple active in-game HUD lists match \"{list_name}\""
                ));
            }
        }
        matching_list.ok_or_else(|| {
            format!("no active in-game HUD list with a visible viewport matches \"{list_name}\"")
        })
    }

    /// Plan one native pixel-wheel event from the current row and viewport.
    /// This observes offscreen rows but never presses them or writes scroll state.
    pub(super) fn plan_row_wheel_input(
        &self,
        definition: AssetId,
        list_scope: Option<Entity>,
    ) -> Result<CatalogueWheelInputPlan, String> {
        let mut matching_plan = None;
        for (entity, row, entry, row_node, row_transform, visible, ..) in &self.rows {
            if entry.definition != definition
                || list_scope.is_some_and(|list| row.list != list)
                || !visible.get()
                || row_node.is_empty()
            {
                continue;
            }
            let Ok((list, computed, transform, node, _, visible, clip)) = self.lists.get(row.list)
            else {
                continue;
            };
            let Some(viewport) = self.visible_list_viewport(computed, transform, visible, clip)
            else {
                continue;
            };
            let point = viewport.center();
            let row_local = transform
                .inverse()
                .transform_point2(row_transform.translation);
            let point_local = transform.inverse().transform_point2(point);
            let distance = if node.overflow.x == OverflowAxis::Scroll
                && node.overflow.y != OverflowAxis::Scroll
            {
                row_local.x - point_local.x
            } else if node.overflow.y == OverflowAxis::Scroll {
                row_local.y - point_local.y
            } else {
                return Err("the catalogue list has no projected scroll axis".to_owned());
            };
            let plan = CatalogueWheelInputPlan {
                row: entity,
                list,
                point,
                pixels: -distance * computed.inverse_scale_factor(),
            };
            if list_scope.is_none() {
                return Ok(plan);
            }
            if matching_plan.replace(plan).is_some() {
                return Err(
                    "multiple rows in the scoped catalogue list match the requested definition"
                        .to_owned(),
                );
            }
        }
        if let Some(plan) = matching_plan {
            return Ok(plan);
        }
        Err("the open catalogue has no populated row and visible viewport for the requested definition".to_owned())
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn log_catalogue_wheel_row_boundary(
        &self,
        row_entity: Entity,
        definition: AssetId,
        expected_list: Entity,
        reach: &VerificationPointerReach,
        stage: &str,
        frame: u64,
        pixels: Option<f32>,
    ) {
        let Ok((_, row, entry, row_node, row_transform, row_visible, row_clip, row_owner, enabled)) =
            self.rows.get(row_entity)
        else {
            info!(target: "openzt2_verification", stage, frame, ?definition, ?row_entity,
                ?expected_list, "catalogue wheel row no longer available");
            return;
        };
        let Ok((list, computed, transform, node, position, visible, clip)) =
            self.lists.get(expected_list)
        else {
            info!(target: "openzt2_verification", stage, frame, ?definition, ?row_entity,
                ?expected_list, "catalogue wheel list no longer available");
            return;
        };
        let list_owner = self.list_owners.get(list).ok();
        let root = list_owner.and_then(|(_, owner)| self.roots.get(owner.0).ok());
        let row_root = row_owner.and_then(|owner| self.roots.get(owner.0).ok());
        let viewport = self.visible_list_viewport(computed, transform, visible, clip);
        let row_bounds = Rect::from_corners(
            row_transform
                .affine()
                .transform_point2(-row_node.size() * 0.5),
            row_transform
                .affine()
                .transform_point2(row_node.size() * 0.5),
        );
        info!(target: "openzt2_verification", stage, frame, ?definition, ?row_entity,
            row_index = row.index, actual_list = ?row.list, expected_list = ?list,
            definition_matches = entry.definition == definition, list_matches = row.list == list,
            list_name = ?list_owner.map(|(name, _)| name.as_str()),
            document_owner = ?list_owner.map(|(_, owner)| owner.0),
            document = ?root.map(|(root, ..)| root.document.id()),
            document_role = ?root.and_then(|(root, ..)| self.documents.get(&root.document))
                .map(|document| document.canonical_ui_document().role),
            hud_owner = ?root.map(|(_, parent, _)| parent.parent()),
            row_document_owner = ?row_owner.map(|owner| owner.0),
            row_document = ?row_root.map(|(root, ..)| root.document.id()),
            row_document_parent = ?row_root.map(|(_, parent, _)| parent.parent()),
            world = ?self.current_world(),
            list_transform = ?transform, row_transform = ?row_transform,
            axis_x = ?node.overflow.x, axis_y = ?node.overflow.y,
            ?viewport, ?row_bounds, row_center = ?row_transform.translation,
            row_clip = ?row_clip.map(|clip| clip.clip),
            row_visible = row_visible.get(), row_empty = row_node.is_empty(),
            enabled = enabled.is_none_or(|enabled| enabled.0),
            clipped_at_center = reach.is_clipped_at(row_entity, row_transform.translation),
            scale_inverse = computed.inverse_scale_factor(), ?pixels,
            content_size = ?computed.content_size(), viewport_size = ?computed.size(),
            overflow = ?(computed.content_size() - computed.size()),
            offset = ?position.0, wheel_receiver = ?self.capture.wheel,
            "catalogue wheel boundary; geometry is the latest projected layout at this schedule");
    }

    pub(super) fn log_catalogue_selector_failure(
        &self,
        definition: AssetId,
        list_name: Option<&str>,
        reach: &VerificationPointerReach,
        frame: u64,
    ) {
        let Some(list_name) = list_name else {
            return;
        };
        let list = match self.resolve_active_hud_list(list_name) {
            Ok(list) => list,
            Err(reason) => {
                info!(target: "openzt2_verification", frame, ?definition, list_name, %reason,
                    "catalogue selector failure has no unique active HUD list");
                return;
            }
        };
        let mut count = 0usize;
        for (entity, row, entry, ..) in &self.rows {
            if row.list == list && entry.definition == definition {
                count += 1;
                if count <= 2 {
                    self.log_catalogue_wheel_row_boundary(
                        entity,
                        definition,
                        list,
                        reach,
                        "selector-failure",
                        frame,
                        None,
                    );
                }
            }
        }
        info!(target: "openzt2_verification", frame, ?definition, ?list,
            matching_rows = count, sample_limit = 2, truncated = count.saturating_sub(2),
            "catalogue selector failure row inventory");
    }

    pub(super) fn pointer_wheel_reaches_list(&self, list: Entity) -> bool {
        self.capture.wheel == UiWheelCapture::Scroll(list)
    }

    fn visible_list_viewport(
        &self,
        node: &ComputedNode,
        transform: &UiGlobalTransform,
        visible: &InheritedVisibility,
        clip: Option<&CalculatedClip>,
    ) -> Option<Rect> {
        if !visible.get() || node.is_empty() {
            return None;
        }
        let window = self.windows.single().ok()?;
        let bounds = Rect::from_corners(
            transform.affine().transform_point2(-node.size() * 0.5),
            transform.affine().transform_point2(node.size() * 0.5),
        );
        let viewport = Rect::from_corners(Vec2::ZERO, window.physical_size().as_vec2());
        let bounds = bounds.intersect(viewport);
        let bounds = clip.map_or(bounds, |clip| bounds.intersect(clip.clip));
        (!bounds.is_empty()).then_some(bounds)
    }
}

pub(super) fn collect_catalogue_scroll_verification_facts(
    session: Res<VerificationJourneySession>,
    mut report: ResMut<VerificationJourneyReport>,
    catalogue: VerificationCatalogueWheelInput,
) {
    if session.fact_collection != VerificationFactCollectionState::Collected {
        return;
    }
    let mut scroll = Vec2::ZERO;
    let mut first_unclipped_row = None::<u16>;
    let mut burger = false;
    let mut gift = false;
    let enrichment_list = catalogue
        .resolve_active_hud_list("Enrichment Typelist")
        .ok();
    let mut enrichment_ball = false;
    for (list, node, transform, _, position, visible, clip) in &catalogue.lists {
        let Some(viewport) = catalogue.visible_list_viewport(node, transform, visible, clip) else {
            continue;
        };
        scroll += position.0;
        for (_, row, entry, node, transform, visible, clip, ..) in &catalogue.rows {
            if row.list != list
                || !visible.get()
                || node.is_empty()
                || !viewport.contains(transform.translation)
                || clip.is_some_and(|clip| !clip.clip.contains(transform.translation))
            {
                continue;
            }
            first_unclipped_row =
                Some(first_unclipped_row.map_or(row.index, |first| first.min(row.index)));
            burger |= entry.definition == AssetId::from_key("foodstand_burger_df");
            gift |= entry.definition == AssetId::from_key("giftcart_df");
            enrichment_ball |=
                Some(list) == enrichment_list && entry.definition == AssetId::from_key("ball");
        }
    }
    for (name, value) in [
        ("catalogue_scroll_x", f64::from(scroll.x)),
        ("catalogue_scroll_y", f64::from(scroll.y)),
        (
            "catalogue_first_unclipped_row",
            first_unclipped_row.map_or(-1.0, f64::from),
        ),
        ("catalogue_burger_unclipped", f64::from(u8::from(burger))),
        ("catalogue_gift_unclipped", f64::from(u8::from(gift))),
        (
            "catalogue_enrichment_list_ready",
            f64::from(u8::from(enrichment_list.is_some())),
        ),
        (
            "catalogue_enrichment_ball_unclipped",
            f64::from(u8::from(enrichment_ball)),
        ),
        (
            "catalogue_ibex_wheel_ready",
            f64::from(u8::from(
                catalogue
                    .resolve_active_hud_list("Animal Typelist")
                    .and_then(|list| {
                        catalogue.plan_row_wheel_input(AssetId::from_key("ibex"), Some(list))
                    })
                    .is_ok(),
            )),
        ),
        (
            "catalogue_bamboo_wheel_ready",
            f64::from(u8::from(
                catalogue
                    .resolve_active_hud_list("Food Typelist")
                    .and_then(|list| {
                        catalogue
                            .plan_row_wheel_input(AssetId::from_key("trough_bamboo"), Some(list))
                    })
                    .is_ok(),
            )),
        ),
    ] {
        report.current_facts.insert(name.to_owned(), value);
    }
}
