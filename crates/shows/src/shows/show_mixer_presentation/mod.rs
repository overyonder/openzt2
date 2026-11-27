use bevy::{
    ecs::{
        entity::{EntityHashMap, EntityHashSet},
        relationship::RelationshipTarget,
        system::SystemParam,
    },
    prelude::*,
};
use openzt2_game_assets::{
    localization::{
        loaded_localization_queries::LoadedLocalizationView,
        localization_asset_types::LocalizationAsset,
        localization_precedence_index::LocalizationPrecedenceIndex,
    },
    ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset,
    world_definitions::{
        world_definition_asset_set_state_and_borrowing_queries::{
            WorldDefinitions, WorldDefinitionsView,
        },
        world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
    },
};
use openzt2_game_data::{
    ui_document::{action::UiTrigger, document::UiDocumentRole},
    AssetId,
};
use openzt2_interface_state::ui::{
    authored_ui_activation_contracts::UiNodeActivated,
    authored_ui_node_projection_components::{UiDocumentOwner, UiDocumentRoot},
    ui_document_lifecycle_contracts::ShowUiDocument,
};
use openzt2_world_state::world_spawn::world_membership_types::{WorldMember, WorldMembers};
use openzt2_zoo_state::{
    animal_health::types::Dead,
    animal_lifecycle::types::{Animal, SpeciesHandle},
    shows::{
        performer_training::PerformerTraining,
        show_schedule_types::{
            ScheduledShowPerformancePlan, ScheduledShowRow, ScheduledShowTrickPerformance,
            ScheduledTrickPerformer, ShowTrickSlots,
        },
        show_stage_types::ShowStage,
    },
};

use super::{
    scheduled_trick_execution::performance_level,
    show_editor_interaction_types::{EditedShow, ShowEditing},
    show_timing_operations::convert_authored_nanoseconds_to_fixed_simulation_ticks,
    EditShowMixerSlot, ShowMixerSlotChoice, ShowMixerTimeSlot, ShowMixerWait,
};

const MIXER_FRAGMENTS: [&str; 4] = [
    "ui/layout/showmixer/mixerrow.xml",
    "ui/layout/showmixer/trickentry.xml",
    "ui/layout/showmixer/animaldroplistentry.xml",
    "ui/layout/showmixer/trickdroplistentry.xml",
];

#[derive(Resource)]
pub(super) struct ShowMixerFragments([Handle<UiDocumentAsset>; 4]);

pub(super) fn load_show_mixer_fragments(mut commands: Commands, server: Res<AssetServer>) {
    commands.insert_resource(ShowMixerFragments(
        MIXER_FRAGMENTS.map(|path| server.load(path)),
    ));
}

#[derive(Component)]
#[relationship(relationship_target = ShowMixerPanels)]
pub(super) struct ShowMixerPanelOf(Entity);

#[derive(Component)]
#[relationship_target(relationship = ShowMixerPanelOf, linked_spawn)]
pub(super) struct ShowMixerPanels(Entity);

#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub(super) enum ShowMixerMenu {
    Animals {
        replacing: Option<Entity>,
    },
    Tricks {
        animal: Entity,
        slot: Option<Entity>,
        time_slot: Option<u16>,
    },
}

#[derive(Component)]
enum ShowMixerControl {
    Open(ShowMixerMenu),
    SelectAnimal(Entity),
    Edit {
        slot: Option<Entity>,
        time_slot: Option<u16>,
        choice: ShowMixerSlotChoice,
    },
}

#[derive(Component)]
pub(super) struct ShowMixerFieldValues {
    label: Option<String>,
    icon: Option<Handle<Image>>,
    kind: Option<ShowMixerControl>,
}

#[derive(Component)]
pub(super) struct ShowMixerTrickEntry;

#[derive(SystemParam)]
pub(super) struct ShowMixerSourceQueries<'w, 's> {
    rows: Query<
        'w,
        's,
        (
            Ref<'static, ScheduledShowRow>,
            Ref<'static, ScheduledShowPerformancePlan>,
            &'static WorldMember,
            Option<Ref<'static, ShowTrickSlots>>,
        ),
    >,
    stages: Query<'w, 's, &'static ShowStage>,
    slots: Query<
        'w,
        's,
        (
            Option<Ref<'static, ScheduledShowTrickPerformance>>,
            Option<Ref<'static, ScheduledTrickPerformer>>,
            Option<Ref<'static, ShowMixerWait>>,
            Option<Ref<'static, ShowMixerTimeSlot>>,
        ),
    >,
    animals: Query<
        'w,
        's,
        (
            &'static SpeciesHandle,
            Option<Ref<'static, PerformerTraining>>,
            Option<Ref<'static, Name>>,
        ),
        (With<Animal>, Without<Dead>),
    >,
    world_members: Query<'w, 's, Ref<'static, WorldMembers>>,
}

