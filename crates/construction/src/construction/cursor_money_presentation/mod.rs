use bevy::prelude::*;
use bevy::ui::UiGlobalTransform;
use bevy::window::PrimaryWindow;
use openzt2_game_data::localization::LocalizationFormatArgument;
use openzt2_game_data::ui_document::node_property_binding::UiTextPropertyBindingSource;
use openzt2_game_data::AssetId;

use openzt2_game_assets::localization::localization_asset_types::LocalizationAsset;
use openzt2_game_assets::localization::localization_precedence_index::LocalizationPrecedenceIndex;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_zoo_state::economy::authored_price_queries::find_authored_object_or_placeable_price;
use openzt2_zoo_state::placement::placement_transaction_types::PlacedObjectRelocationSource;
use openzt2_zoo_state::economy::money_types::Money;
use openzt2_ui::animation::UiAnimationOrigin;
use openzt2_ui::animation::UiShowHideAnimation;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentRoot;
use openzt2_ui::authored_ui_text_content_binding::UiTextBinding;

use crate::construction::cursor_money_types::CursorMoneyContainer;
use crate::construction::cursor_money_types::CursorMoneyPreviewFragmentOwner;
use crate::construction::cursor_money_types::CursorMoneySpendFragmentOwner;
use crate::construction::cursor_money_types::PendingCursorMoneySpendAnimation;
use openzt2_zoo_state::animal_lifecycle::animal_adoption_contracts::{
    AnimalPlacementOperation, AnimalSpeciesAwaitingHabitatPlacement,
};
use openzt2_zoo_state::construction::construction_interaction_types::ConstructionPreview;
use openzt2_zoo_state::construction::construction_interaction_types::PlacementValidity;

