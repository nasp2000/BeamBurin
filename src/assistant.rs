//! Text the Assistant panel copies for an agent session.
//!
//! Nothing here calls a model: the panel gives a person the exact launch line,
//! the token and a prompt that carries the current document state.

use std::fmt::Write as _;

use crate::document::Document;

/// A short prompt with the live document state and the safe workflow.
pub fn state_prompt(document: &Document) -> String {
    let report = crate::preflight::check(document);
    let holes = document
        .objects
        .iter()
        .filter(|object| object.hole_of.is_some())
        .count();
    let visible = document
        .objects
        .iter()
        .filter(|object| object.visible)
        .count();
    let mut text = String::new();
    text.push_str("# BeamWright project\n\n");
    let _ = writeln!(text, "Name: {}", document.name);
    let _ = writeln!(
        text,
        "Page: {} x {} mm, origin {}",
        crate::editor::renderer::format_mm(document.width_mm),
        crate::editor::renderer::format_mm(document.height_mm),
        match document.origin {
            crate::document::Origin::BottomLeft => "bottom-left",
            crate::document::Origin::TopLeft => "top-left",
        }
    );
    let _ = writeln!(
        text,
        "Objects: {} ({} visible, {} holes), layers: {}, operations: {}, parameters: {}",
        document.objects.len(),
        visible,
        holes,
        document.layers.len(),
        document.operations.len(),
        document.parameters.len()
    );
    let _ = writeln!(
        text,
        "Preflight: {} error(s), {} warning(s)",
        report.errors.len(),
        report.warnings.len()
    );
    if !report.errors.is_empty() {
        text.push_str("\n## Errors\n");
        for issue in &report.errors {
            let _ = writeln!(text, "- [{}] {}", issue.code, issue.message);
        }
    }
    if !report.warnings.is_empty() {
        text.push_str("\n## Warnings\n");
        for issue in &report.warnings {
            let _ = writeln!(text, "- [{}] {}", issue.code, issue.message);
        }
    }
    text.push_str(
        "\n## How to work\n\n\
1. Call `inspect` and `measure` before changing anything.\n\
2. Change the document with `apply_batch`; it is atomic and becomes one undo step.\n\
3. Call `check_job` and fix errors before exporting.\n\
4. Export with `export_gcode`, `export_svg` or `export_dxf`; the user loads the file into the sender of their choice.\n\
5. Never claim a physical result: speed, power and duration are estimates.\n",
    );
    text
}

/// Launch lines shown in the panel, with a short description.
pub fn launch_commands() -> Vec<(&'static str, &'static str)> {
    vec![
        ("Control the open editor", "BeamWright mcp --live"),
        ("Own document, no window", "BeamWright mcp"),
        (
            "Confined and read-only",
            "BeamWright mcp --workspace ~/laser-jobs --read-only",
        ),
        (
            "HTTP for a remote agent",
            "BeamWright mcp-http --token <token>",
        ),
    ]
}

/// A generic MCP client entry for the current executable.
pub fn client_config() -> String {
    serde_json::json!({
        "mcpServers": {
            "beamwright": {
                "command": "BeamWright",
                "args": ["mcp", "--live"]
            }
        }
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::{client_config, launch_commands, state_prompt};
    use crate::document::model::Object;
    use crate::document::{Document, Shape};

    #[test]
    fn the_prompt_carries_the_project_state() {
        let mut document = Document::new("Lamp", 200.0, 150.0);
        let layer_id = document.layers[0].id.clone();
        document.add_object(Object::new(
            layer_id,
            Shape::Rect {
                width: 20.0,
                height: 10.0,
                corner_radius: 0.0,
            },
        ));
        let text = state_prompt(&document);
        assert!(text.contains("Name: Lamp"));
        assert!(text.contains("200.0 x 150.0 mm"));
        assert!(text.contains("Objects: 1"));
        assert!(text.contains("Preflight: 0 error(s)"));
        assert!(text.contains("apply_batch"));
        assert!(text.contains("check_job"));
    }

    #[test]
    fn the_prompt_lists_preflight_problems_with_codes() {
        let mut document = Document::new("Notes", 200.0, 150.0);
        let layer_id = document.layers[0].id.clone();
        document.add_object(Object::new(
            layer_id,
            Shape::Text(crate::objects::TextObject {
                content: "Hello".to_string(),
                ..crate::objects::TextObject::default()
            }),
        ));
        let text = state_prompt(&document);
        assert!(text.contains("## Warnings"));
        assert!(text.contains("text-not-outlined"));
    }

    #[test]
    fn the_launch_lines_cover_the_safe_options() {
        let commands = launch_commands();
        assert_eq!(commands.len(), 4);
        assert!(
            commands
                .iter()
                .any(|(_, line)| line.contains("--read-only"))
        );
        assert!(
            commands
                .iter()
                .any(|(_, line)| line.contains("--workspace"))
        );
        assert!(commands.iter().any(|(_, line)| line.contains("--live")));
    }

    #[test]
    fn the_client_config_names_the_executable() {
        let config: serde_json::Value = serde_json::from_str(&client_config()).unwrap();
        assert_eq!(config["mcpServers"]["beamwright"]["command"], "BeamWright");
        assert_eq!(config["mcpServers"]["beamwright"]["args"][0], "mcp");
    }
}