struct MixerFragments<'a> {
    documents: &'a Assets<UiDocumentAsset>,
    handles: &'a ShowMixerFragments,
    locale: LoadedLocalizationView<'a>,
}

impl MixerFragments<'_> {
    fn project(
        &self,
        commands: &mut Commands,
        owner: Entity,
        index: usize,
        fields: ShowMixerFieldValues,
    ) -> Option<Entity> {
        let handle = &self.handles.0[index];
        let document = self.documents.get(handle)?;
        let root = openzt2_ui::projection::authored_document_projection::project_document(
            commands,
            &ShowUiDocument {
                owner,
                document: handle.clone(),
            },
            document,
            self.locale,
            None,
            false,
        );
        commands.entity(root).insert(fields);
        if index == 1 {
            commands.entity(root).insert(ShowMixerTrickEntry);
        }
        Some(root)
    }

    fn label(&self, id: AssetId) -> Option<String> {
        self.locale.find_plain_localized_text(id).map(str::to_owned)
    }
}

pub(super) fn project_live_show_mixer(
    handles: Res<ShowMixerFragments>,
    assets: Res<Assets<UiDocumentAsset>>,
    locales: Res<LocalizationPrecedenceIndex>,
    locale_assets: Res<Assets<LocalizationAsset>>,
    definitions: Res<WorldDefinitions>,
    definition_assets: Res<Assets<WorldDefinitionAsset>>,
    targets: Query<(
        Entity,
        Ref<Name>,
        Ref<UiDocumentOwner>,
        Option<&ShowMixerPanels>,
    )>,
    documents: Query<(&UiDocumentRoot, Option<&ChildOf>)>,
    controllers: Query<(
        Ref<EditedShow>,
        Has<ShowEditing>,
        Option<Ref<ShowMixerMenu>>,
    )>,
    editing: Query<(), Added<ShowEditing>>,
    mut retired: (
        RemovedComponents<ShowEditing>,
        RemovedComponents<EditedShow>,
        RemovedComponents<ShowMixerMenu>,
        RemovedComponents<ShowTrickSlots>,
    ),
    inputs: ShowMixerSourceQueries,
    mut commands: Commands,
) {
    let (Some(definitions_view), Some(locale)) = (
        definitions.get(&definition_assets),
        locales.borrow_loaded_localization_view(&locale_assets),
    ) else {
        return;
    };
    if handles.0.iter().any(|handle| !assets.contains(handle.id())) {
        return;
    }
    let fragments = MixerFragments {
        documents: &assets,
        handles: &handles,
        locale,
    };
    let authored_changed = handles.is_changed()
        || assets.is_changed()
        || definitions.is_changed()
        || definition_assets.is_changed()
        || locales.is_changed()
        || locale_assets.is_changed();
    let controller_changes: EntityHashSet = retired
        .0
        .read()
        .chain(retired.1.read())
        .chain(retired.2.read())
        .collect();
    let emptied_plans: EntityHashSet = retired.3.read().collect();
    for (target, name, owner, rendered) in &targets {
        if name.as_str() != "master listbox" {
            continue;
        }
        let Ok((document, parent)) = documents.get(owner.0) else {
            continue;
        };
        if !assets.get(&document.document).is_some_and(|document| {
            document.canonical_ui_document().role == UiDocumentRole::ShowEditor
        }) {
            continue;
        }
        let controller = parent.map_or(owner.0, ChildOf::parent);
        let current = controllers
            .get(controller)
            .ok()
            .and_then(|(edited, can_edit, menu)| {
                let (row, plan, world, slots) = inputs.rows.get(edited.0).ok()?;
                let stage = inputs.stages.get(row.stage).ok()?;
                let policy = definitions_view.find_show_stage(stage.show_stage_definition)?;
                Some((edited, can_edit, menu, row, plan, world, slots, policy))
            });
        let Some((edited, can_edit, menu, row, plan, world, slots, policy)) = current else {
            if let Some(panel) = rendered.and_then(|panels| panels.iter().next()) {
                commands.entity(panel).try_despawn();
            }
            continue;
        };
        let members = inputs.world_members.get(world.root).ok();
        let menu = menu.as_ref();
        let changed = authored_changed
            || edited.is_changed()
            || row.is_changed()
            || plan.is_changed()
            || name.is_changed()
            || owner.is_changed()
            || controller_changes.contains(&controller)
            || editing.contains(controller)
            || emptied_plans.contains(&edited.0)
            || slots.as_ref().is_some_and(Ref::is_changed)
            || menu.is_some_and(Ref::is_changed)
            || members.as_ref().is_some_and(Ref::is_changed)
            || inputs
                .slots
                .iter_many(
                    slots
                        .as_deref()
                        .into_iter()
                        .flat_map(RelationshipTarget::iter),
                )
                .flatten()
                .any(|(trick, animal, wait, time)| {
                    trick.as_ref().is_some_and(Ref::is_changed)
                        || animal.as_ref().is_some_and(Ref::is_changed)
                        || wait.as_ref().is_some_and(Ref::is_changed)
                        || time.as_ref().is_some_and(Ref::is_changed)
                        || animal
                            .and_then(|animal| inputs.animals.get(animal.0).ok())
                            .is_some_and(|(_, training, name)| {
                                training.as_ref().is_some_and(Ref::is_changed)
                                    || name.as_ref().is_some_and(Ref::is_changed)
                            })
                });
        if let Some(panel) = rendered.and_then(|panels| panels.iter().next()) {
            if !changed {
                continue;
            }
            commands.entity(panel).try_despawn();
        }
        let panel = commands
            .spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    ..default()
                },
                ShowMixerPanelOf(target),
                ChildOf(target),
            ))
            .id();
        let mut animal_rows = EntityHashMap::default();
        let mut next_time = EntityHashMap::<u16>::default();
        let mut entries: Vec<_> = slots
            .as_deref()
            .into_iter()
            .flat_map(RelationshipTarget::iter)
            .enumerate()
            .filter_map(|(index, slot)| {
                let (trick, animal, wait, time) = inputs.slots.get(slot).ok()?;
                Some((
                    time.map_or(index as u16, |time| time.0),
                    slot,
                    trick,
                    animal,
                    wait,
                ))
            })
            .collect();
        entries.sort_by_key(|(time, ..)| *time);
        for (time, slot, trick, animal, wait) in entries {
            let mut label = None;
            let mut icon = None;
            let mut control = None;
            let mut owner = panel;
            if let Some(animal) = animal {
                let previous = next_time
                    .insert(animal.0, time.saturating_add(1))
                    .unwrap_or_default();
                owner = *animal_rows.entry(animal.0).or_insert_with(|| {
                    let name = inputs
                        .animals
                        .get(animal.0)
                        .ok()
                        .and_then(|(_, _, name)| name.map(|name| name.as_str().to_owned()));
                    fragments
                        .project(
                            &mut commands,
                            panel,
                            0,
                            ShowMixerFieldValues {
                                label: name,
                                icon: None,
                                kind: can_edit.then_some(ShowMixerControl::Open(
                                    ShowMixerMenu::Animals {
                                        replacing: Some(animal.0),
                                    },
                                )),
                            },
                        )
                        .unwrap_or(panel)
                });
                for missing in previous..time {
                    fragments.project(
                        &mut commands,
                        owner,
                        1,
                        ShowMixerFieldValues {
                            label: None,
                            icon: show_mixer_icon(definitions_view, "wait"),
                            kind: can_edit.then_some(ShowMixerControl::Open(
                                ShowMixerMenu::Tricks {
                                    animal: animal.0,
                                    slot: None,
                                    time_slot: Some(missing),
                                },
                            )),
                        },
                    );
                }
                if let Some(trick) =
                    trick.and_then(|trick| definitions_view.find_trick(trick.trick))
                {
                    label = fragments.label(trick.name_key);
                    icon = definitions_view.texture_image(trick.icon);
                }
                if wait.is_some() {
                    icon = show_mixer_icon(definitions_view, "wait");
                }
                if can_edit {
                    control = Some(ShowMixerControl::Open(ShowMixerMenu::Tricks {
                        animal: animal.0,
                        slot: Some(slot),
                        time_slot: None,
                    }));
                }
            } else if wait.is_some() {
                icon = show_mixer_icon(definitions_view, "wait");
                if can_edit {
                    control = Some(ShowMixerControl::Edit {
                        slot: Some(slot),
                        time_slot: None,
                        choice: ShowMixerSlotChoice::Remove,
                    });
                }
            }
            fragments.project(
                &mut commands,
                owner,
                1,
                ShowMixerFieldValues {
                    label,
                    icon,
                    kind: control,
                },
            );
        }
        if !can_edit {
            continue;
        }
        for (&animal, &owner) in &animal_rows {
            if next_time
                .get(&animal)
                .is_some_and(|&time| time < plan.trick_performance_capacity)
            {
                fragments.project(
                    &mut commands,
                    owner,
                    1,
                    ShowMixerFieldValues {
                        label: None,
                        icon: show_mixer_icon(definitions_view, "add"),
                        kind: Some(ShowMixerControl::Open(ShowMixerMenu::Tricks {
                            animal,
                            slot: None,
                            time_slot: None,
                        })),
                    },
                );
            }
        }
        let maximum_animals = definitions_view
            .show_editor_presentation()
            .map_or(policy.performer_slots, |editor| {
                editor.maximum_animals.min(policy.performer_slots)
            });
        if animal_rows.len() < usize::from(maximum_animals) {
            fragments.project(
                &mut commands,
                panel,
                0,
                ShowMixerFieldValues {
                    label: None,
                    icon: show_mixer_icon(definitions_view, "addanimal"),
                    kind: Some(ShowMixerControl::Open(ShowMixerMenu::Animals {
                        replacing: None,
                    })),
                },
            );
        }
        match menu.map(|menu| **menu) {
            Some(ShowMixerMenu::Animals { replacing }) => {
                if let Some(animal) = replacing {
                    fragments.project(
                        &mut commands,
                        panel,
                        2,
                        ShowMixerFieldValues {
                            label: None,
                            icon: show_mixer_icon(definitions_view, "removeanimal"),
                            kind: Some(ShowMixerControl::Edit {
                                slot: None,
                                time_slot: None,
                                choice: ShowMixerSlotChoice::RemovePerformer { animal },
                            }),
                        },
                    );
                }
                for animal in members
                    .as_deref()
                    .into_iter()
                    .flat_map(RelationshipTarget::iter)
                {
                    if animal_rows.contains_key(&animal) {
                        continue;
                    }
                    let Ok((species, training, name)) = inputs.animals.get(animal) else {
                        continue;
                    };
                    if !definitions_view.tricks().any(|trick| {
                        trick.species.contains(&species.species)
                            && (policy.supported_tricks.is_empty()
                                || policy.supported_tricks.contains(&trick.id))
                            && performance_level(trick, training.as_deref()).is_some()
                    }) {
                        continue;
                    }
                    if let Some(previous) = replacing {
                        let compatible = inputs
                            .slots
                            .iter_many(
                                slots
                                    .as_deref()
                                    .into_iter()
                                    .flat_map(RelationshipTarget::iter),
                            )
                            .flatten()
                            .filter(|(_, performer, ..)| {
                                performer
                                    .as_ref()
                                    .is_some_and(|performer| performer.0 == previous)
                            })
                            .all(|(performance, ..)| {
                                performance.is_none_or(|performance| {
                                    definitions_view.find_trick(performance.trick).is_some_and(
                                        |trick| {
                                            trick.species.contains(&species.species)
                                                && performance_level(trick, training.as_deref())
                                                    .is_some()
                                        },
                                    )
                                })
                            });
                        if !compatible {
                            continue;
                        }
                    }
                    fragments.project(
                        &mut commands,
                        panel,
                        2,
                        ShowMixerFieldValues {
                            label: name.map(|name| name.as_str().to_owned()),
                            icon: None,
                            kind: Some(if let Some(previous) = replacing {
                                ShowMixerControl::Edit {
                                    slot: None,
                                    time_slot: None,
                                    choice: ShowMixerSlotChoice::ReplacePerformer {
                                        previous,
                                        animal,
                                    },
                                }
                            } else {
                                ShowMixerControl::SelectAnimal(animal)
                            }),
                        },
                    );
                }
            }
            Some(ShowMixerMenu::Tricks {
                animal,
                slot,
                time_slot,
            }) => {
                if let Ok((species, training, _)) = inputs.animals.get(animal) {
                    let mut tricks = definitions_view
                        .tricks()
                        .filter(|trick| {
                            trick.species.contains(&species.species)
                                && (policy.supported_tricks.is_empty()
                                    || policy.supported_tricks.contains(&trick.id))
                                && performance_level(trick, training.as_deref()).is_some()
                        })
                        .collect::<Vec<_>>();
                    tricks.sort_by_key(|trick| (trick.display_order, trick.id));
                    for trick in tricks {
                        fragments.project(
                            &mut commands,
                            panel,
                            3,
                            ShowMixerFieldValues {
                                label: fragments.label(trick.name_key),
                                icon: definitions_view.texture_image(trick.icon),
                                kind: Some(ShowMixerControl::Edit {
                                    slot,
                                    time_slot,
                                    choice: ShowMixerSlotChoice::Trick {
                                        animal,
                                        trick: trick.id,
                                    },
                                }),
                            },
                        );
                    }
                }
                if let Some(timing) = definitions_view
                    .show_scheduling()
                    .filter(|_| animal_rows.contains_key(&animal))
                {
                    let ticks = convert_authored_nanoseconds_to_fixed_simulation_ticks(
                        timing.time_category_ns,
                        definitions_view,
                    );
                    fragments.project(
                        &mut commands,
                        panel,
                        3,
                        ShowMixerFieldValues {
                            label: None,
                            icon: show_mixer_icon(definitions_view, "wait"),
                            kind: Some(ShowMixerControl::Edit {
                                slot,
                                time_slot,
                                choice: ShowMixerSlotChoice::Wait { animal, ticks },
                            }),
                        },
                    );
                }
                if slot.is_some() {
                    fragments.project(
                        &mut commands,
                        panel,
                        3,
                        ShowMixerFieldValues {
                            label: None,
                            icon: show_mixer_icon(definitions_view, "remove"),
                            kind: Some(ShowMixerControl::Edit {
                                slot,
                                time_slot,
                                choice: ShowMixerSlotChoice::Remove,
                            }),
                        },
                    );
                }
            }
            None => {}
        }
    }
}

