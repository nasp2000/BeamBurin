//! Interface layout actions: the default layout and the side-panel toggle.
//!
//! These actions never touch the document or its undo history; they only
//! change which panels and overlays are visible.

use crate::config::preferences::Workspace;

/// The layout `Reset to Default Layout` applies.
pub fn reset_layout() -> Workspace {
    Workspace::initial()
}

/// Hide both side panels, remembering their state, or restore it.
///
/// With either panel visible the pair is hidden and its state is stored; when
/// both are hidden the stored state is restored (both visible if there is no
/// memory yet). Other layout flags are not touched.
pub fn toggle_side_panels(
    layers: &mut bool,
    properties: &mut bool,
    remembered: &mut Option<(bool, bool)>,
) {
    if *layers || *properties {
        *remembered = Some((*layers, *properties));
        *layers = false;
        *properties = false;
    } else {
        let (remembered_layers, remembered_properties) = remembered.take().unwrap_or((true, true));
        *layers = remembered_layers;
        *properties = remembered_properties;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Document;
    use crate::editor::EditorState;

    #[test]
    fn reset_layout_restores_the_initial_visibility() {
        let workspace = reset_layout();
        assert!(workspace.layers_panel);
        assert!(workspace.properties_panel);
        assert!(workspace.rulers);
        assert!(workspace.grid);
        assert!(workspace.guides);
        assert!(workspace.tool_panel);
        assert_eq!(workspace.toolpaths, Workspace::initial().toolpaths);
    }

    #[test]
    fn reset_layout_does_not_touch_the_document() {
        let mut editor = EditorState::new(Document::new("Layout", 100.0, 100.0));
        let objects = editor.document.objects.len();
        let history = editor.history.undo_len();
        let workspace = reset_layout();
        assert!(workspace.layers_panel);
        assert_eq!(editor.document.objects.len(), objects);
        assert_eq!(editor.history.undo_len(), history);
    }

    #[test]
    fn toggle_side_panels_hides_both_and_remembers() {
        let (mut layers, mut properties) = (true, true);
        let mut remembered = None;
        toggle_side_panels(&mut layers, &mut properties, &mut remembered);
        assert!(!layers);
        assert!(!properties);
        assert_eq!(remembered, Some((true, true)));
    }

    #[test]
    fn toggle_side_panels_restores_the_previous_state() {
        let (mut layers, mut properties) = (true, false);
        let mut remembered = None;
        toggle_side_panels(&mut layers, &mut properties, &mut remembered);
        assert!(!layers);
        assert!(!properties);
        toggle_side_panels(&mut layers, &mut properties, &mut remembered);
        assert!(layers);
        assert!(!properties);
        assert!(remembered.is_none());
    }

    #[test]
    fn toggle_with_one_panel_hidden_remembers_it() {
        let (mut layers, mut properties) = (false, true);
        let mut remembered = None;
        toggle_side_panels(&mut layers, &mut properties, &mut remembered);
        assert_eq!(remembered, Some((false, true)));
        toggle_side_panels(&mut layers, &mut properties, &mut remembered);
        assert!(!layers);
        assert!(properties);
    }

    #[test]
    fn toggle_does_not_affect_other_layout_flags() {
        let mut workspace = Workspace::initial();
        let grid = workspace.grid;
        let guides = workspace.guides;
        let rulers = workspace.rulers;
        let toolpaths = workspace.toolpaths;
        let tool_panel = workspace.tool_panel;
        let mut remembered = None;
        toggle_side_panels(
            &mut workspace.layers_panel,
            &mut workspace.properties_panel,
            &mut remembered,
        );
        assert_eq!(workspace.grid, grid);
        assert_eq!(workspace.guides, guides);
        assert_eq!(workspace.rulers, rulers);
        assert_eq!(workspace.toolpaths, toolpaths);
        assert_eq!(workspace.tool_panel, tool_panel);
    }
}
