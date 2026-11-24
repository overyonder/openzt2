use super::*;
use openzt2_interface_state::ui::authored_ui_activation_contracts::UiNodeActivated;

#[test]
fn plain_notification_survives_projection_latency_without_localization() {
    let mut app = App::new();
    app.init_resource::<LocalizationPrecedenceIndex>()
        .init_resource::<Assets<LocalizationAsset>>()
        .add_message::<ShowNotification>()
        .add_message::<ShowUiRole>()
        .add_message::<NotificationRejected>()
        .add_message::<NotificationPresented>()
        .add_message::<UiNodeActivated>()
        .add_message::<crate::animation::UiAnimationPlaybackRequest>()
        .add_systems(
            Update,
            (
                request_notifications,
                populate_projected_notification_alerts,
                report_effectively_visible_notifications,
            )
                .chain(),
        );
    let operation = app.world_mut().spawn_empty().id();
    app.world_mut().write_message(ShowNotification {
        operation,
        message: NotificationMessage::Text("The selected save could not be read.".into()),
    });
    app.update();
    app.update();
    let show = app
        .world_mut()
        .resource_mut::<Messages<ShowUiRole>>()
        .drain()
        .collect::<Vec<_>>();
    assert_eq!(show.len(), 1);
    assert_eq!(show[0].role, UiDocumentRole::Modal);
    let owner = show[0].owner;
    for _ in 0..3 {
        app.update();
    }
    assert!(app.world().get::<PendingNotification>(owner).is_some());
    assert!(app
        .world()
        .resource::<Messages<NotificationPresented>>()
        .is_empty());
    let root = app.world_mut().spawn((ChildOf(owner),)).id();
    let alert = app
        .world_mut()
        .spawn((
            Visibility::Hidden,
            ChildOf(root),
            InheritedVisibility::HIDDEN,
        ))
        .id();
    let body = app
        .world_mut()
        .spawn((ChildOf(root), InheritedVisibility::HIDDEN, Text::new("")))
        .id();
    app.world_mut()
        .entity_mut(alert)
        .insert(ProjectedNotificationAlert { owner, body });
    app.update();
    assert!(app.world().get::<NotificationAlertShowing>(alert).is_some());
    assert!(app
        .world()
        .resource::<Messages<NotificationPresented>>()
        .is_empty());
    assert_eq!(
        app.world().get::<Text>(body).unwrap().0,
        "The selected save could not be read."
    );
    assert_eq!(
        *app.world().get::<Visibility>(alert).unwrap(),
        Visibility::Inherited
    );
    app.world_mut()
        .entity_mut(alert)
        .insert(InheritedVisibility::VISIBLE);
    app.update();
    let presented = app
        .world_mut()
        .resource_mut::<Messages<NotificationPresented>>()
        .drain()
        .collect::<Vec<_>>();
    assert_eq!(presented, [NotificationPresented { operation }]);
    assert!(app.world().get::<PendingNotification>(owner).is_none());
    app.update();
    assert!(app
        .world()
        .resource::<Messages<NotificationPresented>>()
        .is_empty());
    assert!(app.world().get_entity(owner).is_ok());
    app.world_mut()
        .entity_mut(alert)
        .insert(InheritedVisibility::HIDDEN);
    app.update();
    assert!(app.world().get_entity(owner).is_err());
    assert!(app.world().get_entity(root).is_err());
    assert!(app.world().get_entity(alert).is_err());
    assert!(app.world().get_entity(body).is_err());
}
