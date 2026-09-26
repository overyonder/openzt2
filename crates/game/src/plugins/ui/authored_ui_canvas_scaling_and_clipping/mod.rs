use bevy::{math::Affine2, prelude::*, window::PrimaryWindow};

use crate::plugins::settings::display_settings_types::DisplaySettings;

use super::authored_tree_expansion_and_row_indentation::UiAuthoredTreeRowPhysicalIndent;

#[derive(Component, Clone, Copy)]
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

pub(super) fn scale_authored_ui_canvas_to_primary_window(
    windows: Query<&Window, With<PrimaryWindow>>,
    settings: Res<DisplaySettings>,
    mut canvases: Query<(&UiLogicalCanvas, &mut UiTransform)>,
    mut physical_aspect_nodes: Query<
        &mut UiTransform,
        (With<UiPhysicalAspect>, Without<UiLogicalCanvas>),
    >,
    mut physical_list_rows: Query<(&UiPhysicalListRowHeight, &mut Node)>,
    mut physical_tree_rows: Query<
        (&UiAuthoredTreeRowPhysicalIndent, &mut UiTransform),
        (Without<UiLogicalCanvas>, Without<UiPhysicalAspect>),
    >,
    mut scale: ResMut<UiScale>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    let user_scale = f32::from(settings.ui_scale_permille) / 1000.0;
    if (scale.0 - 1.0).abs() > f32::EPSILON {
        scale.0 = 1.0;
    }
    let viewport = Vec2::new(window.width(), window.height());
    for (logical, mut transform) in &mut canvases {
        let logical_size = logical.logical_size();
        if logical_size.min_element() <= 0.0 {
            continue;
        }
        let canvas_scale = viewport / logical_size * user_scale;
        let next = UiTransform {
            // Bevy applies UiTransform scale about the node's centre. The
            // authored root is laid out at its unscaled logical size, so move
            // that centre from logical_size / 2 to viewport / 2. Using the
            // post-scale extent here leaves the canvas anchored around its
            // old centre and crops the left/top of every widescreen capture.
            translation: Val2::px(
                (viewport.x - logical_size.x) * 0.5,
                (viewport.y - logical_size.y) * 0.5,
            ),
            scale: canvas_scale,
            ..UiTransform::IDENTITY
        };
        if *transform != next {
            *transform = next;
        }
        let aspect_correction = Vec2::new(canvas_scale.y / canvas_scale.x, 1.0);
        for mut transform in &mut physical_aspect_nodes {
            if transform.scale != aspect_correction {
                transform.scale = aspect_correction;
            }
        }
        // Generated list cells use the same authored 1024x768 coordinate
        // space as their templates. Bevy's root transform performs the one
        // canvas-to-backbuffer mapping for both static and generated nodes.
        let base_canvas_scale = viewport / logical_size;
        for (row_height, mut node) in &mut physical_list_rows {
            let height = px(row_height.0 / base_canvas_scale.y);
            if node.height != height {
                node.height = height;
            }
        }
        for (indent, mut transform) in &mut physical_tree_rows {
            let translation = Val2::px(indent.physical_pixels() / base_canvas_scale.x, 0.0);
            if transform.translation != translation {
                transform.translation = translation;
            }
        }
    }
}

/// Correct Bevy's inherited clip rectangles for transformed authored canvases.
///
/// Bevy 0.19 resolves an overflowing node's local rectangle correctly, but its
/// clipping system then adds only the global translation. The scale carried by
/// `UiGlobalTransform` is omitted, so a canvas stretched from 1024x768 to the
/// backbuffer renders at the requested size while its descendants are clipped
/// against their old logical widths. Keep Bevy's native `CalculatedClip`
/// contract, but project each clipping ancestor's rectangle through the same
/// transform used by rendering and picking.
pub(super) fn project_authored_scaled_canvas_clip_rectangles(
    mut commands: Commands,
    canvases: Query<Entity, With<UiLogicalCanvas>>,
    children: Query<&Children>,
    nodes: Query<(
        &Node,
        &ComputedNode,
        &UiGlobalTransform,
        Option<&CalculatedClip>,
        Has<OverrideClip>,
    )>,
) {
    for canvas in &canvases {
        project_authored_scaled_canvas_clip_rectangle(
            &mut commands,
            &children,
            &nodes,
            canvas,
            None,
        );
    }
}

