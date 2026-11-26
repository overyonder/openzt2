use bevy::prelude::*;
use openzt2_game_data::ui_document::node_property_binding::UiTranquilizerHeadsUpDisplaySlotBinding;

use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_zoo_state::animal_lifecycle::types::Animal;
use openzt2_zoo_state::animal_lifecycle::types::SpeciesHandle;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiValue;
use openzt2_interface_state::ui::authored_ui_visual_types::UiVisualLayer;
use openzt2_ui::authored_ui_presentation_slot_bindings::UiTranquilizerHudBinding;

use super::types::{TranquilizerHud, TranquilizerReticle, TranquilizerTool};

#[derive(Component, Clone, Copy, PartialEq)]
pub(super) struct TranquilizerChargeBarLayout {
    top_px: f32,
    height_px: f32,
}

#[derive(Resource)]
pub(super) struct TranquilizerChargeBarImages {
    charging: Handle<Image>,
    ready: Handle<Image>,
}

pub(super) fn load_tranquilizer_charge_bar_images(
    mut commands: Commands,
    assets: Res<AssetServer>,
) {
    commands.insert_resource(TranquilizerChargeBarImages {
        charging: assets.load("ui/minigames/tranqgun_readoutbar.z2dds"),
        ready: assets.load("ui/minigames/tranqgun_readoutbar_full.z2dds"),
    });
}

/// Updates the authored `tranqm.xml` slots from the player's tool and reticle.
pub(super) fn project_tranquilizer_heads_up_display_to_authored_ui_nodes(
    mut commands: Commands,
    huds: Query<(Ref<TranquilizerTool>, Ref<TranquilizerHud>)>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    targets: Query<(Ref<Animal>, Option<Ref<Name>>, Option<Ref<SpeciesHandle>>)>,
    mut removed_animals: RemovedComponents<Animal>,
    mut removed_names: RemovedComponents<Name>,
    mut removed_species: RemovedComponents<SpeciesHandle>,
    bar_images: Res<TranquilizerChargeBarImages>,
    changed_bindings: Query<
        (),
        (
            With<UiTranquilizerHudBinding>,
            Or<(Changed<UiTranquilizerHudBinding>, Changed<Children>)>,
        ),
    >,
    visual_layers: Query<(), With<UiVisualLayer>>,
    mut images: Query<&mut ImageNode>,
    mut nodes: Query<(
        Entity,
        Ref<UiTranquilizerHudBinding>,
        Option<&mut Visibility>,
        Option<&mut Text>,
        Option<&mut Node>,
        Option<&mut UiValue>,
        Option<&mut TranquilizerChargeBarLayout>,
        Option<Ref<Children>>,
    )>,
) {
    let Ok((tool, hud)) = huds.single() else {
        removed_animals.clear();
        removed_names.clear();
        removed_species.clear();
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
    let selected_target = tool
        .target
        .filter(|_| hud.reticle != TranquilizerReticle::NoTarget);
    let target_removed = target_component_removed(&mut removed_animals, selected_target);
    let name_removed = target_component_removed(&mut removed_names, selected_target);
    let species_removed = target_component_removed(&mut removed_species, selected_target);
    let target_facts = selected_target.and_then(|target| targets.get(target).ok());
    let target = selected_target.filter(|_| target_facts.is_some());
    let target_added = target_facts
        .as_ref()
        .is_some_and(|(animal, ..)| animal.is_added());
    let (target_name, target_species) =
        target_facts.map_or_else(Default::default, |(_, name, species)| (name, species));
    let target_changed = tool.is_changed() || hud.is_changed() || target_removed || target_added;
    let name_changed = target_changed
        || name_removed
        || target_name.as_ref().is_some_and(DetectChanges::is_changed);
    let species_changed = target_changed
        || species_removed
        || target_species
            .as_ref()
            .is_some_and(DetectChanges::is_changed);
    if !assets_changed
        && !tool.is_changed()
        && !hud.is_changed()
        && !name_changed
        && !species_changed
        && changed_bindings.is_empty()
    {
        return;
    }
    let charge_ready = tool.charge_points >= tool.required_charge_points;
    let fill = if charge_ready {
        1.0
    } else if tool.required_charge_points > 0.0 {
        (tool.charge_points / tool.required_charge_points).clamp(0.0, 1.0)
    } else {
        0.0
    };
    for (entity, binding, visibility, text, node, value, charge_bar, children) in &mut nodes {
        let children_changed = children.as_ref().is_some_and(DetectChanges::is_changed);
        let binding_changed = binding.is_changed() || children_changed;
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
                if let Some(mut text) =
                    text.filter(|_| binding_changed || name_changed || assets_changed)
                {
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
                match (handle, images.get_mut(entity).ok()) {
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
                    if let Ok(image) = images.get_mut(entity) {
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
                    let authored_layout = || match (node.top, node.height) {
                        (Val::Px(top_px), Val::Px(height_px)) => {
                            Some(TranquilizerChargeBarLayout { top_px, height_px })
                        }
                        _ => None,
                    };
                    let layout = if let Some(mut layout) = charge_bar {
                        if binding.is_changed() && node.is_changed() {
                            if let Some(authored) = authored_layout() {
                                layout.set_if_neq(authored);
                            }
                        }
                        Some(*layout)
                    } else {
                        authored_layout().inspect(|layout| {
                            commands.entity(entity).insert(*layout);
                        })
                    };
                    if let Some(layout) = layout {
                        let filled_height = (layout.height_px * fill).trunc();
                        let height = Val::Px(filled_height);
                        let top = Val::Px(layout.top_px + layout.height_px - filled_height);
                        if (node.height, node.top) != (height, top) {
                            node.height = height;
                            node.top = top;
                        }
                    }
                }
                let handle = if charge_ready {
                    &bar_images.ready
                } else {
                    &bar_images.charging
                };
                let rect = Some(Rect {
                    min: Vec2::new(0.0, 128.0 - (128.0 * fill).trunc()),
                    max: Vec2::new(32.0, 128.0),
                });
                let image_entities = std::iter::once(entity).chain(
                    children
                        .iter()
                        .flat_map(|children| children.iter())
                        .filter(|child| visual_layers.contains(*child)),
                );
                for image_entity in image_entities {
                    if let Ok(mut image) = images.get_mut(image_entity) {
                        image
                            .reborrow()
                            .map_unchanged(|image| &mut image.image)
                            .set_if_neq(handle.clone());
                        image
                            .map_unchanged(|image| &mut image.rect)
                            .set_if_neq(rect);
                    }
                }
            }
        }
    }
}

fn target_component_removed<T: Component>(
    removed: &mut RemovedComponents<T>,
    target: Option<Entity>,
) -> bool {
    let matched = target.is_some_and(|target| removed.read().any(|entity| entity == target));
    removed.clear();
    matched
}
