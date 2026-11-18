use bevy::{math::Affine2, prelude::*, window::PrimaryWindow};

use openzt2_interface_state::settings::display_settings_types::DisplaySettings;

use super::authored_tree_expansion_and_row_indentation::UiAuthoredTreeRowPhysicalIndent;

#[derive(Component, Clone, Copy)]
#[require(UiCanvasMetrics)]
pub struct UiLogicalCanvas(Vec2);

impl UiLogicalCanvas {
    pub(super) fn from_logical_size(logical_size: Vec2) -> Self {
        Self(logical_size)
    }

    pub(super) fn logical_size(&self) -> Vec2 {
        self.0
    }
}

/// Authored extent used by a legacy list refresh on its packing axis.
///
/// The source list manager writes child-cell positions after the authored
/// canvas has been mapped to the backbuffer, so a 20-unit row advances by 20
/// backbuffer pixels rather than by `20 * canvas_scale_y`. Bevy still owns the
/// layout; this component only supplies the inverse-scaled flex-item extent
/// which gives Taffy the same final packing distance.
#[derive(Component, Clone, Copy)]
pub(super) struct UiPhysicalListRowHeight(f32);

impl UiPhysicalListRowHeight {
    pub(super) fn from_physical_pixels(physical_pixels: f32) -> Self {
        Self(physical_pixels)
    }
}

/// Authored draw-3D content retains physical pixel aspect after the legacy
/// canvas is stretched independently across the window axes.
#[derive(Component)]
pub(super) struct UiPhysicalAspect;

#[derive(Component, Default, Clone, Copy, PartialEq)]
pub(super) struct UiCanvasMetrics {
    base_scale: Vec2,
    pub(super) isotropic_correction: Vec2,
}

impl UiCanvasMetrics {
    pub(super) fn has_extent(&self) -> bool {
        self.base_scale.min_element() > 0.0
    }
}

pub(super) fn project_authored_ui_canvas_viewport_metrics(
    windows: Query<Ref<Window>, With<PrimaryWindow>>,
    settings: Res<DisplaySettings>,
    mut canvases: Query<(Ref<UiLogicalCanvas>, &mut UiCanvasMetrics, &mut UiTransform)>,
    mut scale: ResMut<UiScale>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    scale.map_unchanged(|scale| &mut scale.0).set_if_neq(1.0);
    let viewport = Vec2::new(window.width(), window.height());
    let user_scale = f32::from(settings.ui_scale_permille) / 1000.0;
    for (logical, mut metrics, mut transform) in &mut canvases {
        if !window.is_changed() && !settings.is_changed() && !logical.is_changed() {
            continue;
        }
        let logical_size = logical.logical_size();
        if logical_size.min_element() <= 0.0 || viewport.min_element() <= 0.0 {
            metrics.set_if_neq(UiCanvasMetrics::default());
            continue;
        }
        let base_scale = viewport / logical_size;
        metrics.set_if_neq(UiCanvasMetrics {
            base_scale,
            isotropic_correction: Vec2::new(base_scale.y / base_scale.x, 1.0),
        });
        let center_offset = (viewport - logical_size) * 0.5;
        transform.set_if_neq(UiTransform {
            translation: Val2::px(center_offset.x, center_offset.y),
            scale: base_scale * user_scale,
            ..UiTransform::IDENTITY
        });
    }
}