#[allow(clippy::too_many_arguments)]
pub(super) fn project_cursor_money_text_position_and_animation(
    windows: Query<&Window, With<PrimaryWindow>>,
    containers: Query<(&ComputedNode, &UiGlobalTransform), With<CursorMoneyContainer>>,
    fragment_owners: Query<
        (Entity, &Children, Option<&CursorMoneySpendFragmentOwner>),
        Or<(
            With<CursorMoneyPreviewFragmentOwner>,
            With<PendingCursorMoneySpendAnimation>,
        )>,
    >,
    mut fragment_layouts: Query<&mut Node, Without<UiDocumentRoot>>,
    previews: Query<(&ConstructionPreview, Has<PlacedObjectRelocationSource>)>,
    animal_placements: Query<&AnimalSpeciesAwaitingHabitatPlacement>,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    active_localization: Res<LocalizationPrecedenceIndex>,
    localizations: Res<Assets<LocalizationAsset>>,
    mut roots: Query<
        (
            Entity,
            &UiTextBinding,
            &mut Text,
            &mut TextFont,
            &mut Node,
            &mut Visibility,
            &mut UiShowHideAnimation,
            Option<&mut UiAnimationOrigin>,
        ),
        With<UiDocumentRoot>,
    >,
    mut commands: Commands,
) {
    let Ok(primary_window) = windows.single() else {
        return;
    };
    let Ok((cursor_money_container_node, cursor_money_container_transform)) = containers.single()
    else {
        return;
    };
    if cursor_money_container_node.size() == Vec2::ZERO {
        return;
    }
    let Some(localization) = active_localization.borrow_loaded_localization_view(&localizations)
    else {
        return;
    };
    let world_definitions = active_world_definitions.get(&world_definition_assets);
    let construction_preview_cost = if animal_placements
        .iter()
        .any(|placement| matches!(placement.operation, AnimalPlacementOperation::Relocation(_)))
    {
        Some(Money(0))
    } else {
        previews
            .single()
            .ok()
            .and_then(|(preview, relocating)| match preview.validity {
                // Moving a placed object is free whether or not the spot is valid yet.
                _ if relocating => Some(Money(0)),
                PlacementValidity::Valid { cost } => Some(cost),
                PlacementValidity::Pending | PlacementValidity::Invalid(_) => world_definitions
                    .and_then(|world_definitions| {
                        find_authored_object_or_placeable_price(
                            world_definitions,
                            preview.definition,
                        )
                    }),
            })
    };
    let current_pointer_screen_position = primary_window.cursor_position();

    for (owner, children, spend) in &fragment_owners {
        let Some(root) = children.iter().find(|&child| roots.contains(child)) else {
            continue;
        };
        let Ok((root, binding, text, font, mut node, mut visibility, mut animation, origin)) =
            roots.get_mut(root)
        else {
            continue;
        };
        let UiTextPropertyBindingSource::ConstructionPreviewCost {
            positive_format,
            negative_format,
            no_cents,
            no_minus,
        } = &binding.0
        else {
            continue;
        };
        let (cost, screen_position) = if let Some(spend) = spend {
            if animation.duration_ms <= 0.0 {
                commands.entity(owner).try_despawn();
                continue;
            }
            (spend.cost, spend.screen_position)
        } else {
            let (Some(cost), Some(position)) =
                (construction_preview_cost, current_pointer_screen_position)
            else {
                visibility.set_if_neq(Visibility::Hidden);
                continue;
            };
            (cost, position)
        };
        let authored_position = cursor_money_container_transform
            .inverse()
            .transform_point2(screen_position)
            + cursor_money_container_node.size() * 0.5;
        if let Ok(mut layout) = fragment_layouts.get_mut(owner) {
            layout.set_if_neq(Node {
                position_type: PositionType::Absolute,
                left: px(0.0),
                top: px(0.0),
                width: percent(100.0),
                height: percent(100.0),
                ..default()
            });
        }
        write_localized_cursor_money_text_and_font(
            localization,
            cost,
            *positive_format,
            *negative_format,
            *no_cents,
            *no_minus,
            cost.0 > 0,
            text,
            font,
        );
        visibility.set_if_neq(Visibility::Inherited);
        if let Some(mut origin) = origin {
            origin.set_if_neq(UiAnimationOrigin(authored_position));
        } else {
            commands
                .entity(root)
                .insert(UiAnimationOrigin(authored_position));
        }
        node.reborrow()
                .map_unchanged(|n| &mut n.position_type)
                .set_if_neq(PositionType::Absolute);
            node.reborrow()
                .map_unchanged(|n| &mut n.left)
                .set_if_neq(px(authored_position.x
                    + animation.rect_intervals[0]
                        .map_or(animation.base_rect[0], |[_, end]| end)));
            node.reborrow()
                .map_unchanged(|n| &mut n.top)
                .set_if_neq(px(authored_position.y
                    + animation.rect_intervals[1]
                        .map_or(animation.base_rect[1], |[_, end]| end)));
            node.reborrow()
                .map_unchanged(|n| &mut n.width)
                .set_if_neq(px(animation.base_rect[2]));
            node.map_unchanged(|n| &mut n.height)
                .set_if_neq(px(animation.base_rect[3]));
        if spend.is_some() {
            animation.start_authored_visibility_transition(false);
            commands
                .entity(owner)
                .remove::<PendingCursorMoneySpendAnimation>();
        } else {
            animation.elapsed_ms = animation.duration_ms;
            animation.forward = true;
            animation.running = false;
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn write_localized_cursor_money_text_and_font(
    localization: openzt2_game_assets::localization::loaded_localization_queries::LoadedLocalizationView<
        '_,
    >,
    cost: Money,
    positive_format: AssetId,
    negative_format: AssetId,
    no_cents: bool,
    no_minus: bool,
    spending: bool,
    text: Mut<Text>,
    mut font: Mut<TextFont>,
) {
    let absolute_amount = Money(cost.0.saturating_abs());
    let mut raw_localized_currency = String::new();
    let _ = localization.write_localized_currency_amount(
        absolute_amount.0,
        !no_cents,
        &mut raw_localized_currency,
    );
    let unformatted_currency_text = if spending && !no_minus {
        format_accounting_currency(&raw_localized_currency, true, no_minus)
    } else {
        raw_localized_currency
    };
    let localized_format_identifier = if spending {
        negative_format
    } else {
        positive_format
    };
    let mut formatted = String::new();
    if localization
        .write_localized_text_with_format_arguments(
            localized_format_identifier,
            &[LocalizationFormatArgument::Text(&unformatted_currency_text)],
            &mut formatted,
        )
        .is_err()
    {
        formatted = unformatted_currency_text;
    }
    text.map_unchanged(|text| &mut text.0).set_if_neq(formatted);
    if let Some(presentation) =
        localization.find_localized_text_presentation(localized_format_identifier)
    {
        if let Some(font_size) = presentation.font_size_pixels {
            font.reborrow()
                .map_unchanged(|font| &mut font.font_size)
                .set_if_neq(FontSize::Px(font_size));
        }
        if presentation.bold_text {
            font.map_unchanged(|font| &mut font.weight)
                .set_if_neq(FontWeight::BOLD);
        }
    }
}

fn format_accounting_currency(
    raw_localized_currency: &str,
    spending: bool,
    no_minus: bool,
) -> String {
    if spending && !no_minus {
        format!("({raw_localized_currency})")
    } else {
        raw_localized_currency.to_owned()
    }
}