/// Quantize final authored UI rectangle edges after the logical canvas transform.
///
/// Bevy rounds layout before `UiTransform` is applied. The independently scaled
/// 1024x768 canvas can therefore put two otherwise identical shared edges between
/// physical pixels. Rebuilding each final axis-aligned transform from its rounded
/// edges keeps adjacent authored rectangles coincident for rendering and picking.
pub(super) fn snap_authored_ui_canvas_descendant_edges_to_physical_pixels(
    children: Query<&Children>,
    mut transforms: ParamSet<(
        Query<(Entity, &UiGlobalTransform), With<UiLogicalCanvas>>,
        Query<(&ComputedNode, &mut UiGlobalTransform)>,
    )>,
    mut canvas_transforms: Local<Vec<(Entity, Affine2)>>,
    mut descendants: Local<Vec<Entity>>,
) {
    canvas_transforms.clear();
    canvas_transforms.extend(
        transforms
            .p0()
            .iter()
            .map(|(entity, transform)| (entity, transform.affine())),
    );
    for &(canvas, canvas_affine) in canvas_transforms.iter() {
        let authored_from_physical = canvas_affine.inverse();
        descendants.clear();
        descendants.push(canvas);
        let mut cursor = 0;
        while cursor < descendants.len() {
            if let Ok(children) = children.get(descendants[cursor]) {
                descendants.extend(children.iter());
            }
            cursor += 1;
        }

        let mut nodes = transforms.p1();
        for &entity in descendants.iter() {
            let Ok((computed, mut transform)) = nodes.get_mut(entity) else {
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
}

fn project_authored_scaled_canvas_clip_rectangle(
    commands: &mut Commands,
    children: &Query<&Children>,
    nodes: &Query<(
        &Node,
        &ComputedNode,
        &UiGlobalTransform,
        Option<&CalculatedClip>,
        Has<OverrideClip>,
    )>,
    entity: Entity,
    inherited_clip: Option<Rect>,
) {
    let Ok((node, computed, transform, calculated, overrides_clip)) = nodes.get(entity) else {
        return;
    };
    let inherited_clip = if overrides_clip { None } else { inherited_clip };
    let inherited_clip = (node.display != Display::None)
        .then_some(inherited_clip)
        .flatten()
        .or_else(|| (node.display == Display::None).then_some(Rect::default()));

    match (inherited_clip, calculated) {
        (Some(clip), Some(current)) if current.clip != clip => {
            commands.entity(entity).insert(CalculatedClip { clip });
        }
        (Some(clip), None) => {
            commands.entity(entity).insert(CalculatedClip { clip });
        }
        (None, Some(_)) => {
            commands.entity(entity).remove::<CalculatedClip>();
        }
        _ => {}
    }

    let child_clip = if node.overflow.is_visible() {
        inherited_clip
    } else {
        let local = computed.resolve_clip_rect(node.overflow, node.overflow_clip_margin);
        let affine = transform.affine();
        let corners = [
            affine.transform_point2(local.min),
            affine.transform_point2(Vec2::new(local.max.x, local.min.y)),
            affine.transform_point2(local.max),
            affine.transform_point2(Vec2::new(local.min.x, local.max.y)),
        ];
        let transformed = Rect {
            min: corners
                .iter()
                .copied()
                .reduce(Vec2::min)
                .unwrap_or_default(),
            max: corners
                .iter()
                .copied()
                .reduce(Vec2::max)
                .unwrap_or_default(),
        };
        Some(inherited_clip.map_or(transformed, |clip| clip.intersect(transformed)))
    };

    if let Ok(descendants) = children.get(entity) {
        for child in descendants.iter() {
            project_authored_scaled_canvas_clip_rectangle(
                commands, children, nodes, child, child_clip,
            );
        }
    }
}
