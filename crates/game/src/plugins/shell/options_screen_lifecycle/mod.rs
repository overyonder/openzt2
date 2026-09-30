use bevy::prelude::*;
use openzt2_game_data::ui_document::document::UiDocumentRole;

use crate::plugins::{
    settings::{
        display_settings_types::DisplaySettings, graphics_settings_types::GraphicsSettings,
        online_message_policy_types::OnlineMessagePolicy,
        options_screen_settings_draft_types::OptionsScreenDisplayGraphicsAndOnlineMessageDraft,
    },
    ui::ui_document_lifecycle_contracts::ShowUiRole,
};

use super::{
    shell_navigation_request_types::ShowOptions, shell_screen_presentation_types::MainMenuBackdrop,
    shell_selection_types::ShellScreen,
};

pub(super) fn replace_current_shell_screen_with_requested_options_screen(
    mut commands: Commands,
    mut options_screen_requests: MessageReader<ShowOptions>,
    current_shell_screens: Query<(Entity, &ShellScreen)>,
    mut requested_ui_roles: MessageWriter<ShowUiRole>,
    display: Res<DisplaySettings>,
    graphics: Res<GraphicsSettings>,
    online_messages: Res<OnlineMessagePolicy>,
    mut draft: ResMut<OptionsScreenDisplayGraphicsAndOnlineMessageDraft>,
) {
    for _ in options_screen_requests.read() {
        // Apply queues acceptance before its trailing Back resets the draft.
        // Each visit must start from the now-accepted settings.
        *draft = OptionsScreenDisplayGraphicsAndOnlineMessageDraft {
            display: *display,
            graphics: *graphics,
            message_of_the_day: online_messages.enabled,
            dirty_display: false,
            dirty_graphics: false,
        };
        for (shell_screen, _) in &current_shell_screens {
            commands.entity(shell_screen).despawn();
        }
        let options_screen_owner = commands
            .spawn((ShellScreen::Options, Visibility::Inherited))
            .id();
        let main_menu_backdrop_owner = commands
            .spawn((
                MainMenuBackdrop,
                Visibility::Inherited,
                GlobalZIndex(-1),
                ChildOf(options_screen_owner),
            ))
            .id();
        requested_ui_roles.write(ShowUiRole {
            role: UiDocumentRole::MainMenuBackdrop,
            owner: main_menu_backdrop_owner,
        });
        requested_ui_roles.write(ShowUiRole {
            role: UiDocumentRole::Options,
            owner: options_screen_owner,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reopening_options_uses_accepted_settings_and_replaces_the_previous_screen() {
        let mut app = App::new();
        app.add_message::<ShowOptions>()
            .add_message::<ShowUiRole>()
            .init_resource::<DisplaySettings>()
            .init_resource::<GraphicsSettings>()
            .init_resource::<OnlineMessagePolicy>()
            .init_resource::<OptionsScreenDisplayGraphicsAndOnlineMessageDraft>()
            .add_systems(
                Update,
                replace_current_shell_screen_with_requested_options_screen,
            );
        app.world_mut().write_message(ShowOptions);
        app.update();
        let previous_screen = app
            .world_mut()
            .query_filtered::<Entity, With<ShellScreen>>()
            .single(app.world())
            .unwrap();
        let accepted_graphics = GraphicsSettings::from_source_highest_detail_preset();
        app.world_mut().insert_resource(accepted_graphics);
        app.world_mut()
            .resource_mut::<OnlineMessagePolicy>()
            .enabled = false;
        app.world_mut()
            .resource_mut::<OptionsScreenDisplayGraphicsAndOnlineMessageDraft>()
            .dirty_display = true;

        app.world_mut().write_message(ShowOptions);
        app.update();

        let draft = app
            .world()
            .resource::<OptionsScreenDisplayGraphicsAndOnlineMessageDraft>();
        assert_eq!(draft.graphics, accepted_graphics);
        assert_eq!(draft.display, *app.world().resource::<DisplaySettings>());
        assert!(!draft.message_of_the_day);
        assert!(!draft.dirty_display);
        assert!(!draft.dirty_graphics);
        assert!(!app.world().entities().contains(previous_screen));
        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, With<ShellScreen>>()
                .iter(app.world())
                .count(),
            1
        );
    }
}