pub(super) fn apply_authored_canvas_layout_corrections(
    parents: Query<&ChildOf>,
    canvases: Query<Ref<UiCanvasMetrics>>,
    mut physical_aspect_nodes: Query<
        (
            Entity,
            Ref<UiPhysicalAspect>,
            Ref<ChildOf>,
            &mut UiTransform,
        ),
        Without<UiLogicalCanvas>,
    >,
    mut physical_list_rows: Query<(
        Entity,
        Ref<UiPhysicalListRowHeight>,
        Ref<ChildOf>,
        &mut Node,
    )>,
    mut physical_tree_rows: Query<
        (
            Entity,
            Ref<UiAuthoredTreeRowPhysicalIndent>,
            Ref<ChildOf>,
            &mut UiTransform,
        ),
        (Without<UiLogicalCanvas>, Without<UiPhysicalAspect>),
    >,
) {
    let canvas_changed = canvases.iter().any(|canvas| canvas.is_changed());
    let metrics_for = |entity, member_changed| {
        parents
            .iter_ancestors::<ChildOf>(entity)
            .find_map(|ancestor| canvases.get(ancestor).ok())
            .filter(|metrics| metrics.has_extent() && (member_changed || metrics.is_changed()))
    };
    for (entity, aspect, parent, transform) in &mut physical_aspect_nodes {
        let changed = aspect.is_changed() || parent.is_changed();
        if canvas_changed || changed {
            if let Some(metrics) = metrics_for(entity, changed) {
                transform
                    .map_unchanged(|transform| &mut transform.scale)
                    .set_if_neq(metrics.isotropic_correction);
            }
        }
    }
    for (entity, height, parent, node) in &mut physical_list_rows {
        let changed = height.is_changed() || parent.is_changed();
        if canvas_changed || changed {
            if let Some(metrics) = metrics_for(entity, changed) {
                node.map_unchanged(|node| &mut node.height)
                    .set_if_neq(px(height.0 / metrics.base_scale.y));
            }
        }
    }
    for (entity, indent, parent, transform) in &mut physical_tree_rows {
        let changed = indent.is_changed() || parent.is_changed();
        if canvas_changed || changed {
            if let Some(metrics) = metrics_for(entity, changed) {
                transform
                    .map_unchanged(|transform| &mut transform.translation)
                    .set_if_neq(Val2::px(
                        indent.physical_pixels() / metrics.base_scale.x,
                        0.0,
                    ));
            }
        }
    }
}

/// Quantize final authored UI rectangle edges after the logical canvas transform.
///
/// Bevy rounds layout before `UiTransform` is applied. The independently scaled
/// 1024x768 canvas can therefore put two otherwise identical shared edges between
/// physical pixels. Rebuilding each final axis-aligned transform from its rounded
/// edges keeps adjacent authored rectangles coincident for rendering and picking.
pub(super) fn snap_authored_ui_canvas_descendant_edges_to_physical_pixels(
    parents: Query<&ChildOf>,
    mut transforms: ParamSet<(
        Query<(Entity, &UiGlobalTransform), With<UiLogicalCanvas>>,
        Query<
            (Entity, &ComputedNode, &mut UiGlobalTransform),
            Or<(
                Changed<ComputedNode>,
                Changed<UiGlobalTransform>,
                Changed<ChildOf>,
            )>,
        >,
    )>,
    mut canvas_transforms: Local<bevy::ecs::entity::EntityHashMap<(Affine2, Affine2)>>,
) {
    canvas_transforms.clear();
    for (entity, transform) in &transforms.p0() {
        let canvas = transform.affine();
        if canvas.matrix2.determinant().abs() > f32::EPSILON {
            canvas_transforms.insert(entity, (canvas, canvas.inverse()));
        }
    }
    for (entity, computed, mut transform) in &mut transforms.p1() {
        let Some(&(canvas_affine, authored_from_physical)) = std::iter::once(entity)
            .chain(parents.iter_ancestors::<ChildOf>(entity))
            .find_map(|ancestor| canvas_transforms.get(&ancestor))
        else {
            continue;
        };
        let size = computed.size();
        if size.min_element() <= 0.0 {
            continue;
        }
        let affine = transform.affine();
        if affine.matrix2.x_axis.y.abs() > f32::EPSILON
            || affine.matrix2.y_axis.x.abs() > f32::EPSILON
        {
            continue;
        }
        let half_size = size * 0.5;
        let first_corner = affine.transform_point2(-half_size);
        let second_corner = affine.transform_point2(half_size);
        // Independent transform multiplication can leave the same authored
        // edge on opposite sides of a physical half-pixel. Recover and
        // quantize the authored edge first so every owner of that edge is
        // mapped through one identical canvas projection.
        let first_authored = authored_from_physical
            .transform_point2(first_corner)
            .round();
        let second_authored = authored_from_physical
            .transform_point2(second_corner)
            .round();
        let first_physical = canvas_affine.transform_point2(first_authored);
        let second_physical = canvas_affine.transform_point2(second_authored);
        let minimum = first_physical.min(second_physical).round();
        let maximum = first_physical.max(second_physical).round();
        let snapped_size = maximum - minimum;
        if snapped_size.min_element() <= 0.0 {
            continue;
        }
        let signed_scale = Vec2::new(
            snapped_size.x.copysign(affine.matrix2.x_axis.x) / size.x,
            snapped_size.y.copysign(affine.matrix2.y_axis.y) / size.y,
        );
        let snapped_transform =
            Affine2::from_scale_angle_translation(signed_scale, 0.0, (minimum + maximum) * 0.5)
                .into();
        if *transform != snapped_transform {
            *transform = snapped_transform;
        }
    }
}
