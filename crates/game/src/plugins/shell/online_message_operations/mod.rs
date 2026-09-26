use std::time::{Duration, SystemTime, UNIX_EPOCH};

use bevy::{
    prelude::*,
    tasks::{block_on, poll_once, IoTaskPool},
};
use serde::Deserialize;

use super::online_message_types::{
    DisplayedOnlineMessageRow, OnlineMessageContentAvailable, OnlineMessageFetchTask,
    OnlineMessageRequestStarted,
};

use crate::plugins::{
    settings::online_message_policy_types::OnlineMessagePolicy,
    ui::{
        authored_online_message_node_roles::{
            UiAuthoredOnlineMessageIcon, UiAuthoredOnlineMessageList,
            UiAuthoredOnlineMessageSurface, UiAuthoredOnlineMessageText,
        },
        authored_ui_node_projection_components::UiDocumentOwner,
        ui_document_lifecycle_contracts::ShowUiDocument,
    },
};

const DEFAULT_ONLINE_MESSAGE_FEED_URL: &str = "https://openzt2.org/motd.json";
const ONLINE_MESSAGE_ROW_UI_DOCUMENT_SOURCE_PATH: &str = "ui/layout/motditem.xml";
const BUILT_IN_ONLINE_MESSAGE_GREETINGS: &[&str] = &["Welcome to OpenZT2."];

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OnlineMessageFeed {
    #[serde(rename = "schema")]
    schema_version: u32,
    messages: Vec<String>,
    greetings: Vec<String>,
}

/// Starts one bounded HTTP request for an enabled, newly projected main-menu
/// surface. Network and JSON work stay on Bevy's I/O pool and cannot stall the
/// render or UI schedules.
pub(super) fn start_online_message_request_for_newly_projected_surfaces(
    mut commands: Commands,
    online_message_policy: Res<OnlineMessagePolicy>,
    online_message_surfaces: Query<
        Entity,
        (
            With<UiAuthoredOnlineMessageSurface>,
            Without<OnlineMessageRequestStarted>,
        ),
    >,
) {
    if !online_message_policy.enabled {
        return;
    }
    for online_message_surface in &online_message_surfaces {
        commands
            .entity(online_message_surface)
            .insert(OnlineMessageRequestStarted);
        commands.spawn((
            OnlineMessageFetchTask(
                IoTaskPool::get().spawn(async { fetch_online_messages_from_configured_endpoint() }),
            ),
            ChildOf(online_message_surface),
        ));
    }
}

/// Polls the asynchronous request without blocking. A failed or malformed
/// endpoint yields one built-in greeting, so the authored rail remains useful
/// offline and never exposes transport failures to the player.
pub(super) fn poll_online_message_requests_and_create_authored_rows(
    mut commands: Commands,
    mut online_message_fetch_tasks: Query<(Entity, &mut OnlineMessageFetchTask, &ChildOf)>,
    parent_relationships: Query<&ChildOf>,
    online_message_lists: Query<Entity, With<UiAuthoredOnlineMessageList>>,
    asset_server: Res<AssetServer>,
    mut show_ui_document_requests: MessageWriter<ShowUiDocument>,
) {
    for (task_entity, mut online_message_fetch_task, task_owner) in &mut online_message_fetch_tasks
    {
        let Some(fetch_result) = block_on(poll_once(&mut online_message_fetch_task.0)) else {
            continue;
        };
        commands.entity(task_entity).despawn();
        let online_messages = fetch_result.unwrap_or_else(|error| {
            warn!(%error, "online message request failed; using built-in zoo greeting");
            vec![select_built_in_online_message_greeting()]
        });
        let online_message_list = online_message_lists.iter().find(|candidate| {
            entity_is_descendant_of_ancestor(*candidate, task_owner.parent(), &parent_relationships)
        });
        let row_parent = online_message_list.unwrap_or(task_owner.parent());
        let online_message_row_document =
            asset_server.load(ONLINE_MESSAGE_ROW_UI_DOCUMENT_SOURCE_PATH);
        for online_message in online_messages {
            let message_row = commands
                .spawn((
                    DisplayedOnlineMessageRow(online_message),
                    Node {
                        width: Val::Percent(100.0),
                        height: Val::Px(72.0),
                        ..default()
                    },
                    ChildOf(row_parent),
                ))
                .id();
            show_ui_document_requests.write(ShowUiDocument {
                document: online_message_row_document.clone(),
                owner: message_row,
            });
        }
        commands
            .entity(task_owner.parent())
            .insert((OnlineMessageContentAvailable, Visibility::Inherited));
    }
}

