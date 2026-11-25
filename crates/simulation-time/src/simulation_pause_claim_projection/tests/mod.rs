use super::*;

#[test]
fn forced_visible_claims_follow_hidden_ancestors_through_transfers_and_removal() {
    let mut app = App::new();
    app.add_message::<SimulationControlChanged>();
    configure_pause_claim_projection(&mut app);
    let root = app
        .world_mut()
        .spawn((
            WorldRoot {
                scenario: Default::default(),
            },
            SimulationControl::new(false),
            SimulationPaused(false),
            SimulationSpeedTier(0),
        ))
        .id();
    let parent = app.world_mut().spawn(Visibility::Hidden).id();
    let other = app.world_mut().spawn(Visibility::Inherited).id();
    let claim = app
        .world_mut()
        .spawn((
            UiDocumentOwner(parent),
            ChildOf(parent),
            Visibility::Visible,
            InheritedVisibility::VISIBLE,
            AuthoredUiTemporarySimulationPause::holding(),
        ))
        .id();

    app.update();
    assert_eq!(
        app.world().get::<SimulationPaused>(root),
        Some(&SimulationPaused(false))
    );

    app.world_mut().entity_mut(claim).insert(ChildOf(other));
    app.update();
    assert_eq!(
        app.world().get::<SimulationPaused>(root),
        Some(&SimulationPaused(true))
    );

    let transfer = app
        .world_mut()
        .spawn((
            Visibility::Inherited,
            InheritedVisibility::VISIBLE,
            UiDocumentPauseTransfer(true),
        ))
        .id();
    app.world_mut().entity_mut(claim).despawn();
    app.update();
    assert_eq!(
        app.world().get::<SimulationPaused>(root),
        Some(&SimulationPaused(true))
    );

    app.world_mut().entity_mut(transfer).despawn();
    app.update();
    assert_eq!(
        app.world().get::<SimulationPaused>(root),
        Some(&SimulationPaused(false))
    );
}