fn show_mixer_icon(definitions: WorldDefinitionsView<'_>, name: &str) -> Option<Handle<Image>> {
    let icon = definitions.find_show_presentation_icon(AssetId::from_key(name))?;
    definitions.texture_image(icon.texture)
}

pub(super) fn bind_projected_show_mixer_fields(
    rows: Query<(&ShowMixerFieldValues, &Children), Added<ShowMixerFieldValues>>,
    children: Query<&Children>,
    mut fields: Query<(Option<&Name>, Option<&mut Text>, Option<&mut ImageNode>)>,
) {
    for (row, members) in &rows {
        for entity in members.iter().flat_map(|entry| {
            std::iter::once(entry).chain(children.iter_descendants::<Children>(entry))
        }) {
            let Ok((name, text, icon)) = fields.get_mut(entity) else {
                continue;
            };
            let name = name.map(Name::as_str);
            if matches!(name, Some("name" | "trick number")) {
                if let (Some(mut text), Some(label)) = (text, row.label.as_ref()) {
                    if text.0 != *label {
                        text.0.clone_from(label);
                    }
                }
            }
            if matches!(name, Some("icon" | "poplist display" | "Trick")) {
                if let (Some(mut image), Some(icon)) = (icon, row.icon.as_ref()) {
                    if image.image != *icon {
                        image.image = icon.clone();
                    }
                }
            }
        }
    }
}

