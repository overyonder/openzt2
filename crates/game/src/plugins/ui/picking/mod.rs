//! Original alpha-aware picking over Bevy's canonical UI stack.

use bevy::{
    camera::RenderTarget,
    picking::{
        backend::HitData, backend::PointerHits, pointer::PointerId, pointer::PointerLocation,
        PickingSystems,
    },
    prelude::*,
    ui::{clip_check_recursive, ComputedUiTargetCamera, UiGlobalTransform, UiStack, UiSystems},
    window::PrimaryWindow,
};

use crate::assets::texture::interactive_texture_metadata_asset_and_borrowing_queries::InteractiveTextureMetadataAsset;

use super::authored_ui_interaction_enabled_state::UiInteractionEnabled;

/// Whether the mouse currently resolves to an authored UI surface.
///
/// Alpha-aware picking is the sole writer. World interaction systems read this
/// instead of independently interpreting Bevy's raw hover stack, so visual
/// hover, button activation, and world-click capture share one result.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct UiPointerCapture {
    pub(crate) over_ui: bool,
    pub(crate) target: Option<Entity>,
    /// The live leaf which accepted this primary press, retained while held.
    pressed_target: Option<Entity>,
}

/// The source texture region sampled by the original `normal` UI hit policy.
/// Layout remains Bevy-owned; this component only supplies the authored alpha
/// coverage needed after the rectangular bounds test succeeds.
#[derive(Component, Debug, Clone)]
pub(super) struct UiAlphaHitTest {
    pub(super) metadata: Handle<InteractiveTextureMetadataAsset>,
    pub(super) source_rect: Option<Rect>,
}

