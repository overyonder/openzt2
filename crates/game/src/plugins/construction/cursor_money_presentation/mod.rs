use bevy::{prelude::*, ui::UiGlobalTransform, window::PrimaryWindow};
use openzt2_game_data::{
    localization::LocalizationFormatArgument,
    ui_document::node_property_binding::UiTextPropertyBindingSource, AssetId,
};

use crate::assets::localization::localization_asset_types::LocalizationAsset;
use crate::assets::localization::localization_precedence_index::LocalizationPrecedenceIndex;
use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::economy::authored_economy_fact_hydration::find_authored_object_or_placeable_price;
use crate::plugins::economy::money_types::Money;
use crate::plugins::ui::animation::UiAnimationOrigin;
use crate::plugins::ui::animation::UiShowHideAnimation;
use crate::plugins::ui::authored_ui_node_projection_components::UiDocumentRoot;
use crate::plugins::ui::authored_ui_text_content_binding::UiTextBinding;

use super::{
    construction_interaction_types::{ConstructionPreview, PlacementValidity},
    cursor_money_types::{
        CursorMoneyContainer, CursorMoneyPreviewFragmentOwner, CursorMoneySpendFragmentOwner,
    },
};

#[allow(clippy::too_many_arguments)]
pub(super) fn project_cursor_money_text_position_and_animation(
    windows: Query<&Window, With<PrimaryWindow>>,
    containers: Query<(&ComputedNode, &UiGlobalTransform), With<CursorMoneyContainer>>,
    preview_fragment_owners: Query<(), With<CursorMoneyPreviewFragmentOwner>>,
    mut spend_fragment_owners: Query<&mut CursorMoneySpendFragmentOwner>,
    previews: Query<&ConstructionPreview>,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    active_localization: Res<LocalizationPrecedenceIndex>,
    localizations: Res<Assets<LocalizationAsset>>,
    mut roots: Query<
        (
            Entity,
            &ChildOf,
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
    let Some(localization) = active_localization.borrow_loaded_localization_view(&localizations)
    else {
        return;
    };
    let world_definitions = active_world_definitions.get(&world_definition_assets);
    let construction_preview_cost =
        previews
            .single()
            .ok()
            .and_then(|preview| match preview.validity {
                PlacementValidity::Valid { cost } => Some(cost),
                PlacementValidity::Pending | PlacementValidity::Invalid(_) => world_definitions
                    .and_then(|world_definitions| {
                        find_authored_object_or_placeable_price(
                            world_definitions,
                            preview.definition,
                        )
                    }),
            });
    let current_pointer_screen_position = primary_window.cursor_position();

    for (
        root,
        parent,
        binding,
        mut text,
        mut font,
        mut node,
        mut visibility,
        mut animation,
        origin,
    ) in &mut roots
    {
        let UiTextPropertyBindingSource::ConstructionPreviewCost {
            positive_format,
            negative_format,
            no_cents,
            no_minus,
        } = &binding.0
        else {
            continue;
        };
        let (cost, screen_position, spending, animate, start_animation) = if preview_fragment_owners
            .contains(parent.parent())
        {
            let (Some(cost), Some(position)) =
                (construction_preview_cost, current_pointer_screen_position)
            else {
                *visibility = Visibility::Hidden;
                continue;
            };
            (cost, position, true, false, false)
        } else if let Ok(mut spend_fragment_owner) = spend_fragment_owners.get_mut(parent.parent())
        {
            let start_animation = !spend_fragment_owner.animation_started;
            spend_fragment_owner.animation_started = true;
            (
                spend_fragment_owner.cost,
                spend_fragment_owner.screen_position,
                true,
                true,
                start_animation,
            )
        } else {
            continue;
        };
        let authored_position = cursor_money_container_transform
            .inverse()
            .transform_point2(screen_position)
            + cursor_money_container_node.size() * 0.5;
        write_localized_cursor_money_text_and_font(
            localization,
            cost,
            *positive_format,
            *negative_format,
            *no_cents,
            *no_minus,
            spending,
            &mut text,
            &mut font,
        );
        *visibility = Visibility::Inherited;
        if let Some(mut origin) = origin {
            origin.0 = authored_position;
        } else {
            commands
                .entity(root)
                .insert(UiAnimationOrigin(authored_position));
        }
        commands.entity(parent.parent()).insert((
            Node {
                position_type: PositionType::Absolute,
                left: px(0.0),
                top: px(0.0),
                width: percent(100.0),
                height: percent(100.0),
                ..default()
            },
            Pickable::IGNORE,
        ));
        if !animate || start_animation {
            node.position_type = PositionType::Absolute;
            node.left = px(authored_position.x + animation.base_rect[0] + animation.end_rect[0]);
            node.top = px(authored_position.y + animation.base_rect[1] + animation.end_rect[1]);
            node.width = px(animation.base_rect[2]);
            node.height = px(animation.base_rect[3]);
        }
        if animate && start_animation {
            animation.elapsed_ms = animation.duration_ms;
            animation.forward = false;
            animation.running = animation.duration_ms > 0.0;
        } else if !animate {
            animation.elapsed_ms = animation.duration_ms;
            animation.forward = true;
            animation.running = false;
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn write_localized_cursor_money_text_and_font(
    localization: crate::assets::localization::loaded_localization_queries::LoadedLocalizationView<
        '_,
    >,
    cost: Money,
    positive_format: AssetId,
    negative_format: AssetId,
    no_cents: bool,
    no_minus: bool,
    spending: bool,
    text: &mut Text,
    font: &mut TextFont,
) {
    text.0.clear();
    let absolute_amount = Money(cost.0.saturating_abs());
    let mut raw_localized_currency = String::new();
    let _ = localization.write_localized_currency_amount(
        absolute_amount.0,
        !no_cents,
        &mut raw_localized_currency,
    );
    if spending && !no_minus {
        text.0 = format_accounting_currency(&raw_localized_currency, true, no_minus);
    } else {
        text.0.push_str(&raw_localized_currency);
    }
    let localized_format_identifier = if spending {
        negative_format
    } else {
        positive_format
    };
    let unformatted_currency_text = std::mem::take(&mut text.0);
    if localization
        .write_localized_text_with_format_arguments(
            localized_format_identifier,
            &[LocalizationFormatArgument::Text(&unformatted_currency_text)],
            &mut text.0,
        )
        .is_err()
    {
        text.0 = unformatted_currency_text;
    }
    if let Some(presentation) =
        localization.find_localized_text_presentation(localized_format_identifier)
    {
        if let Some(font_size) = presentation.font_size_pixels {
            font.font_size = FontSize::Px(font_size);
        }
        if presentation.bold_text {
            font.weight = FontWeight::BOLD;
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