pub(super) fn attach_show_mixer_tricks_to_authored_row(
    entries: Query<(Entity, &ChildOf), Added<ShowMixerTrickEntry>>,
    children: Query<&Children>,
    names: Query<&Name>,
    mut commands: Commands,
) {
    for (entity, owner) in &entries {
        if let Some(list) = children
            .iter_descendants::<Children>(owner.parent())
            .find(|&child| {
                names
                    .get(child)
                    .is_ok_and(|name| name.as_str() == "trick items")
            })
        {
            commands.entity(entity).insert(ChildOf(list));
        }
    }
}

pub(super) fn route_live_show_mixer_controls(
    mut activations: MessageReader<UiNodeActivated>,
    parents: Query<&ChildOf>,
    rows: Query<&ShowMixerFieldValues>,
    panels: Query<&ShowMixerPanelOf>,
    owners: Query<&UiDocumentOwner>,
    mut menus: Query<Option<&mut ShowMixerMenu>, With<ShowEditing>>,
    mut edits: MessageWriter<EditShowMixerSlot>,
    mut commands: Commands,
) {
    for activation in activations.read() {
        if activation.trigger != UiTrigger::Press {
            continue;
        }
        let ancestors = || {
            std::iter::successors(Some(activation.node), |entity| {
                parents.get(*entity).ok().map(ChildOf::parent)
            })
        };
        let Some(control) = ancestors().find_map(|entity| rows.get(entity).ok()?.kind.as_ref())
        else {
            continue;
        };
        let Some(target) = ancestors().find_map(|entity| panels.get(entity).ok()) else {
            continue;
        };
        let Ok(owner) = owners.get(target.0) else {
            continue;
        };
        let controller = parents.get(owner.0).map_or(owner.0, ChildOf::parent);
        let Ok(current) = menus.get_mut(controller) else {
            continue;
        };
        let menu = match control {
            ShowMixerControl::Open(menu) => *menu,
            ShowMixerControl::SelectAnimal(animal) => ShowMixerMenu::Tricks {
                animal: *animal,
                slot: None,
                time_slot: None,
            },
            ShowMixerControl::Edit {
                slot,
                time_slot,
                choice,
            } => {
                commands.entity(controller).remove::<ShowMixerMenu>();
                edits.write(EditShowMixerSlot {
                    controller,
                    slot: *slot,
                    time_slot: *time_slot,
                    choice: *choice,
                });
                continue;
            }
        };
        if let Some(mut current) = current {
            current.set_if_neq(menu);
        } else {
            commands.entity(controller).insert(menu);
        }
    }
}

