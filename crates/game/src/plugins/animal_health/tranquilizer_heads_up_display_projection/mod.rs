use bevy::prelude::*;
use openzt2_game_data::ui_document::node_property_binding::UiTranquilizerHeadsUpDisplaySlotBinding;

use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::animal_lifecycle::types::Animal;
use crate::plugins::animal_lifecycle::types::SpeciesHandle;
use crate::plugins::ui::authored_ui_node_projection_components::UiValue;
use crate::plugins::ui::authored_ui_presentation_slot_bindings::UiTranquilizerHudBinding;

use super::types::{TranquilizerHud, TranquilizerReticle, TranquilizerTool};

#[derive(Component, Clone, Copy)]
pub(super) struct TranquilizerChargeBarLayout {
    top_px: f32,
    height_px: f32,
}

/// Updates the authored `tranqm.xml` slots from the player's tool and reticle.
pub(super) fn project_tranquilizer_heads_up_display_to_authored_ui_nodes(
    mut commands: Commands,
    huds: Query<(&TranquilizerTool, &TranquilizerHud)>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    targets: Query<(Option<&Name>, Option<&SpeciesHandle>), With<Animal>>,
    mut nodes: Query<(
        Entity,
        &UiTranquilizerHudBinding,
        Option<&mut Visibility>,
        Option<&mut Text>,
        Option<&mut Node>,
        Option<&mut ImageNode>,
        Option<&mut UiValue>,
        Option<&TranquilizerChargeBarLayout>,
    )>,
) {
    let Ok((tool, hud)) = huds.single() else {
        return;
    };
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    let Some(mode) = definitions.tranquilizer_mode() else {
        return;
    };
    if definitions.find_tranquilizer(tool.tranquilizer).is_none() {
        return;
    }
    let target = tool
        .target
        .filter(|_| hud.reticle != TranquilizerReticle::NoTarget);
    let fill = if tool.required_charge_points > 0.0 {
        (tool.charge_points / tool.required_charge_points).clamp(0.0, 1.0)
    } else {
        0.0
    };
    for (entity, binding, visibility, text, node, image, value, charge_bar) in &mut nodes {
        match binding.0 {
            UiTranquilizerHeadsUpDisplaySlotBinding::Screen => {
                if let Some(mut visibility) = visibility {
                    *visibility = Visibility::Inherited;
                }
            }
            UiTranquilizerHeadsUpDisplaySlotBinding::TargetName => {
                if let Some(mut text) = text {
                    text.0.clear();
                    if let Some(target) = target {
                        if let Ok((Some(name), _)) = targets.get(target) {
                            text.0.push_str(name.as_str());
                        }
                    }
                }
            }
            UiTranquilizerHeadsUpDisplaySlotBinding::TargetImage => {
                let handle = target
                    .and_then(|target| targets.get(target).ok())
                    .and_then(|(_, species)| species)
                    .and_then(|species| {
                        definitions.catalogue().find(|entry| {
                            entry.definition.0 == species.species.0
                                || entry.id.0 == species.species.0
                        })
                    })
                    .and_then(|entry| definitions.texture_image(entry.icon));
                match (handle, image) {
                    (Some(handle), Some(mut image)) => {
                        image.image = handle;
                    }
                    (Some(handle), None) => {
                        commands.entity(entity).insert(ImageNode::new(handle));
                    }
                    (None, Some(_)) => {
                        commands.entity(entity).remove::<ImageNode>();
                    }
                    (None, None) => {}
                }
            }
            UiTranquilizerHeadsUpDisplaySlotBinding::DistanceLabel
            | UiTranquilizerHeadsUpDisplaySlotBinding::DistanceText => {
                if let Some(mut visibility) = visibility {
                    *visibility = if target.is_some() && hud.in_range {
                        Visibility::Inherited
                    } else {
                        Visibility::Hidden
                    };
                }
                if binding.0 == UiTranquilizerHeadsUpDisplaySlotBinding::DistanceText {
                    if let Some(mut text) = text {
                        text.0.clear();
                        use std::fmt::Write;
                        let _ = write!(text.0, "{:.1} m", hud.distance_m);
                    }
                }
            }
            UiTranquilizerHeadsUpDisplaySlotBinding::OutOfRangeLabel => {
                if let Some(mut visibility) = visibility {
                    *visibility = if target.is_some() && !hud.in_range {
                        Visibility::Inherited
                    } else {
                        Visibility::Hidden
                    };
                }
            }
            UiTranquilizerHeadsUpDisplaySlotBinding::MisfireIndicator => {
                if let Some(mut visibility) = visibility {
                    *visibility = if hud.misfire_visible {
                        Visibility::Inherited
                    } else {
                        Visibility::Hidden
                    };
                }
            }
            UiTranquilizerHeadsUpDisplaySlotBinding::Reticle => {
                let texture = match hud.reticle {
                    TranquilizerReticle::Ready => mode.reticle_ready,
                    _ => mode.reticle_charging,
                };
                let handle = definitions.texture_image(texture);
                if let Some(handle) = handle {
                    if let Some(mut image) = image {
                        image.image = handle;
                    } else {
                        commands.entity(entity).insert(ImageNode::new(handle));
                    }
                }
            }
            UiTranquilizerHeadsUpDisplaySlotBinding::ChargeBar => {
                if let Some(mut value) = value {
                    value.0 = (fill * 1000.0).round() as i64;
                } else {
                    commands
                        .entity(entity)
                        .insert(UiValue((fill * 1000.0).round() as i64));
                }
                if let Some(mut node) = node {
                    let layout = charge_bar
                        .copied()
                        .or_else(|| match (node.top, node.height) {
                            (Val::Px(top_px), Val::Px(height_px)) => {
                                let layout = TranquilizerChargeBarLayout { top_px, height_px };
                                commands.entity(entity).insert(layout);
                                Some(layout)
                            }
                            _ => None,
                        });
                    if let Some(layout) = layout {
                        node.height = Val::Px(layout.height_px * fill);
                        node.top = Val::Px(layout.top_px + layout.height_px * (1.0 - fill));
                    }
                }
                if let Some(mut image) = image {
                    image.rect = Some(Rect {
                        min: Vec2::new(0.0, 64.0 * (1.0 - fill)),
                        max: Vec2::new(16.0, 64.0),
                    });
                }
            }
        }
    }
}