/// Replaces Bevy's rectangular-only UI backend. It retains Bevy's computed
/// layout, clipping, camera targeting, render order, and pointer pipeline, but
/// applies the source `normal` alpha test before an authored node can block
/// the node below it.
pub(super) fn alpha_ui_picking(
    pointers: Query<(&PointerId, &PointerLocation)>,
    cameras: Query<(Entity, &Camera, &RenderTarget)>,
    primary_window: Query<Entity, With<PrimaryWindow>>,
    ui_stack: Res<UiStack>,
    nodes: Query<(
        Entity,
        &ComputedNode,
        &UiGlobalTransform,
        Option<&Pickable>,
        Option<&UiInteractionEnabled>,
        Option<&InheritedVisibility>,
        &ComputedUiTargetCamera,
        Option<&UiAlphaHitTest>,
    )>,
    clipping: Query<(&ComputedNode, &UiGlobalTransform, &Node)>,
    parents: Query<&ChildOf, Without<OverrideClip>>,
    metadata: Res<Assets<InteractiveTextureMetadataAsset>>,
    pointer_input: Res<crate::plugins::input::input_types::PrimaryPointerInputState>,
    mut capture: ResMut<UiPointerCapture>,
    modal_input: Res<super::active_authored_ui_context::AuthoredModalInputCapture>,
    context: super::active_authored_ui_context::ActiveAuthoredUiContext,
    mut interactions: Query<(Entity, &UiInteractionEnabled, &mut Interaction)>,
    mut output: MessageWriter<PointerHits>,
) {
    let previous_pressed_target = capture.pressed_target;
    *capture = UiPointerCapture {
        over_ui: modal_input.0.is_some(),
        target: None,
        pressed_target: None,
    };
    let mut mouse_hit = None::<(f32, Entity)>;
    for (pointer, location) in pointers
        .iter()
        .filter_map(|(pointer, location)| Some((*pointer, location.location()?)))
    {
        for (camera_entity, camera, target) in &cameras {
            if !camera.is_active
                || target.normalize(primary_window.single().ok()).as_ref() != Some(&location.target)
            {
                continue;
            }
            let mut pointer_position =
                location.position * camera.target_scaling_factor().unwrap_or(1.0);
            if let Some(viewport) = camera.physical_viewport_rect() {
                if !viewport.as_rect().contains(pointer_position) {
                    continue;
                }
                pointer_position -= viewport.min.as_vec2();
            }

            let mut picks = Vec::new();
            let mut depth = 0.0;
            'stack: for partition in ui_stack.partition.iter().rev() {
                for entity in ui_stack.uinodes[partition.clone()].iter().rev().copied() {
                    let Ok((
                        entity,
                        node,
                        transform,
                        pickable,
                        interaction_enabled,
                        inherited_visibility,
                        target_camera,
                        alpha,
                    )) = nodes.get(entity)
                    else {
                        continue;
                    };
                    let interactive = interaction_enabled.is_some();
                    if modal_input.0.is_some_and(|modal| !context.is_in_modal(entity, modal))
                        || (pickable.is_none() && !interactive)
                        || target_camera.get() != Some(camera_entity)
                        || node.size() == Vec2::ZERO
                        || inherited_visibility.is_none_or(|visibility| !visibility.get())
                        || !node.contains_point(*transform, pointer_position)
                        || !clip_check_recursive(pointer_position, entity, &clipping, &parents)
                        // Every projected node carries an explicit policy.
                        // `Pickable::IGNORE` must pass through to the world;
                        // reporting it would make the canvas root capture the
                        // entire viewport even though Bevy later discards it.
                        || pickable.is_some_and(|pickable| {
                            !pickable.is_hoverable && !pickable.should_block_lower
                        })
                    {
                        continue;
                    }
                    let local = transform.inverse().transform_point2(pointer_position);
                    let normalized = local / node.size() + Vec2::splat(0.5);
                    if alpha.is_some_and(|alpha| {
                        !alpha_hit(&metadata, alpha, normalized.clamp(Vec2::ZERO, Vec2::ONE))
                    }) {
                        continue;
                    }
                    picks.push((
                        entity,
                        HitData::new(
                            camera_entity,
                            depth,
                            Some((local / node.size()).extend(0.0)),
                            None,
                        ),
                    ));
                    if interactive {
                        if pointer.is_mouse() {
                            let order = camera.order as f32 + 0.5;
                            if mouse_hit.is_none_or(|(current, _)| order > current) {
                                mouse_hit = Some((order, entity));
                            }
                        }
                        // An opaque interactive node is the one authored leaf
                        // target. Pointer propagation reaches its ancestors;
                        // lower siblings must not independently hover merely
                        // because the projected node was marked non-blocking.
                        break 'stack;
                    }
                    if pickable.is_none_or(|pickable| pickable.should_block_lower) {
                        break 'stack;
                    }
                    depth += 0.000_01;
                }
            }
            if !picks.is_empty() {
                if pointer.is_mouse() {
                    capture.over_ui = true;
                }
                output.write(PointerHits::new(pointer, picks, camera.order as f32 + 0.5));
            }
        }
    }

    capture.target = mouse_hit.map(|(_, entity)| entity);
    capture.pressed_target = if pointer_input.just_pressed {
        capture.target
    } else if pointer_input.pressed {
        previous_pressed_target
    } else {
        None
    }
    .filter(|entity| {
        context.visible_and_enabled(*entity)
            && modal_input
                .0
                .is_none_or(|modal| context.is_in_modal(*entity, modal))
    });

    // Bevy's legacy `Interaction` owner performs rectangle-only focus before
    // the picking backends run. Replace those authored-control states with the
    // same alpha-aware, topmost result emitted above so presentation, tooltips,
    // and activation cannot disagree with picking.
    for (entity, enabled, mut interaction) in &mut interactions {
        let hit = enabled.0 && mouse_hit.is_some_and(|(_, hovered)| hovered == entity);
        // Never retain Pressed from Bevy's earlier rectangle-only focus pass:
        // it may name another control beneath a transparent part of its skin.
        let next = if enabled.0 && capture.pressed_target == Some(entity) {
            Interaction::Pressed
        } else if hit {
            Interaction::Hovered
        } else {
            Interaction::None
        };
        interaction.set_if_neq(next);
    }
}

fn alpha_hit(
    metadata: &Assets<InteractiveTextureMetadataAsset>,
    hit: &UiAlphaHitTest,
    normalized: Vec2,
) -> bool {
    let Some(metadata) = metadata.get(&hit.metadata) else {
        return false;
    };
    let Some(dimensions) = metadata.alpha_hit_mask_dimensions().map(UVec2::as_vec2) else {
        return false;
    };
    let rect = hit.source_rect.unwrap_or(Rect {
        min: Vec2::ZERO,
        max: dimensions,
    });
    let point = rect.min + normalized * rect.size();
    metadata.alpha_hit_mask_contains_pixel(
        point.x.floor().clamp(0.0, dimensions.x - 1.0) as u32,
        point.y.floor().clamp(0.0, dimensions.y - 1.0) as u32,
    )
}

pub(super) fn initialize(app: &mut App) {
    app.init_resource::<UiPointerCapture>().add_systems(
        PreUpdate,
        alpha_ui_picking
            .in_set(PickingSystems::Backend)
            .after(UiSystems::Focus),
    );
}
