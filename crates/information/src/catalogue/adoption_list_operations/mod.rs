use bevy::{
    ecs::{relationship::RelationshipTarget, system::SystemParam},
    prelude::*,
};
use openzt2_game_data::ui_document::action::UiTrigger;
use openzt2_game_data::{species::Sex, AssetId};
use openzt2_world_state::game_session_types::WorldSessionMode;
use openzt2_world_state::world_spawn::selected_world_identity::SelectedWorldIdentity;

use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_animals::animal_lifecycle::adoption_offer_types::{
    AdoptionOfferCooldown, AdoptionOfferRandom, AdoptionOfferOption, AdoptionOfferOptions, AdoptionOfferSupply, AdoptionOfferUnlocked, CurrentAdoptionOffer,
};
use openzt2_zoo_state::animal_lifecycle::animal_adoption_contracts::AnimalCatalogueGender;
use openzt2_zoo_state::animal_lifecycle::animal_adoption_contracts::BeginAnimalAdoptionPlacement;
use openzt2_interface_state::ui::authored_reusable_list_and_table_runtime_types::SetUiListRowCount;
use openzt2_interface_state::ui::authored_reusable_list_and_table_runtime_types::UiListRow;
use openzt2_interface_state::ui::authored_reusable_list_and_table_runtime_types::UiTablePolicy;
use openzt2_ui::authored_tooltip_presentation::UiLongTooltipKey;
use openzt2_ui::authored_tooltip_presentation::UiShortTooltipKey;
use openzt2_interface_state::ui::authored_ui_interaction_enabled_state::UiInteractionEnabled;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentOwner;
use openzt2_interface_state::ui::authored_ui_selection_state::UiSelected;
use openzt2_interface_state::ui::authored_ui_visual_types::UiSourceRect;
use openzt2_interface_state::ui::authored_ui_visual_types::UiVisualLayer;
use openzt2_world_state::world_spawn::world_membership_types::WorldRoot;

use openzt2_interface_state::information::catalogue_types::AdoptionRowEntry;
use openzt2_interface_state::information::catalogue_types::SelectedCatalogueEntry;
use openzt2_interface_state::input::input_types::ActionSource;
use openzt2_interface_state::ui::authored_ui_activation_contracts::UiNodeActivated;

pub(crate) fn an_authored_adoption_catalogue_is_visible(
    adoption_lists: Query<(&UiTablePolicy, &InheritedVisibility)>,
) -> bool {
    adoption_lists
        .iter()
        .any(|(policy, visibility)| *policy == UiTablePolicy::AdoptionList && visibility.get())
}

