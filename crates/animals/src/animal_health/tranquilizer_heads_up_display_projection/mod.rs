use bevy::prelude::*;
use openzt2_game_data::ui_document::node_property_binding::UiTranquilizerHeadsUpDisplaySlotBinding;

use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_zoo_state::animal_lifecycle::types::Animal;
use openzt2_zoo_state::animal_lifecycle::types::SpeciesHandle;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiValue;
use openzt2_ui::authored_ui_presentation_slot_bindings::UiTranquilizerHudBinding;

use super::types::{TranquilizerHud, TranquilizerReticle, TranquilizerTool};

#[derive(Component, Clone, Copy)]
pub(super) struct TranquilizerChargeBarLayout {
    top_px: f32,
    height_px: f32,
}

/// Updates the authored `tranqm.xml` slots from the player's tool and reticle.
pub(super) fn project_tranquilizer_heads_up_display_to_authored_ui_nodes(
    mut commands: Commands,
    huds: Query<(Ref<TranquilizerTool>, Ref<TranquilizerHud>)>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    targets: Query<(Option<Ref<Name>>, Option<Ref<SpeciesHandle>>), With<Animal>>,
    mut previous_target: Local<(Option<Entity>, bool, bool)>,
    changed_bindings: Query<(), Changed<UiTranquilizerHudBinding>>,
    mut nodes: Query<(
        Entity,
        Ref<UiTranquilizerHudBinding>,
        Option<&mut Visibility>,
        Option<&mut Text>,
        Option<&mut Node>,
        Option<&mut ImageNode>,
        Option<&mut UiValue>,
        Option<&TranquilizerChargeBarLayout>,
    )>,
) {
    let Ok((tool, hud)) = huds.single() else {
        *previous_target = Default::default();
        return;
    };
    let assets_changed = active_definitions.is_changed() || definitions.is_changed();
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
    let (target_name, target_species) = target
        .and_then(|target| targets.get(target).ok())
        .unwrap_or_default();
    let (previous_entity, had_name, had_species) = *previous_target;
    let target_changed = previous_entity != target;
    let name_changed = target_changed
        || had_name != target_name.is_some()
        || target_name.as_ref().is_some_and(DetectChanges::is_changed);
    let species_changed = target_changed
        || had_species != target_species.is_some()
        || target_species
            .as_ref()
            .is_some_and(DetectChanges::is_changed);
    *previous_target = (target, target_name.is_some(), target_species.is_some());
    if !assets_changed
        && !tool.is_changed()
        && !hud.is_changed()
        && !name_changed
        && !species_changed
        && changed_bindings.is_empty()
    {
        return;
    }
    let fill = if tool.required_charge_points > 0.0 {
        (tool.charge_points / tool.required_charge_points).clamp(0.0, 1.0)
    } else {
        0.0
    };
    for (entity, binding, visibility, text, node, image, value, charge_bar) in &mut nodes {
        let binding_changed = binding.is_changed();
        if !binding_changed
            && !assets_changed
            && !tool.is_changed()
            && !hud.is_changed()
            && !name_changed
            && !species_changed
        {
            continue;
        }
        match binding.0 {
            UiTranquilizerHeadsUpDisplaySlotBinding::Screen => {
                if let Some(mut visibility) = visibility {
                    visibility.set_if_neq(Visibility::Inherited);
                }
            }
            UiTranquilizerHeadsUpDisplaySlotBinding::TargetName => {
                if let Some(mut text) = text.filter(|_| binding_changed || name_changed) {
                    let name = target_name.as_ref().map_or("", |name| name.as_str());
                    if text.0 != name {
                        name.clone_into(&mut text.0);
                    }
                }
            }
            UiTranquilizerHeadsUpDisplaySlotBinding::TargetImage => {
                if !binding_changed && !species_changed && !assets_changed {
                    continue;
                }
                let handle = target_species
                    .as_ref()
                    .and_then(|species| {
                        definitions.catalogue().find(|entry| {
                            entry.definition.0 == species.species.0
                                || entry.id.0 == species.species.0
                        })
                    })
                    .and_then(|entry| definitions.texture_image(entry.icon));
                match (handle, image) {
                    (Some(handle), Some(image)) => {
                        image
                            .map_unchanged(|image| &mut image.image)
                            .set_if_neq(handle);
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
                    visibility.set_if_neq(if target.is_some() && hud.in_range {
                        Visibility::Inherited
                    } else {
                        Visibility::Hidden
                    });
                }
                if binding.0 == UiTranquilizerHeadsUpDisplaySlotBinding::DistanceText {
                    if let Some(text) = text {
                        text.map_unchanged(|text| &mut text.0)
                            .set_if_neq(format!("{:.1} m", hud.distance_m));
                    }
                }
            }
            UiTranquilizerHeadsUpDisplaySlotBinding::OutOfRangeLabel => {
                if let Some(mut visibility) = visibility {
                    visibility.set_if_neq(if target.is_some() && !hud.in_range {
                        Visibility::Inherited
                    } else {
                        Visibility::Hidden
                    });
                }
            }
            UiTranquilizerHeadsUpDisplaySlotBinding::MisfireIndicator => {
                if let Some(mut visibility) = visibility {
                    visibility.set_if_neq(if hud.misfire_visible {
                        Visibility::Inherited
                    } else {
                        Visibility::Hidden
                    });
                }
            }
            UiTranquilizerHeadsUpDisplaySlotBinding::Reticle => {
                let texture = match hud.reticle {
                    TranquilizerReticle::Ready => mode.reticle_ready,
                    _ => mode.reticle_charging,
                };
                let handle = definitions.texture_image(texture);
                if let Some(handle) = handle {
                    if let Some(image) = image {
                        image
                            .map_unchanged(|image| &mut image.image)
                            .set_if_neq(handle);
                    } else {
                        commands.entity(entity).insert(ImageNode::new(handle));
                    }
                }
            }
            UiTranquilizerHeadsUpDisplaySlotBinding::ChargeBar => {
                if let Some(value) = value {
                    value
                        .map_unchanged(|value| &mut value.0)
                        .set_if_neq((fill * 1000.0).round() as i64);
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
                        let height = Val::Px(layout.height_px * fill);
                        let top = Val::Px(layout.top_px + layout.height_px * (1.0 - fill));
                        if (node.height, node.top) != (height, top) {
                            node.height = height;
                            node.top = top;
                        }
                    }
                }
                if let Some(image) = image {
                    image
                        .map_unchanged(|image| &mut image.rect)
                        .set_if_neq(Some(Rect {
                            min: Vec2::new(0.0, 64.0 * (1.0 - fill)),
                            max: Vec2::new(16.0, 64.0),
                        }));
                }
            }
        }
    }
}
