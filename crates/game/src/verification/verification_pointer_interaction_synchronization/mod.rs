use bevy::{
    picking::{
        hover::HoverMap,
        pointer::{PointerButton, PointerId, PointerInput as BevyPointerInput},
    },
    prelude::*,
};

pub(super) fn synchronize_bevy_ui_interaction_from_verification_pointer_input(
    mut pointer_inputs: MessageReader<BevyPointerInput>,
    hover: Res<HoverMap>,
    mut primary_pressed: Local<bool>,
    mut interactions: Query<(Entity, &mut Interaction)>,
) {
    let mut just_pressed = false;
    let mut just_released = false;
    for input in pointer_inputs
        .read()
        .filter(|input| input.pointer_id == PointerId::Mouse)
    {
        if input.button_just_pressed(PointerButton::Primary) {
            *primary_pressed = true;
            just_pressed = true;
        } else if input.button_just_released(PointerButton::Primary) {
            *primary_pressed = false;
            just_released = true;
        }
    }
    if !hover.is_changed() && !just_pressed && !just_released {
        return;
    }
    let top = hover.get(&PointerId::Mouse).and_then(|hits| {
        hits.iter()
            .filter(|(entity, _)| interactions.contains(**entity))
            .min_by(|(left_entity, left), (right_entity, right)| {
                left.depth
                    .total_cmp(&right.depth)
                    .then_with(|| left_entity.to_bits().cmp(&right_entity.to_bits()))
            })
            .map(|(entity, _)| *entity)
    });
    for (entity, mut interaction) in &mut interactions {
        let next = if Some(entity) == top {
            if just_pressed || (*interaction == Interaction::Pressed && *primary_pressed) {
                Interaction::Pressed
            } else {
                Interaction::Hovered
            }
        } else {
            Interaction::None
        };
        interaction.set_if_neq(next);
    }
}
