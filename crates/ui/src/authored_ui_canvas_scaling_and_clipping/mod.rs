use bevy::{math::Affine2, prelude::*, window::PrimaryWindow};

use openzt2_interface_state::settings::display_settings_types::DisplaySettings;

use super::authored_tree_expansion_and_row_indentation::UiAuthoredTreeRowPhysicalIndent;

#[derive(Component, Clone, Copy)]
#[require(UiCanvasMetrics, UiCanvasPixelTransform)]
pub(super) struct UiLogicalCanvas(Vec2);

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
    scale: ResMut<UiScale>,
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

#[derive(Component)]
#[relationship(relationship_target = UiCanvasMembers, allow_self_referential)]
pub(super) struct UiCanvasMembership(pub(super) Entity);

#[derive(Component)]
#[relationship_target(relationship = UiCanvasMembership)]
pub(super) struct UiCanvasMembers(Vec<Entity>);

#[derive(Component, Default, PartialEq)]
pub(super) struct UiCanvasPixelTransform(Option<(Affine2, Affine2)>);

pub(super) fn link_ui_canvas_members(
    mut commands: Commands,
    nodes: Query<(), With<Node>>,
    changed: Query<Entity, (With<Node>, Or<(Changed<ChildOf>, Added<UiLogicalCanvas>)>)>,
    mut detached: RemovedComponents<ChildOf>,
    parents: Query<&ChildOf>,
    descendants: Query<&Children>,
    canvases: Query<(), With<UiLogicalCanvas>>,
    membership: Query<&UiCanvasMembership>,
) {
    let mut roots = bevy::ecs::entity::EntityHashSet::default();
    roots.extend(changed.iter());
    roots.extend(detached.read().filter(|node| nodes.contains(*node)));
    for root in roots.iter().copied().filter(|node| {
        parents
            .iter_ancestors::<ChildOf>(*node)
            .all(|ancestor| !roots.contains(&ancestor))
    }) {
        for node in std::iter::once(root).chain(descendants.iter_descendants::<Children>(root)) {
            if !nodes.contains(node) {
                continue;
            }
            let owner = std::iter::once(node)
                .chain(parents.iter_ancestors::<ChildOf>(node))
                .find(|ancestor| canvases.contains(*ancestor));
            let old = membership.get(node).ok().map(|membership| membership.0);
            match owner {
                Some(owner) if old != Some(owner) => {
                    commands.entity(node).insert(UiCanvasMembership(owner));
                }
                None if old.is_some() => {
                    commands.entity(node).remove::<UiCanvasMembership>();
                }
                _ => {}
            }
        }
    }
}

pub(super) fn apply_authored_canvas_layout_corrections(
    canvases: Query<Ref<UiCanvasMetrics>>,
    mut physical_aspect_nodes: Query<
        (
            Ref<UiPhysicalAspect>,
            Ref<UiCanvasMembership>,
            &mut UiTransform,
        ),
        Without<UiLogicalCanvas>,
    >,
    mut physical_list_rows: Query<(
        Ref<UiPhysicalListRowHeight>,
        Ref<UiCanvasMembership>,
        &mut Node,
    )>,
    mut physical_tree_rows: Query<
        (
            Ref<UiAuthoredTreeRowPhysicalIndent>,
            Ref<UiCanvasMembership>,
            &mut UiTransform,
        ),
        (Without<UiLogicalCanvas>, Without<UiPhysicalAspect>),
    >,
) {
    let metrics_for = |membership: &Ref<UiCanvasMembership>, changed| {
        canvases.get(membership.0).ok().filter(|metrics| {
            metrics.has_extent() && (changed || membership.is_changed() || metrics.is_changed())
        })
    };
    for (aspect, membership, transform) in &mut physical_aspect_nodes {
        if let Some(metrics) = metrics_for(&membership, aspect.is_changed()) {
            transform
                .map_unchanged(|value| &mut value.scale)
                .set_if_neq(metrics.isotropic_correction);
        }
    }
    for (height, membership, node) in &mut physical_list_rows {
        if let Some(metrics) = metrics_for(&membership, height.is_changed()) {
            node.map_unchanged(|node| &mut node.height)
                .set_if_neq(px(height.0 / metrics.base_scale.y));
        }
    }
    for (indent, membership, transform) in &mut physical_tree_rows {
        if let Some(metrics) = metrics_for(&membership, indent.is_changed()) {
            let x = indent.physical_pixels() / metrics.base_scale.x;
            transform
                .map_unchanged(|value| &mut value.translation)
                .set_if_neq(Val2::px(x, 0.0));
        }
    }
}

pub(super) fn project_ui_canvas_pixel_transform(
    mut canvases: Query<
        (&UiGlobalTransform, &mut UiCanvasPixelTransform),
        Changed<UiGlobalTransform>,
    >,
) {
    for (transform, mut pixel) in &mut canvases {
        let affine = transform.affine();
        let projected =
            (affine.matrix2.determinant().abs() > f32::EPSILON).then(|| (affine, affine.inverse()));
        pixel.set_if_neq(UiCanvasPixelTransform(projected));
    }
}

pub(super) fn snap_authored_ui_canvas_descendant_edges_to_physical_pixels(
    canvases: Query<Ref<UiCanvasPixelTransform>>,
    mut nodes: Query<
        (
            Ref<UiCanvasMembership>,
            Ref<ComputedNode>,
            &mut UiGlobalTransform,
        ),
        Without<UiLogicalCanvas>,
    >,
) {
    for (membership, node, mut transform) in &mut nodes {
        let Ok(canvas) = canvases.get(membership.0) else {
            continue;
        };
        if !canvas.is_changed()
            && !membership.is_changed()
            && !node.is_changed()
            && !transform.is_changed()
        {
            continue;
        }
        let Some((canvas_affine, authored_from_physical)) = canvas.0 else {
            continue;
        };
        let size = node.size();
        let affine = transform.affine();
        if size.min_element() <= 0.0
            || affine.matrix2.x_axis.y.abs() > f32::EPSILON
            || affine.matrix2.y_axis.x.abs() > f32::EPSILON
        {
            continue;
        }
        let a = affine.transform_point2(-size * 0.5);
        let b = affine.transform_point2(size * 0.5);
        let a = canvas_affine.transform_point2(authored_from_physical.transform_point2(a).round());
        let b = canvas_affine.transform_point2(authored_from_physical.transform_point2(b).round());
        let minimum = a.min(b).round();
        let maximum = a.max(b).round();
        let snapped = maximum - minimum;
        if snapped.min_element() <= 0.0 {
            continue;
        }
        let scale = Vec2::new(
            snapped.x.copysign(affine.matrix2.x_axis.x) / size.x,
            snapped.y.copysign(affine.matrix2.y_axis.y) / size.y,
        );
        transform.set_if_neq(
            Affine2::from_scale_angle_translation(scale, 0.0, (minimum + maximum) * 0.5).into(),
        );
    }
}
