use bevy::text::EditableText;

/// Replaces projected UI text while retaining the existing allocation when
/// the projected value has not changed.
pub(in crate::plugins::information) fn replace_projected_ui_text_if_changed(
    projected_ui_text: &mut String,
    current_domain_text: &str,
) {
    if projected_ui_text == current_domain_text {
        return;
    }
    projected_ui_text.clear();
    projected_ui_text.push_str(current_domain_text);
}

pub(in crate::plugins::information) fn replace_projected_editable_ui_text_if_changed(
    projected_ui_text: &mut EditableText,
    current_domain_text: &str,
) {
    if projected_ui_text.value() == current_domain_text {
        return;
    }
    projected_ui_text.clear();
    projected_ui_text.editor_mut().set_text(current_domain_text);
}