pub(super) fn clear_closed_mixer_menu(event: On<Remove<ShowEditing>>, mut commands: Commands) {
    commands.entity(event.entity).try_remove::<ShowMixerMenu>();
}

pub(super) fn project_authored_show_score_pips(
    documents: Res<Assets<UiDocumentAsset>>,
    definitions: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    nodes: Query<(Entity, Ref<Name>, &UiDocumentOwner)>,
    roots: Query<(&UiDocumentRoot, Option<&ChildOf>)>,
    edited: Query<Ref<EditedShow>>,
    ratings: Query<Ref<openzt2_zoo_state::shows::show_schedule_types::ScheduledShowRating>>,
    children: Query<&Children>,
    mut images: Query<(&mut Node, Option<&mut ImageNode>)>,
) {
    let refresh = definitions.is_changed() || assets.is_changed() || documents.is_changed();
    let Some(policy) = definitions
        .get(&assets)
        .and_then(|view| view.show_editor_presentation())
    else {
        return;
    };
    for (entity, name, owner) in &nodes {
        if name.as_str() != "show score layer 3" {
            continue;
        }
        let Ok((root, parent)) = roots.get(owner.0) else {
            continue;
        };
        if !documents.get(&root.document).is_some_and(|document| {
            document.canonical_ui_document().role == UiDocumentRole::ShowEditor
        }) {
            continue;
        }
        let controller = parent.map_or(owner.0, ChildOf::parent);
        let selection = edited.get(controller).ok();
        let rating = selection.as_ref().and_then(|show| ratings.get(show.0).ok());
        if !refresh
            && !name.is_changed()
            && !selection.as_ref().is_some_and(Ref::is_changed)
            && !rating.as_ref().is_some_and(Ref::is_changed)
        {
            continue;
        }
        let ratio = rating
            .and_then(|rating| rating.0)
            .unwrap_or_default()
            .clamp(0.0, 5.0)
            / 5.0;
        let width = Val::Px(f32::from(policy.show_score_scaled_width) * ratio);
        for node in std::iter::once(entity).chain(children.iter_descendants::<Children>(entity)) {
            let Ok((mut layout, image)) = images.get_mut(node) else {
                continue;
            };
            if node == entity && layout.width != width {
                layout.width = width;
            }
            if let Some(mut image) = image {
                if let Some(mut source) = image.rect {
                    source.max.x =
                        source.min.x + f32::from(policy.show_score_pips_dimension) * ratio;
                    if image.rect != Some(source) {
                        image.rect = Some(source);
                    }
                }
            }
        }
    }
}