/// Supplies the selected text to the original authored MOTD row fragment after
/// the ordinary UI projector has created its `MOTDText` entity.
pub(super) fn bind_displayed_online_message_to_authored_row_text(
    displayed_online_message_rows: Query<&DisplayedOnlineMessageRow>,
    mut authored_online_message_nodes: Query<
        (
            &UiDocumentOwner,
            Has<UiAuthoredOnlineMessageText>,
            Has<UiAuthoredOnlineMessageIcon>,
            &mut Text,
        ),
        (
            Or<(
                With<UiAuthoredOnlineMessageText>,
                With<UiAuthoredOnlineMessageIcon>,
            )>,
            Added<Text>,
        ),
    >,
) {
    for (document_owner, is_text, is_icon, mut text) in &mut authored_online_message_nodes {
        let Ok(displayed_online_message) = displayed_online_message_rows.get(document_owner.0)
        else {
            continue;
        };
        if is_text {
            text.0.clone_from(&displayed_online_message.0);
        } else if is_icon {
            text.0.clear();
        }
    }
}

/// Projects the accepted user preference onto the authored main-menu rail.
pub(super) fn project_online_message_policy_and_content_availability_to_surface(
    online_message_policy: Res<OnlineMessagePolicy>,
    mut online_message_surfaces: Query<(
        &mut Visibility,
        Ref<UiAuthoredOnlineMessageSurface>,
        Has<OnlineMessageContentAvailable>,
    )>,
) {
    for (mut visibility, online_message_surface, content_is_available) in
        &mut online_message_surfaces
    {
        if online_message_policy.is_changed() || online_message_surface.is_added() {
            *visibility = if online_message_policy.enabled && content_is_available {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
        }
    }
}

fn fetch_online_messages_from_configured_endpoint() -> Result<Vec<String>, String> {
    let online_message_feed_url = std::env::var("OPENZT2_MOTD_URL")
        .unwrap_or_else(|_| DEFAULT_ONLINE_MESSAGE_FEED_URL.to_owned());
    let http_agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(5)))
        .build()
        .into();
    let mut http_response = http_agent
        .get(online_message_feed_url)
        .call()
        .map_err(|error| error.to_string())?;
    let online_message_feed: OnlineMessageFeed = http_response
        .body_mut()
        .read_json()
        .map_err(|error| error.to_string())?;
    select_nonempty_messages_from_online_message_feed(online_message_feed)
}

fn select_nonempty_messages_from_online_message_feed(
    online_message_feed: OnlineMessageFeed,
) -> Result<Vec<String>, String> {
    if online_message_feed.schema_version != 1 {
        return Err(format!(
            "unsupported online message schema {}",
            online_message_feed.schema_version
        ));
    }

    let online_messages = online_message_feed
        .messages
        .into_iter()
        .filter(|message| !message.trim().is_empty())
        .collect::<Vec<_>>();
    if !online_messages.is_empty() {
        return Ok(online_messages);
    }

    let fallback_greetings = online_message_feed
        .greetings
        .into_iter()
        .filter(|greeting| !greeting.trim().is_empty())
        .collect::<Vec<_>>();
    fallback_greetings
        .get(select_time_varying_index_within_length(
            fallback_greetings.len(),
        ))
        .cloned()
        .map(|text| vec![text])
        .ok_or_else(|| "online message feed contains no notices or greetings".into())
}

fn select_built_in_online_message_greeting() -> String {
    BUILT_IN_ONLINE_MESSAGE_GREETINGS
        [select_time_varying_index_within_length(BUILT_IN_ONLINE_MESSAGE_GREETINGS.len())]
    .into()
}

fn select_time_varying_index_within_length(available_index_count: usize) -> usize {
    if available_index_count == 0 {
        return 0;
    }
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            elapsed.subsec_nanos() as usize % available_index_count
        })
}

fn entity_is_descendant_of_ancestor(
    descendant_entity: Entity,
    ancestor_entity: Entity,
    parent_relationships: &Query<&ChildOf>,
) -> bool {
    let mut current_ancestor_candidate = descendant_entity;
    while let Ok(parent_relationship) = parent_relationships.get(current_ancestor_candidate) {
        if parent_relationship.parent() == ancestor_entity {
            return true;
        }
        current_ancestor_candidate = parent_relationship.parent();
    }
    false
}
