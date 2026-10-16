use bevy::{ecs::resource::IsResource, prelude::*};
use openzt2_interface_state::ui::input_frame_capture_types::AuthoredModalInputCapture;

pub(super) fn trace_changed_modal_render_state(
    capture: Res<AuthoredModalInputCapture>,
    entities: Query<EntityRef, Without<IsResource>>,
    mut previous: Local<(Option<Entity>, u8)>,
) {
    let changed = previous.0 != capture.0;
    if changed {
        *previous = (capture.0, 0);
    } else if previous.1 < 10 {
        previous.1 += 1;
    } else {
        return;
    }
    if !changed && previous.1 != 10 {
        return;
    }
    let Some(modal) = capture.0 else {
        return;
    };
    info!(target: "openzt2_modal_render", ?modal, settled_frames = previous.1,
        "active modal render snapshot");

    let mut ancestor = Some(modal);
    for depth in 0..32 {
        let Some(entity) = ancestor else {
            break;
        };
        let Ok(entity) = entities.get(entity) else {
            break;
        };
        trace_node(&entity, "ancestor", depth);
        ancestor = entity.get::<ChildOf>().map(ChildOf::parent);
    }
    let mut pending = vec![(modal, 0_u8)];
    let mut visited = 0;
    while let Some((entity, depth)) = pending.pop() {
        let Ok(entity) = entities.get(entity) else {
            continue;
        };
        if depth > 0 {
            trace_node(&entity, "descendant", depth);
        }
        visited += 1;
        if visited >= 128 {
            info!(target: "openzt2_modal_render", "modal descendant snapshot capped at 128 nodes");
            break;
        }
        if depth < 6 {
            if let Some(children) = entity.get::<Children>() {
                pending.extend(children.iter().rev().map(|child| (child, depth + 1)));
            }
        }
    }
}

fn trace_node(entity: &EntityRef, relation: &str, depth: u8) {
    info!(target: "openzt2_modal_render",
        entity = ?entity.id(), relation, depth,
        name = ?entity.get::<Name>().map(Name::as_str),
        parent = ?entity.get::<ChildOf>().map(ChildOf::parent),
        visibility = ?entity.get::<Visibility>(),
        inherited_visibility = ?entity.get::<InheritedVisibility>().map(|visibility| visibility.get()),
        node = ?entity.get::<Node>(),
        computed_size = ?entity.get::<ComputedNode>().map(ComputedNode::size),
        clip = ?entity.get::<CalculatedClip>(),
        transform = ?entity.get::<UiGlobalTransform>(),
        global_z = ?entity.get::<GlobalZIndex>(),
        local_z = ?entity.get::<ZIndex>(),
        stack = ?entity.get::<bevy::ui::ComputedStackIndex>().map(|stack| stack.0),
        camera = ?entity.get::<bevy::ui::ComputedUiTargetCamera>(),
        image = ?entity.get::<ImageNode>().map(|image| (&image.image, image.color)),
        background = ?entity.get::<BackgroundColor>(),
        text_color = ?entity.get::<TextColor>(),
        "modal rendering node");
}
