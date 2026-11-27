use bevy::{ecs::relationship::RelationshipTarget, prelude::*};
use openzt2_game_assets::localization::{
    localization_asset_types::LocalizationAsset,
    localization_precedence_index::LocalizationPrecedenceIndex,
};
use openzt2_game_data::{localization::LocalizationFormatArgument, AssetId};

use super::first_person_waste_cleaning::FirstPersonStaffAim;

#[derive(Component)]
#[relationship(relationship_target = FirstPersonStaffPrompts)]
pub(super) struct FirstPersonStaffPromptOf(Entity);

#[derive(Component)]
#[relationship_target(relationship = FirstPersonStaffPromptOf, linked_spawn)]
pub(super) struct FirstPersonStaffPrompts(Vec<Entity>);

pub(super) fn present_first_person_staff_aim(
    sessions: Query<(
        Entity,
        Ref<FirstPersonStaffAim>,
        Option<&FirstPersonStaffPrompts>,
    )>,
    active: Res<LocalizationPrecedenceIndex>,
    assets: Res<Assets<LocalizationAsset>>,
    mut prompts: Query<(&mut Text, &mut Visibility), With<FirstPersonStaffPromptOf>>,
    mut commands: Commands,
) {
    let Some(localization) = active.borrow_loaded_localization_view(&assets) else {
        return;
    };
    for (session, aim, existing) in &sessions {
        if !aim.is_changed() && !active.is_changed() && !assets.is_changed() && existing.is_some() {
            continue;
        }
        let action = aim
            .target
            .zip(aim.localization)
            .and_then(|(_, key)| localization.find_plain_localized_text(key));
        let mut prompt = String::new();
        if let Some(action) = action {
            let _ = localization.write_localized_text_with_format_arguments(
                AssetId::from_key("superstaff:format"),
                &[LocalizationFormatArgument::Text(action)],
                &mut prompt,
            );
            if prompt.is_empty() {
                prompt.push_str(action);
            }
        }
        if let Some((mut text, mut visibility)) = existing
            .and_then(|prompts| prompts.iter().next())
            .and_then(|entity| prompts.get_mut(entity).ok())
        {
            visibility.set_if_neq(if prompt.is_empty() {
                Visibility::Hidden
            } else {
                Visibility::Inherited
            });
            text.set_if_neq(Text::new(prompt));
        } else if !prompt.is_empty() {
            commands.spawn((
                FirstPersonStaffPromptOf(session),
                Node {
                    position_type: PositionType::Absolute,
                    top: percent(8),
                    width: percent(100),
                    justify_content: JustifyContent::Center,
                    ..default()
                },
                GlobalZIndex(20),
                Pickable::IGNORE,
                Text::new(prompt),
                TextLayout::justify(Justify::Center),
                TextColor(Color::srgb(1.0, 0.92, 0.52)),
            ));
        }
    }
}
