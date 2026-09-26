use bevy::{prelude::*, tasks::Task};

#[derive(Component)]
pub(super) struct OnlineMessageFetchTask(pub(super) Task<Result<Vec<String>, String>>);

#[derive(Component)]
pub(super) struct OnlineMessageRequestStarted;

/// Presentation readiness for the authored rail. The source starts the rail
/// empty and exposes it only after online-message population produces a row.
#[derive(Component)]
pub(super) struct OnlineMessageContentAvailable;

/// The selected online notice or zoo greeting. Only the displayed row survives
/// deserialization; the remote feed does not become a retained catalogue.
#[derive(Component, Debug)]
pub(super) struct DisplayedOnlineMessageRow(pub(super) String);