/// Selects the authored purchase or limited-offer tab for the loaded game mode.
///
/// Only the selection and the losing tab's Off presentation apply here. The
/// winning tab's On actions repress its gender set, which activates the type
/// list and arms placement; the authored `animals` button runs them through
/// its `Animal ToggleSet` repress when the player opens the panel. Entering
/// or loading a world must not arm a placement tool by itself.
pub(crate) fn apply_authored_adoption_catalogue_tab_initialization(
    adoption_lists: Query<(Ref<UiTablePolicy>, &UiDocumentOwner)>,
    worlds: Query<Ref<SelectedWorldIdentity>, With<WorldRoot>>,
    mut authored_nodes: Query<(
        Entity,
        &Name,
        &UiDocumentOwner,
        &mut Visibility,
        Option<&mut UiSelected>,
    )>,
    mut activations: MessageWriter<UiNodeActivated>,
) {
    let Ok(world) = worlds.single() else {
        return;
    };
    // freeform_mode.xml selects buyanimal.xml; challenge/campaign use the
    // limited-offer manager in adoption.xml. Both tabs already belong to the
    // authored animal document and share the canonical placement consumer.
    let buy_animals = world.mode == WorldSessionMode::Freeform;
    for (table_policy, list_document_owner) in &adoption_lists {
        if *table_policy != UiTablePolicy::AdoptionList
            || (!table_policy.is_added() && !world.is_added() && !world.is_changed())
        {
            continue;
        }
        for (entity, name, node_document_owner, mut visibility, selected) in &mut authored_nodes {
            if node_document_owner.0 != list_document_owner.0 {
                continue;
            }
            let active = if name.as_str().eq_ignore_ascii_case("Buy Animal Tab") {
                buy_animals
            } else if name.as_str().eq_ignore_ascii_case("Adopt Animal Tab") {
                !buy_animals
            } else {
                continue;
            };
            *visibility = if active {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
            if let Some(mut selected) = selected {
                selected.0 = active;
            }
            // This owner decides which transition runs; keep the generic
            // selection-change dispatcher from replaying the winning tab's On.
            if !active {
                activations.write(UiNodeActivated {
                    source: ActionSource::System,
                    node: entity,
                    trigger: UiTrigger::Off,
                });
            }
        }
    }
}

#[derive(SystemParam)]
pub(crate) struct AdoptionCatalogueOffers<'w, 's> {
    roots: Query<
        'w,
        's,
        (
            Entity,
            Ref<'static, AdoptionOfferRandom>,
            Option<Ref<'static, AdoptionOfferOptions>>,
        ),
        With<WorldRoot>,
    >,
    options: Query<
        'w,
        's,
        (
            &'static AdoptionOfferOption,
            Option<Ref<'static, CurrentAdoptionOffer>>,
            Option<Ref<'static, AdoptionOfferCooldown>>,
        ),
    >,
    supplies: Query<
        'w,
        's,
        (
            Ref<'static, AdoptionOfferSupply>,
            Option<Ref<'static, AdoptionOfferUnlocked>>,
        ),
    >,
    removed_offers: RemovedComponents<'w, 's, CurrentAdoptionOffer>,
    removed_cooldowns: RemovedComponents<'w, 's, AdoptionOfferCooldown>,
}

fn offer_is_actionable(offer: &AdoptionOfferSupply, sex: Sex, unlocked: bool) -> bool {
    unlocked
        && match sex {
            Sex::Female => offer.available_by_sex[0] != 0,
            Sex::Male => offer.available_by_sex[1] != 0,
            Sex::Any => offer.available_by_sex != [0; 2],
        }
}

/// Selects the first live offer once the authored adoption surface opens.
pub(crate) fn select_first_available_species_after_adoption_catalogue_and_offer_inventory_are_both_available(
    adoption_lists: Query<(Ref<UiTablePolicy>, &UiDocumentOwner)>,
    selected_genders: Query<&AnimalCatalogueGender>,
    offers: AdoptionCatalogueOffers,
    mut selections: Query<
        &mut SelectedCatalogueEntry,
        With<openzt2_interface_state::ui::ui_document_lifecycle_contracts::InGameUiOwner>,
    >,
) {
    let Ok(mut selected_catalogue_entry) = selections.single_mut() else {
        return;
    };
    let Ok((_, random, Some(members))) = offers.roots.single() else {
        return;
    };
    for (table_policy, document_owner) in &adoption_lists {
        if *table_policy != UiTablePolicy::AdoptionList
            || (!table_policy.is_added()
                && !random.is_added()
                && selected_catalogue_entry.0.is_some())
        {
            continue;
        }
        let sex = selected_genders
            .get(document_owner.0)
            .map_or(Sex::Female, |gender| gender.0);
        let mut eligible = offers
            .options
            .iter_many(members.iter())
            .flatten()
            .filter_map(|(option, current, cooldown)| {
                if cooldown.is_some() {
                    return None;
                }
                let (supply, unlocked) = offers.supplies.get(*current?.collection()).ok()?;
                offer_is_actionable(&supply, sex, unlocked.is_some()).then_some((
                    option.slot,
                    option.option,
                    supply.species,
                ))
            });
        let mut first = None;
        for candidate in &mut eligible {
            if selected_catalogue_entry.0 == Some(candidate.2) {
                first = Some(candidate);
                break;
            }
            if first.is_none_or(|previous| candidate < previous) {
                first = Some(candidate);
            }
        }
        if let Some((_, _, species)) = first {
            selected_catalogue_entry.set_if_neq(SelectedCatalogueEntry(Some(species)));
        }
    }
}

/// Sizes each authored adoption surface for its live and future fame-gated slots.
pub(crate) fn request_authored_adoption_list_row_count(
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    adoption_lists: Query<(Entity, Ref<UiTablePolicy>, &UiDocumentOwner)>,
    worlds: Query<Ref<AdoptionOfferRandom>, With<WorldRoot>>,
    mut row_count_requests: MessageWriter<SetUiListRowCount>,
) {
    let Some(world_definitions) = active_world_definitions.get(&world_definition_assets) else {
        return;
    };
    let Some(configuration) = world_definitions.animal_adoption_offer_configuration() else {
        return;
    };
    let Ok(random) = worlds.single() else {
        return;
    };
    let displayed_slot_count = maximum_authored_animal_adoption_offer_slot_count(configuration);

    for (list_entity, table_policy, _) in &adoption_lists {
        if *table_policy != UiTablePolicy::AdoptionList
            || (!table_policy.is_added()
                && !random.is_added()
                && !active_world_definitions.is_changed()
                && !world_definition_assets.is_changed())
        {
            continue;
        }
        row_count_requests.write(SetUiListRowCount {
            list: list_entity,
            count: displayed_slot_count,
        });
    }
}

fn maximum_authored_animal_adoption_offer_slot_count(
    configuration: &openzt2_game_data::world_definitions::catalogue_and_progression::animal_adoption_offer_definition_types::AnimalAdoptionOfferConfiguration,
) -> u16 {
    configuration
        .base_slot_count
        .saturating_add(configuration.installed_expansion_slot_count)
        .saturating_add(
            configuration
                .fame_slots
                .iter()
                .map(|threshold| threshold.additional_slot_count)
                .max()
                .unwrap_or_default(),
        )
}

/// Projects adoption buttons directly from canonical adoption catalogue rows.
/// Species payload availability does not decide presentation availability.
pub(crate) fn project_adoption_catalogue_entries_to_authored_rows(
    mut commands: Commands,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    adoption_lists: Query<(&UiTablePolicy, &UiDocumentOwner)>,
    selected_genders: Query<Ref<AnimalCatalogueGender>>,
    mut offers: AdoptionCatalogueOffers,
    selected_catalogue_entry: Single<Ref<SelectedCatalogueEntry>>,
    child_entities: Query<&Children>,
    mut visual_images: Query<
        (Entity, Option<&mut ImageNode>, Option<&UiSourceRect>),
        (With<UiVisualLayer>, Without<UiListRow>),
    >,
    adoption_rows: Query<(Ref<UiListRow>, Ref<ChildOf>)>,
    mut adoption_offer_icons: Query<
        (
            Entity,
            &Name,
            &UiDocumentOwner,
            Option<&mut AdoptionRowEntry>,
            Option<&mut ImageNode>,
            Option<&mut UiInteractionEnabled>,
            Option<&mut UiSelected>,
            Option<&mut UiShortTooltipKey>,
            Option<&mut UiLongTooltipKey>,
            &mut Visibility,
        ),
        (Without<UiListRow>, Without<UiVisualLayer>),
    >,
) {
    let selected_catalogue_entry = selected_catalogue_entry.into_inner();
    let catalogue_content_changed =
        world_definition_assets.is_changed() || active_world_definitions.is_changed();
    let adoption_rows_changed = adoption_rows
        .iter()
        .any(|(row, parent)| row.is_changed() || parent.is_changed());
    let selected_gender_changed = selected_genders.iter().any(|gender| gender.is_changed());
    let Some(world_definitions) = active_world_definitions.get(&world_definition_assets) else {
        return;
    };
    let Some(configuration) = world_definitions.animal_adoption_offer_configuration() else {
        return;
    };
    let removed_offer = offers.removed_offers.read().count() != 0;
    let removed_cooldown = offers.removed_cooldowns.read().count() != 0;
    let Ok((_, random, members)) = offers.roots.single() else {
        return;
    };
    let mut offers_changed = removed_offer
        || removed_cooldown
        || random.is_added()
        || members.as_ref().is_some_and(|members| members.is_changed());
    let mut slots = Vec::<[Option<(Ref<AdoptionOfferSupply>, bool)>; 2]>::new();
    for (option, current, cooldown) in offers
        .options
        .iter_many(
            members
                .as_deref()
                .into_iter()
                .flat_map(RelationshipTarget::iter),
        )
        .flatten()
    {
        let slot = usize::from(option.slot);
        offers_changed |= cooldown.as_ref().is_some_and(Ref::is_added);
        if slots.len() <= slot {
            slots.resize_with(slot + 1, || [None, None]);
        }
        let Some(current) = current else {
            continue;
        };
        offers_changed |= current.is_changed();
        if let Ok((supply, unlocked)) = offers.supplies.get(*current.collection()) {
            offers_changed |= supply.is_changed()
                || unlocked
                    .as_ref()
                    .is_some_and(|unlocked| unlocked.is_added());
            if let Some(entry) = slots[slot].get_mut(usize::from(option.option)) {
                *entry = Some((supply, unlocked.is_some() && cooldown.is_none()));
            }
        }
    }
    if !catalogue_content_changed
        && !adoption_rows_changed
        && !selected_gender_changed
        && !offers_changed
        && !selected_catalogue_entry.is_changed()
    {
        return;
    }

    for (
        icon_entity,
        icon_name,
        row_document_owner,
        bound_entry,
        icon_image,
        interaction_enabled,
        selected,
        short_tooltip,
        long_tooltip,
        mut visibility,
    ) in &mut adoption_offer_icons
    {
        let Ok((row, parent)) = adoption_rows.get(row_document_owner.0) else {
            continue;
        };
        let Ok((table_policy, document_owner)) = adoption_lists.get(parent.parent()) else {
            continue;
        };
        if *table_policy != UiTablePolicy::AdoptionList {
            continue;
        }
        let locked = usize::from(row.index) >= slots.len();
        let is_locked_slot = icon_name
            .as_str()
            .eq_ignore_ascii_case("Locked Adoption Slot");
        let is_live_slot_presentation = ["progress", "number", "Decline Button"]
            .iter()
            .any(|name| icon_name.as_str().eq_ignore_ascii_case(name));
        if is_locked_slot || is_live_slot_presentation {
            let projected_visibility = if locked == is_locked_slot {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
            if *visibility != projected_visibility {
                *visibility = projected_visibility;
            }
            if locked && is_locked_slot {
                let short_tooltip_key = AssetId::from_key("adopt:adoptslot_stt");
                let long_tooltip_key =
                    locked_adoption_slot_long_tooltip_key(configuration, usize::from(row.index));
                let mut entity_commands = commands.entity(icon_entity);
                if let Some(mut key) = short_tooltip {
                    if key.0 != short_tooltip_key {
                        key.0 = short_tooltip_key;
                    }
                } else {
                    entity_commands.insert(UiShortTooltipKey(short_tooltip_key));
                }
                if let Some(mut key) = long_tooltip {
                    if key.0 != long_tooltip_key {
                        key.0 = long_tooltip_key;
                    }
                } else {
                    entity_commands.insert(UiLongTooltipKey(long_tooltip_key));
                }
            }
            continue;
        }
        let option_index = if icon_name
            .as_str()
            .eq_ignore_ascii_case("Primary Adoption Offer")
        {
            0
        } else if icon_name
            .as_str()
            .eq_ignore_ascii_case("Secondary Adoption Offer")
        {
            1
        } else {
            continue;
        };
        let selected_gender = selected_genders.get(document_owner.0).ok();
        if bound_entry.is_some()
            && !row.is_changed()
            && !catalogue_content_changed
            && !offers_changed
            && !selected_catalogue_entry.is_changed()
            && selected_gender
                .as_ref()
                .is_none_or(|gender| !gender.is_changed())
        {
            continue;
        }
        let sex = selected_gender.map_or(Sex::Female, |gender| gender.0);
        let offer_slot_index = row.index;
        let Some((offer, unlocked)) = slots
            .get(usize::from(offer_slot_index))
            .and_then(|slot| slot[option_index].as_ref())
        else {
            if *visibility != Visibility::Hidden {
                *visibility = Visibility::Hidden;
            }
            if bound_entry.is_some() {
                commands.entity(icon_entity).remove::<AdoptionRowEntry>();
            }
            continue;
        };
        if let Some(mut interaction_enabled) = interaction_enabled {
            let enabled = offer_is_actionable(offer, sex, *unlocked);
            if interaction_enabled.0 != enabled {
                interaction_enabled.0 = enabled;
            }
        }
        let is_selected = selected_catalogue_entry.0 == Some(offer.species);
        if let Some(mut selected) = selected {
            selected.0 = is_selected;
        }

        let species_identifier = offer.species;
        let Some(catalogue_entry) = world_definitions
            .catalogue_entry_for_definition(species_identifier)
            .map(|(_, entry)| entry)
        else {
            *visibility = Visibility::Hidden;
            continue;
        };
        if *visibility != Visibility::Inherited {
            *visibility = Visibility::Inherited;
        }
        let icon_identifier = AssetId(catalogue_entry.icon.0);
        if let Some(texture_image) = world_definitions.texture_image(icon_identifier) {
            if let Some(mut icon_image) = icon_image {
                if icon_image.image != texture_image {
                    icon_image.image = texture_image.clone();
                }
            }
            bind_texture_to_authored_adoption_offer_icon_visual_descendants(
                &mut commands,
                icon_entity,
                &texture_image,
                &child_entities,
                &mut visual_images,
            );
        }
        let entry = AdoptionRowEntry {
            species: species_identifier,
            offer_slot_index,
            offer_option_index: option_index as u8,
        };
        if let Some(mut current) = bound_entry {
            current.set_if_neq(entry);
        } else {
            commands.entity(icon_entity).insert(entry);
        }
    }
}

fn locked_adoption_slot_long_tooltip_key(
    configuration: &openzt2_game_data::world_definitions::catalogue_and_progression::animal_adoption_offer_definition_types::AnimalAdoptionOfferConfiguration,
    displayed_slot_index: usize,
) -> AssetId {
    let permanent_slot_count = usize::from(
        configuration
            .base_slot_count
            .saturating_add(configuration.installed_expansion_slot_count),
    );
    let future_fame_slot_number = displayed_slot_index
        .saturating_sub(permanent_slot_count)
        .saturating_add(1)
        .min(usize::from(u16::MAX)) as u16;
    configuration
        .fame_slots
        .iter()
        .filter(|threshold| threshold.additional_slot_count >= future_fame_slot_number)
        .min_by_key(|threshold| threshold.additional_slot_count)
        .map_or(AssetId::default(), |threshold| {
            threshold.locked_slot_long_tooltip_key
        })
}

fn bind_texture_to_authored_adoption_offer_icon_visual_descendants(
    commands: &mut Commands,
    icon_entity: Entity,
    texture_image: &Handle<Image>,
    child_entities: &Query<&Children>,
    visual_images: &mut Query<
        (Entity, Option<&mut ImageNode>, Option<&UiSourceRect>),
        (With<UiVisualLayer>, Without<UiListRow>),
    >,
) {
    let mut pending_descendants = child_entities.get(icon_entity).map_or_else(
        |_| Vec::new(),
        |children| children.iter().collect::<Vec<_>>(),
    );
    while let Some(descendant) = pending_descendants.pop() {
        if let Ok(children) = child_entities.get(descendant) {
            pending_descendants.extend(children.iter());
        }
        let Ok((visual_entity, image, source_rectangle)) = visual_images.get_mut(descendant) else {
            continue;
        };
        if let Some(mut image) = image {
            if image.image != texture_image.clone() {
                image.image = texture_image.clone();
            }
        } else {
            let mut image = ImageNode::new(texture_image.clone());
            image.rect = source_rectangle.map(|source_rectangle| Rect {
                min: Vec2::new(source_rectangle.0[0] as f32, source_rectangle.0[1] as f32),
                max: Vec2::new(
                    (source_rectangle.0[0] + source_rectangle.0[2]) as f32,
                    (source_rectangle.0[1] + source_rectangle.0[3]) as f32,
                ),
            });
            commands.entity(visual_entity).insert(image);
        }
    }
}

pub(crate) fn choose_species_from_activated_adoption_list_row(
    mut ui_node_activations: MessageReader<UiNodeActivated>,
    adoption_rows: Query<&AdoptionRowEntry>,
    list_rows: Query<&ChildOf, With<UiListRow>>,
    adoption_lists: Query<(&UiTablePolicy, &UiDocumentOwner)>,
    selected_genders: Query<&AnimalCatalogueGender>,
    parent_entities: Query<&ChildOf>,
    mut adoption_placement_requests: MessageWriter<BeginAnimalAdoptionPlacement>,
) {
    for activation in ui_node_activations.read() {
        if activation.trigger != UiTrigger::Press {
            continue;
        }
        let ancestors = || {
            std::iter::successors(Some(activation.node), |entity| {
                parent_entities.get(*entity).ok().map(ChildOf::parent)
            })
        };
        let Some(adoption_row) = ancestors().find_map(|entity| adoption_rows.get(entity).ok())
        else {
            continue;
        };
        let Some((_, document_owner)) = ancestors()
            .find_map(|entity| list_rows.get(entity).ok())
            .and_then(|parent| adoption_lists.get(parent.parent()).ok())
            .filter(|(policy, _)| **policy == UiTablePolicy::AdoptionList)
        else {
            continue;
        };
        adoption_placement_requests.write(BeginAnimalAdoptionPlacement {
            species: adoption_row.species,
            sex: Some(
                selected_genders
                    .get(document_owner.0)
                    .map_or(Sex::Female, |gender| gender.0),
            ),
            offer_slot_index: Some(adoption_row.offer_slot_index),
        });
    }
}
