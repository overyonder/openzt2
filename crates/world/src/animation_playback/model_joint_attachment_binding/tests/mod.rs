use super::*;

#[test]
fn model_joint_attachment_tracks_joint_and_instance_without_double_transform() {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, bevy::transform::TransformPlugin))
        .add_systems(Update, bind_model_joint_attachments);
    let root = app
        .world_mut()
        .spawn(Transform::from_xyz(10.0, 0.0, 0.0))
        .id();
    let joint = app
        .world_mut()
        .spawn((
            Name::new("Bip01 Tail"),
            Transform::from_xyz(0.0, 2.0, 0.0),
            ChildOf(root),
        ))
        .id();
    let attachment = app
        .world_mut()
        .spawn((
            Transform::from_xyz(100.0, 0.0, 0.0),
            ChildOf(root),
            PendingModelJointAttachment::new(root, "Bip01 Tail"),
        ))
        .id();
    let child = app
        .world_mut()
        .spawn((Transform::from_xyz(0.0, 0.0, 3.0), ChildOf(attachment)))
        .id();
    app.update();
    assert_eq!(
        app.world().get::<ChildOf>(attachment).unwrap().parent(),
        joint
    );
    assert!(app
        .world()
        .get::<PendingModelJointAttachment>(attachment)
        .is_none());
    app.world_mut()
        .entity_mut(joint)
        .insert(Transform::from_xyz(0.0, 5.0, 0.0));
    app.update();
    let position = app
        .world()
        .get::<GlobalTransform>(child)
        .unwrap()
        .translation();
    assert!((position - Vec3::new(10.0, 5.0, 3.0)).length() < 0.0001);
}

#[test]
fn model_joint_attachment_does_not_bind_to_its_own_descendant() {
    let mut app = App::new();
    app.add_systems(Update, bind_model_joint_attachments);
    let root = app.world_mut().spawn_empty().id();
    let attachment = app
        .world_mut()
        .spawn((
            ChildOf(root),
            PendingModelJointAttachment::new(root, "Tail"),
        ))
        .id();
    app.world_mut()
        .spawn((Name::new("Tail"), ChildOf(attachment)));
    app.update();
    assert_eq!(
        app.world().get::<ChildOf>(attachment).unwrap().parent(),
        root
    );
    assert!(app
        .world()
        .get::<PendingModelJointAttachment>(attachment)
        .is_some());
}
