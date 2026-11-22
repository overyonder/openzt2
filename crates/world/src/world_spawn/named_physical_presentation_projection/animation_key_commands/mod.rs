use bevy::{
    ecs::relationship::{Relationship, RelationshipTarget},
    ecs::system::SystemParam,
    prelude::*,
};
use openzt2_game_assets::graphics_settings_types::GraphicsSettings;
use openzt2_game_assets::source_document::source_document_semantic_name::canonicalize_source_document_record_key;
use openzt2_game_data::animation::animation_text_key::AuthoredAnimationTextAction;
use openzt2_world_state::animation_graph::animation_presentation_relationship_types::AnimationPresentationOwner;
use openzt2_world_state::animation_playback::animation_event_message_types::{
    AnimationObjectCommand, AnimationObjectCommandDelivery,
};
use openzt2_world_state::animation_playback::animation_playback_controller_types::AnimationPlaybackController;
use openzt2_world_state::application_schedule::{FixedGameSet, GameSet};

use openzt2_game_assets::world_definitions::{
    world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
    world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
};
use openzt2_game_data::AssetId;
use openzt2_world_state::world_spawn::{
    prefab_authored_attachment_identifier::PrefabAuthoredAttachmentIdentifier,
    prefab_presentation_types::PrefabPresentation, world_membership_types::DefinitionId,
};

use super::{NamedPhysicalPresentation, NamedPhysicalPresentations};
use crate::animation_playback::{
    AnimationNamedPhysicalPresentationCommand, AuthoredAnimationRenderTextKeys,
};

#[derive(Event)]
struct ApplyAnimationNamedPhysicalPresentation(AnimationObjectCommand);

#[derive(SystemParam)]
struct AnimationBinderCatalogue<'w> {
    definitions: Res<'w, WorldDefinitions>,
    definition_assets: Res<'w, Assets<WorldDefinitionAsset>>,
    graphics: Option<Res<'w, GraphicsSettings>>,
}

pub(in crate::world_spawn) fn register_animation_named_physical_presentation_commands(
    app: &mut App,
) {
    app.add_message::<AnimationNamedPhysicalPresentationCommand<false>>()
        .add_message::<AnimationNamedPhysicalPresentationCommand<true>>()
        .add_observer(apply_animation_named_physical_presentation)
        .add_systems(
            FixedUpdate,
            deliver_animation_named_physical_presentation_commands::<false>
                .after(crate::animation_playback::BehaviorAnimationAliasDelivery)
                .in_set(AnimationObjectCommandDelivery)
                .in_set(FixedGameSet::Act),
        )
        .add_systems(
            Update,
            deliver_animation_named_physical_presentation_commands::<true>
                .after(AuthoredAnimationRenderTextKeys)
                .in_set(GameSet::Presentation),
        );
}

fn deliver_animation_named_physical_presentation_commands<const REAL_TIME: bool>(
    mut events: MessageReader<AnimationNamedPhysicalPresentationCommand<REAL_TIME>>,
    mut commands: Commands,
) {
    for event in events.read() {
        // A following key may destroy a binder created by this key.
        commands.trigger(ApplyAnimationNamedPhysicalPresentation(event.0.clone()));
    }
}

fn apply_animation_named_physical_presentation(
    event: On<ApplyAnimationNamedPhysicalPresentation>,
    mut commands: Commands,
    catalogue: AnimationBinderCatalogue,
    controllers: Query<(&AnimationPresentationOwner, &AnimationPlaybackController)>,
    owners: Query<(&DefinitionId, Option<&NamedPhysicalPresentations>)>,
    binders: Query<&PrefabAuthoredAttachmentIdentifier>,
) {
    let event = &event.0;
    let Ok((owner, playback)) = controllers.get(event.controller) else {
        return;
    };
    if event.playback_generation != playback.playback_generation
        || event.request_id != playback.explicit_clip_request_id
    {
        return;
    }
    let owner = owner.gameplay_entity;
    let Ok((definition, children)) = owners.get(owner) else {
        return;
    };
    let (key, create) = match &event.action {
        AuthoredAnimationTextAction::CreateObject { object_asset_key }
            if catalogue
                .graphics
                .as_ref()
                .is_none_or(|settings| settings.prop_attachments) =>
        {
            (object_asset_key, true)
        }
        AuthoredAnimationTextAction::CreateObjectOverride { object_asset_key } => {
            (object_asset_key, true)
        }
        AuthoredAnimationTextAction::DestroyObject { object_asset_key } => {
            (object_asset_key, false)
        }
        _ => return,
    };
    let binder_name = AssetId::from_key(&canonicalize_source_document_record_key(
        key.trim_matches(['"', '\'']),
    ));
    if !create {
        if let Some(binder) = children
            .into_iter()
            .flat_map(RelationshipTarget::iter)
            .find(|&entity| binders.get(entity).is_ok_and(|id| id.0 == binder_name))
        {
            commands.entity(binder).try_despawn();
        }
        return;
    }
    if commands.get_entity(event.joint).is_err() {
        return;
    }
    let Some(definitions) = catalogue.definitions.get(&catalogue.definition_assets) else {
        return;
    };
    let Some(presentation) = definitions.find_object(definition.0).and_then(|object| {
        object
            .named_physical_presentations
            .iter()
            .find(|binder| binder.name == binder_name)
    }) else {
        return;
    };
    let prefab = definitions.scene(presentation.prefab);
    if presentation.prefab != AssetId::default() && prefab.is_none() {
        return;
    }
    let mut binder = commands.spawn((
        Transform::from_scale(Vec3::splat(presentation.scale)),
        Visibility::Inherited,
        ChildOf(event.joint),
        PrefabAuthoredAttachmentIdentifier(presentation.name),
        <NamedPhysicalPresentation as Relationship>::from(owner),
    ));
    if let Some(prefab) = prefab {
        binder.insert(PrefabPresentation::new(prefab));
    }
}
