//! Short, consistent contextual help for panels and windows.
//!
//! Tooltips explain one control; these sections explain the task of a whole
//! panel. They start collapsed so they do not displace the controls, are
//! reachable with Tab and the keyboard, and expose their text as a normal
//! AccessKit label when opened.

use egui::Ui;

/// One help section. Keyed so tests and callers never depend on the title.
pub struct Topic {
    pub key: &'static str,
    pub title: &'static str,
    pub text: &'static str,
}

/// Every contextual help section, in the order they tend to appear.
pub const TOPICS: &[Topic] = &[
    Topic {
        key: "file_dialog",
        title: "Files and folders",
        text: "Browse folders, type a path or file name, or focus the file list and use the arrow keys. Enter opens the highlighted folder or chooses the file. Show all files reveals formats the current action can still reject.",
    },
    Topic {
        key: "properties",
        title: "Properties",
        text: "The top shows the document size and unit. With an object selected, the panel edits its position, size, style and, for images, the preparation settings. Every change applies immediately and can be undone.",
    },
    Topic {
        key: "layers",
        title: "Layers",
        text: "Each row is a layer with visibility, lock, colour, cut entry and object count. The mode menu picks the cut entry shared by the layer; the arrows change the cutting order.",
    },
    Topic {
        key: "cuts",
        title: "Cuts and output",
        text: "A cut entry stores speed, power, passes, fill and output for every layer bound to it. Expressions can replace a numeric field with a formula over the document parameters.",
    },
    Topic {
        key: "export_image",
        title: "PNG export",
        text: "Exports the page, drawing or selection as a raster image. Resolution and size set the pixel grid; margin and offset add blank space around the output.",
    },
    Topic {
        key: "export_vector",
        title: "Vector export",
        text: "Exports the page, drawing or selection to SVG, DXF, PDF, EPS or HPGL. Choose the layers and the format options; elements that cannot be represented are listed after the export.",
    },
    Topic {
        key: "export_presets",
        title: "Export presets",
        text: "Presets store the export options you use often. Apply one to fill the current dialog, or save the current settings under a name.",
    },
    Topic {
        key: "image_adjust",
        title: "Adjust image",
        text: "Prepares the selected image for engraving: mode, tone, dithering, crop and masks. The preview shows the current settings; Apply updates the object and Cancel keeps the previous ones.",
    },
    Topic {
        key: "preview",
        title: "Preview and playback",
        text: "Shows the planned toolpaths and plays the job back. Playback speed and position affect only this preview, never the exported G-code.",
    },
];

/// Look up a section by key.
pub fn topic(key: &str) -> &'static Topic {
    TOPICS
        .iter()
        .find(|topic| topic.key == key)
        .unwrap_or_else(|| panic!("unknown contextual help key: {key}"))
}

/// Add a compact help button that opens this section.
///
/// The popup text is a normal label, so keyboard and screen-reader users reach
/// it the same way as the controls it describes.
pub fn button(ui: &mut Ui, key: &str) -> egui::Response {
    let topic = topic(key);
    let response = ui.menu_button("?", |ui| {
        ui.set_max_width(360.0);
        ui.strong(topic.title);
        ui.label(topic.text);
    });
    response.response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::Button,
            true,
            format!("Help for {}", topic.title),
        )
    });
    response
        .response
        .on_hover_text(format!("Help for {}", topic.title))
}

#[cfg(test)]
mod tests {
    use super::{TOPICS, topic};

    #[test]
    fn every_help_topic_has_a_useful_single_paragraph() {
        use std::collections::BTreeSet;

        let mut keys = BTreeSet::new();
        assert!(TOPICS.len() >= 9, "too few contextual help sections");
        for topic in TOPICS {
            assert!(keys.insert(topic.key), "duplicate help key {}", topic.key);
            assert_ne!(topic.title.trim(), "");
            assert!(
                !topic.text.trim().is_empty(),
                "{} has no help text",
                topic.key
            );
            assert_eq!(topic.text.trim(), topic.text, "{} has padding", topic.key);
            assert!(
                topic.text.len() >= 60,
                "{} is too short to explain the panel",
                topic.key
            );
            assert!(
                !topic.text.contains('\n'),
                "{} is not a single paragraph",
                topic.key
            );
        }
        assert_eq!(topic("cuts").title, "Cuts and output");
    }
}
