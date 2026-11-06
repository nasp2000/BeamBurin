use super::*;
use crate::geometry::Point2;

/// Whether the image object with this id has its negative switch on.
fn image_is_negative(document: &crate::document::Document, id: &str) -> bool {
    let object = document.object(id).expect("the image is missing");
    let crate::document::Shape::Image(image) = &object.shape else {
        panic!("the object is not an image");
    };
    image.settings.invert
}

#[test]
fn invert_color_image_swaps_light_and_dark() {
    let image = egui::ColorImage::new([1, 1], vec![egui::Color32::from_rgb(10, 20, 30)]);
    let inverted = super::invert_color_image(image);
    assert_eq!(inverted.pixels[0], egui::Color32::from_rgb(245, 235, 225));
}

#[test]
fn preview_marks_burn_black() {
    let bitmap = crate::raster::RasterBitmap {
        width: 2,
        height: 1,
        power: vec![255, 0],
    };
    let image = preview_color_image(&bitmap, false);
    assert_eq!(image.size, [2, 1]);
    assert_eq!(image.pixels[0], egui::Color32::BLACK);
    assert_eq!(image.pixels[1], egui::Color32::WHITE);
}

#[test]
fn burn_map_colours_burned_and_unburned_pixels() {
    let bitmap = crate::raster::RasterBitmap {
        width: 2,
        height: 1,
        power: vec![255, 0],
    };
    let image = preview_color_image(&bitmap, true);
    assert_eq!(image.size, [2, 1]);
    assert_eq!(image.pixels[0], egui::Color32::from_rgb(0xFF, 0x7A, 0x1A));
    assert_eq!(image.pixels[1], egui::Color32::from_rgb(16, 17, 19));
    assert_ne!(image.pixels[0], egui::Color32::BLACK);
    assert_ne!(image.pixels[1], egui::Color32::WHITE);
}

#[test]
fn preview_downscales_large_bitmaps() {
    let bitmap = crate::raster::RasterBitmap {
        width: 1000,
        height: 500,
        power: vec![0; 500_000],
    };
    let image = preview_color_image(&bitmap, false);
    assert!(image.size[0] <= 320);
    assert!(image.size[1] <= 320);
    assert_eq!(image.size[0], 250);
}

#[test]
fn preview_handles_single_pixel() {
    let bitmap = crate::raster::RasterBitmap {
        width: 1,
        height: 1,
        power: vec![255],
    };
    let image = preview_color_image(&bitmap, false);
    assert_eq!(image.size, [1, 1]);
    assert_eq!(image.pixels[0], egui::Color32::BLACK);
    let burn_image = preview_color_image(&bitmap, true);
    assert_eq!(
        burn_image.pixels[0],
        egui::Color32::from_rgb(0xFF, 0x7A, 0x1A)
    );
}

#[test]
fn the_tour_has_short_non_empty_steps() {
    let steps = tour_steps();
    assert!(steps.len() >= 4);
    for (title, body) in steps {
        assert_ne!(*title, "");
        assert_ne!(*body, "");
    }
}

#[test]
fn the_measurement_hud_formats_draw_and_marquee_values() {
    let measure = DragState::Draw {
        tool: Tool::Measure,
        start: Point2::new(0.0, 0.0),
        end: Point2::new(3.0, 4.0),
        constrain: false,
    };
    let text = drag_measurement(Some(&measure)).unwrap();
    assert!(text.contains("5.00 mm"), "{text}");
    assert!(text.contains("53.1"), "{text}");

    let rectangle = DragState::Draw {
        tool: Tool::Rect,
        start: Point2::new(10.0, 5.0),
        end: Point2::new(0.0, 0.0),
        constrain: false,
    };
    assert_eq!(
        drag_measurement(Some(&rectangle)).unwrap(),
        "10.00 × 5.00 mm"
    );

    let polygon = DragState::Draw {
        tool: Tool::Polygon,
        start: Point2::new(0.0, 0.0),
        end: Point2::new(0.0, 2.5),
        constrain: false,
    };
    assert_eq!(drag_measurement(Some(&polygon)).unwrap(), "r 2.50 mm");

    let marquee = DragState::Marquee {
        start: Point2::new(0.0, 0.0),
        end: Point2::new(2.0, 1.5),
        additive: false,
    };
    assert_eq!(drag_measurement(Some(&marquee)).unwrap(), "2.00 × 1.50 mm");

    assert!(drag_measurement(None).is_none());
    let moving = DragState::Move {
        last: Point2::new(1.0, 1.0),
        recorded: true,
    };
    assert!(drag_measurement(Some(&moving)).is_none());
}

#[test]
fn the_palette_finds_objects_layers_operations_and_parameters() {
    let mut document = Document::new("Test", 100.0, 100.0);
    document.layers[0].name = "Engraving".to_string();
    let layer_id = document.layers[0].id.clone();
    let mut object = crate::document::Object::new(
        layer_id,
        Shape::Rect {
            width: 5.0,
            height: 5.0,
            corner_radius: 0.0,
        },
    );
    object.name = "Logo plate".to_string();
    document.add_object(object);
    document.operations.push(crate::operations::Operation::new(
        "Deep cut",
        crate::operations::OperationType::OutlineCutting,
    ));
    document.parameters = vec![crate::parameters::Parameter::new("feed", "900")];

    assert_eq!(filter_objects(&document, "logo").len(), 1);
    assert_eq!(filter_objects(&document, "RECTANGLE").len(), 1);
    assert_eq!(filter_objects(&document, "zzz").len(), 0);
    assert_eq!(filter_objects(&document, "").len(), 0);

    let layers = filter_named(document.layers.iter(), "engra", |layer| layer.name.as_str());
    assert_eq!(layers.len(), 1);
    let operations = filter_named(document.operations.iter(), "deep", |operation| {
        operation.name.as_str()
    });
    assert_eq!(operations.len(), 1);
    let parameters = filter_named(document.parameters.iter(), "FEED", |parameter| {
        parameter.name.as_str()
    });
    assert_eq!(parameters.len(), 1);
    assert_eq!(
        filter_named(document.parameters.iter(), "none", |parameter| {
            parameter.name.as_str()
        })
        .len(),
        0
    );
}

#[test]
fn the_palette_filters_entries_case_insensitively() {
    let entries = crate::commands::ALL;
    assert_eq!(filter_palette(entries, "").len(), entries.len());
    assert_eq!(filter_palette(entries, "   ").len(), entries.len());
    let labels = |query: &str| -> Vec<&'static str> {
        filter_palette(entries, query)
            .into_iter()
            .map(|index| entries[index].label)
            .collect()
    };
    assert!(labels("undo").contains(&"Undo"));
    assert!(labels("UNDO").contains(&"Undo"));
    assert!(labels("g-code").contains(&"Export G-code..."));
    assert!(labels("preferences").contains(&"Preferences..."));
    assert_eq!(labels("zzzzzz").len(), 0);
    // The key and the category are searchable too, not just the label.
    assert!(labels("file.save").contains(&"Save"));
    assert!(labels("help").contains(&"Keyboard shortcuts"));
    let mut seen = std::collections::HashSet::new();
    for command in entries {
        assert_ne!(command.label, "");
        assert!(
            seen.insert(command.label),
            "duplicate palette label {}",
            command.label
        );
    }
}

#[test]
fn object_context_menu_matches_the_selection() {
    let none = object_context_actions(0, false, false);
    assert!(none.contains(&ContextAction::Cut));
    assert!(!none.contains(&ContextAction::Group));
    assert!(!none.contains(&ContextAction::EditNodes));
    assert!(!none.contains(&ContextAction::TextToOutlines));

    let pair = object_context_actions(2, false, false);
    assert!(pair.contains(&ContextAction::Group));
    assert!(pair.contains(&ContextAction::CutSettings));
    assert!(pair.contains(&ContextAction::SelectSameColour));
    assert!(!pair.contains(&ContextAction::EditNodes));

    let text = object_context_actions(1, true, false);
    assert!(text.contains(&ContextAction::EditNodes));
    assert!(text.contains(&ContextAction::TextToOutlines));
    assert!(!text.contains(&ContextAction::Group));

    let image = object_context_actions(1, false, true);
    assert!(image.contains(&ContextAction::AdjustImage));
    assert!(!image.contains(&ContextAction::TextToOutlines));
}

#[test]
fn opening_a_layer_cut_entry_binds_it() {
    let mut app = ui_test_app();
    let layer = app.editor.active_layer_id.clone();
    app.editor
        .add_rectangle(Point2::new(10.0, 10.0), Point2::new(30.0, 25.0));
    app.open_cut_settings_for_layer(&layer);
    assert!(app.cuts_open, "the cut settings did not open");
    let operation = app
        .active_operation
        .clone()
        .expect("no cut entry became active");
    let document = &app.editor.document;
    assert_eq!(
        document.operations[0].params.color, "#000000",
        "the cut entry does not use the layer colour"
    );
    assert_eq!(
        document.layer(&layer).unwrap().operation_id.as_deref(),
        Some(operation.as_str()),
        "the layer was not bound to its cut entry"
    );
    assert_eq!(
        document.objects[0].operation_id.as_deref(),
        Some(operation.as_str()),
        "the object was not bound to the cut entry"
    );
}

#[test]
fn the_object_menu_opens_the_cut_settings_of_its_layer() {
    let mut app = ui_test_app();
    let layer = app.editor.active_layer_id.clone();
    app.editor
        .add_rectangle(Point2::new(10.0, 10.0), Point2::new(30.0, 25.0));
    app.run_context_action(ContextAction::CutSettings);
    assert!(app.cuts_open, "the cut settings did not open");
    let operation = app
        .active_operation
        .clone()
        .expect("no cut entry became active");
    let document = &app.editor.document;
    let entry = document
        .operations
        .iter()
        .find(|entry| entry.id == operation)
        .expect("the cut entry is missing");
    assert_eq!(entry.params.color, "#000000");
    assert_eq!(
        document.layer(&layer).unwrap().operation_id.as_deref(),
        Some(operation.as_str()),
        "the layer was not bound to the cut entry"
    );
    assert_eq!(
        document.objects[0].operation_id.as_deref(),
        Some(operation.as_str()),
        "the object was not bound to the cut entry"
    );
}

#[test]
fn the_object_menu_selects_the_same_colour_across_layers() {
    let mut app = ui_test_app();
    let first = app
        .editor
        .add_rectangle(Point2::new(10.0, 10.0), Point2::new(20.0, 20.0))
        .expect("the first rectangle was not added");
    let second_layer = app.editor.document.add_layer("Layer 2").id.clone();
    app.editor.active_layer_id = second_layer;
    let second = app
        .editor
        .add_rectangle(Point2::new(30.0, 10.0), Point2::new(40.0, 20.0))
        .expect("the second rectangle was not added");
    for id in [&first, &second] {
        app.editor.document.object_mut(id).unwrap().color = Some("#e53935".to_string());
    }
    app.editor.selection.clear();
    app.editor.selection.add(first.clone());
    app.run_context_action(ContextAction::SelectSameColour);
    assert_eq!(app.editor.selection.len(), 2, "the other layer was missed");
    assert!(app.editor.selection.contains(&second));
    assert!(app.status.contains("#e53935"), "{}", app.status);
}

#[test]
fn the_row_menu_deletes_the_cut_entry() {
    use egui_kittest::kittest::Queryable as _;

    let mut app = ui_test_app();
    let layer = app.editor.active_layer_id.clone();
    app.editor
        .add_rectangle(Point2::new(10.0, 10.0), Point2::new(30.0, 25.0));
    let mut harness = ui_harness(app);
    harness.run();
    assert_eq!(
        harness.state().editor.document.operations.len(),
        1,
        "drawing did not create the cut entry"
    );
    harness.get_by_label("Layer 1").click_secondary();
    harness.run();
    harness.get_by_label("Delete cut entry").click_accesskit();
    harness.run();
    let document = &harness.state().editor.document;
    assert!(
        document.operations.is_empty(),
        "the cut entry was not deleted"
    );
    assert!(
        document.layer(&layer).unwrap().operation_id.is_none(),
        "the layer kept the deleted cut entry"
    );
    assert!(
        document.objects[0].operation_id.is_none(),
        "the object kept the deleted cut entry"
    );
}

#[test]
fn canvas_context_menu_offers_work_area_commands() {
    let actions = canvas_context_actions();
    assert!(actions.contains(&ContextAction::Paste));
    assert!(actions.contains(&ContextAction::SelectAll));
    assert!(
        actions.contains(&ContextAction::GroupByColour),
        "the work-area menu has no colour grouping"
    );
    assert!(actions.contains(&ContextAction::ZoomIn));
    assert!(actions.contains(&ContextAction::FitPage));
    assert!(actions.contains(&ContextAction::DocumentProperties));
    assert!(actions.contains(&ContextAction::Preferences));
    assert!(!actions.contains(&ContextAction::Cut));
    assert!(!actions.contains(&ContextAction::Delete));
}

#[test]
fn opening_a_dialog_uses_the_in_app_browser() {
    let mut app = BeamBurinApp::new(None);
    app.open_dialog(FileMode::ExportGcode);
    let dialog = app.dialog.as_ref().expect("the dialog is open");
    assert_eq!(dialog.mode, FileMode::ExportGcode);
    assert!(
        std::path::Path::new(&dialog.file_name)
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("nc")),
        "{}",
        dialog.file_name
    );
}

#[test]
fn undo_status_names_the_action() {
    let mut app = BeamBurinApp::new(None);
    app.editor
        .add_rectangle(Point2::new(0.0, 0.0), Point2::new(10.0, 10.0));
    app.editor.record_action("Move");
    app.editor.move_selection(2.0, 0.0);

    assert_eq!(app.undo_label(), "Undo Move");
    app.undo();
    assert_eq!(app.status, "Undo Move");
    assert_eq!(app.redo_label(), "Redo Move");
    app.redo();
    assert_eq!(app.status, "Redo Move");
}

fn ui_harness(app: BeamBurinApp) -> egui_kittest::Harness<'static, BeamBurinApp> {
    egui_kittest::Harness::builder()
        .with_size(egui::vec2(1280.0, 800.0))
        .build_ui_state(|ui, app| app.show(ui), app)
}

fn ui_temp_path(prefix: &str) -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(0);
    std::env::temp_dir().join(format!(
        "beamburin_ui_{prefix}_{}_{}.tmp",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}

fn ui_test_app() -> BeamBurinApp {
    let mut app = BeamBurinApp::new_for_ui_test();
    app.preferences = crate::config::preferences::AppPreferences::default();
    app.preferences_path = ui_temp_path("preferences");
    app.recovery_path = ui_temp_path("recovery");
    app.profiles_path = ui_temp_path("profiles");
    app.machines_path = ui_temp_path("machines");
    app.last_autosave = std::time::Instant::now();
    app.tour_open = false;
    app.recovery_prompt_open = false;
    app.machine_wizard_open = false;
    app.machines = crate::machines::MachineLibrary::default();
    app
}

fn wait_for_task(harness: &mut egui_kittest::Harness<'static, BeamBurinApp>) {
    let ctx = harness.ctx.clone();
    for _ in 0..600 {
        harness.state_mut().advance(&ctx);
        harness.step();
        if harness.state().task.is_none() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    panic!("a background task did not finish");
}

#[test]
fn the_ui_keeps_the_menus_and_the_toolbar_align_actions() {
    use egui_kittest::kittest::Queryable as _;

    let mut harness = ui_harness(ui_test_app());
    harness.run();
    for label in [
        "File",
        "Edit",
        "Draw",
        "Operations",
        "Materials and profiles",
        "Arrange",
        "View",
        "Help",
    ] {
        assert!(
            harness.query_by_label(label).is_some(),
            "missing menu {label}"
        );
    }
    assert!(
        harness.query_all_by_label("Align").next().is_none(),
        "the Align menu should be gone"
    );
    for label in [
        "Align left",
        "Align centre horizontally",
        "Align right",
        "Align top",
        "Align centre vertically",
        "Align bottom",
        "Distribute horizontally",
        "Distribute vertically",
    ] {
        assert!(
            harness.query_by_label(label).is_some(),
            "missing toolbar action {label}"
        );
    }
    harness.get_by_label("File").click();
    harness.run();
    for entry in [
        "New",
        "Open...",
        "Save",
        "Export G-code...",
        "Export SVG...",
    ] {
        assert!(
            harness.query_all_by_label(entry).next().is_some(),
            "missing File entry {entry}"
        );
    }
}

#[test]
fn the_side_panel_switches_between_properties_and_tool() {
    use egui_kittest::kittest::Queryable as _;

    let mut harness = ui_harness(ui_test_app());
    harness.run();
    assert!(harness.query_by_label("Properties").is_some());
    harness.get_by_label("Tool").click();
    harness.run();
    assert!(
        harness.query_by_label("In progress").is_some(),
        "the Tool tab did not show its content"
    );
    harness.get_by_label("Properties").click();
    harness.run();
    assert!(
        harness.query_by_label("Document").is_some(),
        "the Properties tab did not come back"
    );
}

#[test]
fn a_selected_object_exposes_the_plus_and_minus_controls() {
    use egui_kittest::kittest::Queryable as _;

    let mut app = ui_test_app();
    if let Some(id) = app
        .editor
        .add_rectangle(Point2::new(10.0, 10.0), Point2::new(30.0, 25.0))
    {
        app.editor.selection.add(id);
    }
    let mut harness = ui_harness(app);
    harness.run();
    assert!(harness.query_by_label("Position (mm)").is_some());
    assert!(
        harness.query_all_by_label("+").count() >= 5,
        "the size/position plus controls are missing"
    );
    assert!(
        harness.query_all_by_label("-").count() >= 5,
        "the size/position minus controls are missing"
    );
}

#[test]
fn every_top_level_menu_opens_with_its_main_entry() {
    use egui_kittest::kittest::Queryable as _;

    let mut harness = ui_harness(ui_test_app());
    harness.run();
    for (menu, entry) in [
        ("File", "Export G-code..."),
        ("Edit", "Select all"),
        ("Draw", "Rectangle"),
        ("Operations", "Cuts / Layers..."),
        ("Materials and profiles", "Materials and operations..."),
        ("Arrange", "Bring to front"),
        ("View", "Command palette..."),
        ("Help", "About BeamBurin"),
    ] {
        harness.get_by_label(menu).click_accesskit();
        harness.run();
        assert!(
            harness.query_all_by_label(entry).next().is_some(),
            "missing {entry} in {menu}"
        );
        harness.key_press(egui::Key::Escape);
        harness.run();
    }
}

#[test]
fn the_edit_menu_selects_the_open_shapes() {
    use egui_kittest::kittest::Queryable as _;

    let mut app = ui_test_app();
    let layer = app.editor.document.layers[0].id.clone();
    let open = crate::geometry::Path::from_points(&[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0)], false);
    let open_id = {
        let object =
            crate::document::Object::new(layer.clone(), crate::document::Shape::Path(open));
        let id = object.id.clone();
        app.editor.document.add_object(object);
        id
    };
    app.editor
        .add_rectangle(Point2::new(20.0, 20.0), Point2::new(30.0, 30.0));
    let mut harness = ui_harness(app);
    harness.run();
    harness.get_by_label("Edit").click_accesskit();
    harness.run();
    harness.get_by_label("Select open shapes").click_accesskit();
    harness.run();
    let app = harness.state();
    assert_eq!(
        app.editor.selection.len(),
        1,
        "the wrong objects were selected"
    );
    assert!(app.editor.selection.contains(&open_id));
}

#[test]
fn the_arrange_menu_converts_a_rectangle_to_a_path() {
    use egui_kittest::kittest::Queryable as _;

    let mut app = ui_test_app();
    let id = app
        .editor
        .add_rectangle(Point2::new(10.0, 10.0), Point2::new(30.0, 25.0))
        .expect("the rectangle was not added");
    app.editor.selection.clear();
    app.editor.selection.add(id.clone());
    let mut harness = ui_harness(app);
    harness.run();
    harness.get_by_label("Arrange").click_accesskit();
    harness.run();
    harness.get_by_label("Convert ⏵").click_accesskit();
    harness.run();
    harness.get_by_label("Convert to path").click_accesskit();
    harness.run();
    let object = harness
        .state()
        .editor
        .document
        .object(&id)
        .expect("the object disappeared");
    assert!(
        matches!(object.shape, crate::document::Shape::Path(_)),
        "the rectangle was not converted"
    );
}

#[test]
fn the_text_properties_can_force_upper_case() {
    use egui_kittest::kittest::Queryable as _;

    let mut app = ui_test_app();
    let id = app
        .editor
        .add_text(Point2::new(10.0, 10.0))
        .expect("the text object was not added");
    app.editor.selection.clear();
    app.editor.selection.add(id.clone());
    let mut harness = ui_harness(app);
    harness.run();
    harness
        .get_by_role_and_label(egui::accesskit::Role::CheckBox, "Force upper case")
        .click_accesskit();
    harness.run();
    let object = harness
        .state()
        .editor
        .document
        .object(&id)
        .expect("the text disappeared");
    let crate::document::Shape::Text(text) = &object.shape else {
        panic!("the object is not text");
    };
    assert!(text.uppercase, "the upper-case switch did not apply");
}

#[test]
fn the_arrange_menu_welds_overlapping_shapes() {
    use egui_kittest::kittest::Queryable as _;

    let mut app = ui_test_app();
    let first = app
        .editor
        .add_rectangle(Point2::new(0.0, 0.0), Point2::new(40.0, 20.0))
        .expect("the first rectangle was not added");
    let second = app
        .editor
        .add_rectangle(Point2::new(20.0, 5.0), Point2::new(60.0, 25.0))
        .expect("the second rectangle was not added");
    app.editor.selection.clear();
    app.editor.selection.add(first);
    app.editor.selection.add(second);
    let mut harness = ui_harness(app);
    harness.run();
    harness.get_by_label("Arrange").click_accesskit();
    harness.run();
    harness.get_by_label("Combine ⏵").click_accesskit();
    harness.run();
    harness.get_by_label("Weld shapes").click_accesskit();
    harness.run();
    assert_eq!(
        harness.state().editor.document.objects.len(),
        1,
        "the weld left more than one object"
    );
    let (min, max) = harness.state().editor.document.objects[0]
        .bounds()
        .expect("the welded object has no bounds");
    assert!((min.x).abs() < 1e-6);
    assert!((max.x - 60.0).abs() < 1e-6);
}

#[test]
fn the_arrange_menu_sets_and_clears_the_corner_radius() {
    use egui_kittest::kittest::Queryable as _;

    let mut app = ui_test_app();
    let id = app
        .editor
        .add_rectangle(Point2::new(10.0, 10.0), Point2::new(30.0, 25.0))
        .expect("the rectangle was not added");
    app.editor.selection.clear();
    app.editor.selection.add(id.clone());
    let mut harness = ui_harness(app);
    harness.run();
    harness.get_by_label("Arrange").click_accesskit();
    harness.run();
    harness.get_by_label("Convert ⏵").click_accesskit();
    harness.run();
    harness.get_by_label("Corner radius...").click_accesskit();
    harness.run();
    assert!(
        harness.state().corner_radius_open,
        "the corner radius window did not open"
    );
    harness.state_mut().corner_radius_value = 2.0;
    harness.run();
    harness.get_by_label("Apply").click_accesskit();
    harness.run();
    let object = harness
        .state()
        .editor
        .document
        .object(&id)
        .expect("the rectangle disappeared");
    let crate::document::Shape::Rect { corner_radius, .. } = object.shape else {
        panic!("the object is not a rectangle");
    };
    assert!(
        (corner_radius - 2.0).abs() < 1e-9,
        "radius is {corner_radius}"
    );
    assert!(!harness.state().corner_radius_open);

    harness.get_by_label("Arrange").click_accesskit();
    harness.run();
    harness.get_by_label("Convert ⏵").click_accesskit();
    harness.run();
    harness
        .get_by_label("Clear corner radius")
        .click_accesskit();
    harness.run();
    let object = harness
        .state()
        .editor
        .document
        .object(&id)
        .expect("the rectangle disappeared");
    let crate::document::Shape::Rect { corner_radius, .. } = object.shape else {
        panic!("the object is not a rectangle");
    };
    assert!(corner_radius.abs() < 1e-9, "radius is {corner_radius}");
}

#[test]
fn the_edit_menu_selects_the_layer_and_clears_the_selection() {
    use egui_kittest::kittest::Queryable as _;

    let mut app = ui_test_app();
    app.editor
        .add_rectangle(Point2::new(0.0, 0.0), Point2::new(10.0, 10.0))
        .expect("the first rectangle was not added");
    app.editor
        .add_rectangle(Point2::new(20.0, 0.0), Point2::new(30.0, 10.0))
        .expect("the second rectangle was not added");
    app.editor.selection.clear();
    let mut harness = ui_harness(app);
    harness.run();
    harness.get_by_label("Edit").click_accesskit();
    harness.run();
    harness
        .get_by_label("Select all on layer")
        .click_accesskit();
    harness.run();
    assert_eq!(
        harness.state().editor.selection.len(),
        2,
        "the layer objects were not selected"
    );
    harness.get_by_label("Edit").click_accesskit();
    harness.run();
    harness.get_by_label("Select none").click_accesskit();
    harness.run();
    assert!(
        harness.state().editor.selection.is_empty(),
        "the selection was not cleared"
    );
}

#[test]
fn the_arrange_menu_moves_the_selection_to_a_position() {
    use egui_kittest::kittest::Queryable as _;

    let mut app = ui_test_app();
    let id = app
        .editor
        .add_rectangle(Point2::new(10.0, 10.0), Point2::new(30.0, 25.0))
        .expect("the rectangle was not added");
    app.editor.selection.add(id);
    let mut harness = ui_harness(app);
    harness.run();
    harness.get_by_label("Arrange").click_accesskit();
    harness.run();
    harness
        .get_by_label("Move to position...")
        .click_accesskit();
    harness.run();
    let app = harness.state();
    assert!(app.position_open, "the position window did not open");
    assert!((app.position_x - 10.0).abs() < 1e-9);
    assert!((app.position_y - 10.0).abs() < 1e-9);
    assert!(harness.query_by_label("Apply").is_some());
    assert!(harness.query_by_label("Cancel").is_some());
    harness.state_mut().position_x = 25.0;
    harness.run();
    harness.get_by_label("Apply").click_accesskit();
    harness.run();
    let app = harness.state();
    assert!(!app.position_open, "the window stayed open");
    let (min, _) = app
        .editor
        .selected_bounds()
        .expect("the selection disappeared");
    assert!((min.x - 25.0).abs() < 1e-9, "x is {}", min.x);
    assert!((min.y - 10.0).abs() < 1e-9, "y is {}", min.y);
}

#[test]
fn the_arrange_menu_brings_the_selection_forward_one_step() {
    use egui_kittest::kittest::Queryable as _;

    let mut app = ui_test_app();
    let first = app
        .editor
        .add_rectangle(Point2::new(0.0, 0.0), Point2::new(10.0, 10.0))
        .expect("the first rectangle was not added");
    let second = app
        .editor
        .add_rectangle(Point2::new(20.0, 0.0), Point2::new(30.0, 10.0))
        .expect("the second rectangle was not added");
    app.editor.selection.clear();
    app.editor.selection.add(first.clone());
    let mut harness = ui_harness(app);
    harness.run();
    harness.get_by_label("Arrange").click_accesskit();
    harness.run();
    harness.get_by_label("Bring forward").click_accesskit();
    harness.run();
    let ids: Vec<String> = harness
        .state()
        .editor
        .document
        .objects
        .iter()
        .map(|object| object.id.clone())
        .collect();
    assert_eq!(
        ids,
        vec![second, first],
        "the order did not change by one step"
    );
}

#[test]
fn toolbar_toggles_and_history_change_the_model() {
    use egui_kittest::kittest::Queryable as _;

    let mut app = ui_test_app();
    app.editor
        .add_rectangle(Point2::new(10.0, 10.0), Point2::new(30.0, 25.0));
    let mut harness = ui_harness(app);
    harness.run();
    let grid = harness.state().editor.canvas.grid_visible;
    harness.get_by_label("Grid").click();
    harness.run();
    assert_ne!(harness.state().editor.canvas.grid_visible, grid);
    harness.get_by_label("Snap").click();
    harness.run();
    assert!(harness.state().editor.snap_enabled);
    harness.get_by_label_contains("Undo").click();
    harness.run();
    assert_eq!(harness.state().editor.document.objects.len(), 0);
    harness.get_by_label_contains("Redo").click();
    harness.run();
    assert_eq!(harness.state().editor.document.objects.len(), 1);
}

#[test]
fn the_export_dialog_and_the_command_palette_open() {
    use egui_kittest::kittest::Queryable as _;

    let mut harness = ui_harness(ui_test_app());
    harness.run();
    harness.get_by_label("View").click();
    harness.run();
    harness.get_by_label("Command palette...").click();
    harness.run();
    assert!(
        harness.state().palette_open,
        "the View menu did not open the command palette"
    );
    harness.key_press(egui::Key::Escape);
    harness.run();
    assert!(
        !harness.state().palette_open,
        "Escape did not close the command palette"
    );
    harness.get_by_label("Export G-code...").click();
    harness.run();
    assert!(
        harness.query_by_label("File name:").is_some(),
        "the export dialog did not open"
    );
    harness.get_by_label("Cancel").click();
    harness.run();
    assert!(
        harness.query_by_label("File name:").is_none(),
        "the export dialog did not close"
    );
}

#[test]
fn the_layer_buttons_add_and_remove_a_layer() {
    use egui_kittest::kittest::Queryable as _;

    let mut harness = ui_harness(ui_test_app());
    harness.run();
    let start = harness.state().editor.document.layers.len();
    harness.get_by_label("+ Layer").click();
    harness.run();
    assert_eq!(
        harness.state().editor.document.layers.len(),
        start + 1,
        "+ Layer did not add a layer"
    );
    let added = harness.state().editor.active_layer_id.clone();
    assert_eq!(
        harness
            .state()
            .editor
            .document
            .layer(&added)
            .map(|layer| layer.name.clone()),
        Some(format!("Layer {}", start + 1)),
        "the new layer is not active"
    );
    harness.get_by_label("- Layer").click();
    harness.run();
    assert_eq!(
        harness.state().editor.document.layers.len(),
        start,
        "- Layer did not remove the active layer"
    );
    assert!(
        harness.state().editor.document.layer(&added).is_none(),
        "the removed layer is still present"
    );
}

#[test]
fn a_layer_mode_assignment_reaches_every_object() {
    use egui_kittest::kittest::Queryable as _;

    let mut app = ui_test_app();
    let layer = app.editor.active_layer_id.clone();
    app.editor
        .add_rectangle(Point2::new(10.0, 10.0), Point2::new(30.0, 25.0));
    let mut harness = ui_harness(app);
    harness.run();
    let combo = harness
        .get_all_by_role(egui::accesskit::Role::ComboBox)
        .next()
        .expect("the layer mode combo is missing");
    combo.click();
    harness.run();
    harness
        .get_by_role_and_label(egui::accesskit::Role::Button, "Fill")
        .click();
    harness.run();
    let document = &harness.state().editor.document;
    let operation = document
        .operations
        .iter()
        .find(|operation| operation.op_type == crate::operations::OperationType::Fill)
        .expect("the mode did not create a cut entry");
    assert!(
        document
            .objects_on_layer(&layer)
            .iter()
            .all(|object| object.operation_id.as_deref() == Some(operation.id.as_str())),
        "the mode was not assigned to the layer objects"
    );
    assert_eq!(
        document.layer(&layer).unwrap().operation_id.as_deref(),
        Some(operation.id.as_str()),
        "the layer did not keep its cut entry"
    );
}

#[test]
fn the_layer_switches_change_the_layer_and_the_mode() {
    use egui_kittest::kittest::Queryable as _;

    let mut app = ui_test_app();
    let layer = app.editor.active_layer_id.clone();
    app.editor
        .add_rectangle(Point2::new(10.0, 10.0), Point2::new(30.0, 25.0));
    let operation = app.editor.add_operation(
        "Mode 1".to_string(),
        crate::operations::OperationType::OutlineCutting,
    );
    app.editor
        .assign_operation_to_layer(&layer, Some(operation.as_str()));
    let mut harness = ui_harness(app);
    harness.run();
    harness
        .get_by_role_and_label(egui::accesskit::Role::CheckBox, "Air")
        .click_accesskit();
    harness.run();
    assert!(
        harness
            .state()
            .editor
            .document
            .operations
            .iter()
            .find(|entry| entry.id == operation)
            .expect("the operation is missing")
            .params
            .air_assist,
        "Air did not enable air assist"
    );
    harness
        .get_by_role_and_label(egui::accesskit::Role::CheckBox, "Show")
        .click_accesskit();
    harness.run();
    assert!(
        !harness
            .state()
            .editor
            .document
            .layer(&layer)
            .expect("the layer is missing")
            .visible,
        "Show did not hide the layer"
    );
    harness
        .get_by_role_and_label(egui::accesskit::Role::CheckBox, "Output")
        .click();
    harness.run();
    assert!(
        !harness
            .state()
            .editor
            .document
            .operations
            .iter()
            .find(|entry| entry.id == operation)
            .expect("the operation is missing")
            .enabled,
        "Output did not disable the mode"
    );
}

#[test]
fn the_preferences_window_switches_sections() {
    use egui_kittest::kittest::Queryable as _;

    let mut harness = ui_harness(ui_test_app());
    harness.run();
    harness.get_by_label("View").click();
    harness.run();
    harness.get_by_label("Preferences...").click();
    harness.run();
    assert!(
        harness.state().preferences_open,
        "the preferences window did not open"
    );
    for (label, section) in [
        ("Interface", crate::config::preferences::Section::Interface),
        (
            "Drawing and snapping",
            crate::config::preferences::Section::Drawing,
        ),
        (
            "Files and backup",
            crate::config::preferences::Section::Files,
        ),
        ("Shortcuts", crate::config::preferences::Section::Shortcuts),
    ] {
        harness.get_by_label(label).click_accesskit();
        harness.run();
        assert!(
            harness.state().preferences_section == section,
            "the {label} section was not selected"
        );
    }
}

#[test]
fn save_as_suggests_the_project_extension_and_can_be_cancelled() {
    use egui_kittest::kittest::Queryable as _;

    let mut harness = ui_harness(ui_test_app());
    harness.run();
    harness.get_by_label("File").click();
    harness.run();
    harness.get_by_label("Save as...").click();
    harness.run();
    let dialog = harness
        .state()
        .dialog
        .as_ref()
        .expect("the save dialog did not open");
    assert_eq!(dialog.mode, FileMode::SaveProject);
    assert!(
        std::path::Path::new(&dialog.file_name)
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("bwp")),
        "the suggested name is not a project: {}",
        dialog.file_name
    );
    harness.get_by_label("Cancel").click();
    harness.run();
    assert!(
        harness.state().dialog.is_none(),
        "Cancel did not close the dialog"
    );
}

#[test]
fn a_new_layer_gets_a_colour_and_a_cut_entry() {
    use egui_kittest::kittest::Queryable as _;

    let mut harness = ui_harness(ui_test_app());
    harness.run();
    harness.get_by_label("+ Layer").click();
    harness.run();
    let document = &harness.state().editor.document;
    assert_eq!(document.layers.len(), 2, "+ Layer did not add a layer");
    let added = &document.layers[1];
    assert_eq!(
        added.color, "#1e88e5",
        "the new layer reused another colour"
    );
    let operation = document
        .operations
        .iter()
        .find(|operation| operation.params.color == "#1e88e5")
        .expect("the new layer has no cut entry");
    assert_eq!(
        added.operation_id.as_deref(),
        Some(operation.id.as_str()),
        "the new layer is not bound to its cut entry"
    );
    assert_eq!(operation.name, "C00");
}

#[test]
fn the_row_menu_opens_from_the_speeds_cell() {
    use egui_kittest::kittest::Queryable as _;

    let mut app = ui_test_app();
    app.editor
        .add_rectangle(Point2::new(10.0, 10.0), Point2::new(30.0, 25.0));
    let mut harness = ui_harness(app);
    harness.run();
    harness
        .get_by_role_and_label(egui::accesskit::Role::Button, "Drawing colour #e53935")
        .click_accesskit();
    harness.run();
    harness
        .get_all_by_label("1000 / 20")
        .next()
        .expect("the speeds cell is missing")
        .click_secondary();
    harness.run();
    assert!(
        harness.query_by_label("Cut settings").is_some(),
        "the row menu did not open from the speeds cell"
    );
}

#[test]
fn deleting_a_layer_removes_its_unused_cut_entry() {
    use egui_kittest::kittest::Queryable as _;

    let mut harness = ui_harness(ui_test_app());
    harness.run();
    harness.get_by_label("+ Layer").click();
    harness.run();
    assert_eq!(harness.state().editor.document.operations.len(), 1);
    harness.get_by_label("- Layer").click();
    harness.run();
    assert_eq!(
        harness.state().editor.document.layers.len(),
        1,
        "the layer was not deleted"
    );
    assert!(
        harness.state().editor.document.operations.is_empty(),
        "the orphan cut entry survived the layer"
    );
}

#[test]
fn a_cut_entry_used_by_another_layer_survives() {
    use egui_kittest::kittest::Queryable as _;

    let mut app = ui_test_app();
    let first = app.editor.active_layer_id.clone();
    let second = app.editor.document.add_layer("Layer 2").id.clone();
    app.editor.active_layer_id = second.clone();
    let operation = app.editor.ensure_operation_for_colour("#1e88e5");
    for layer in [&first, &second] {
        app.editor.document.layer_mut(layer).unwrap().operation_id = Some(operation.clone());
    }
    let mut harness = ui_harness(app);
    harness.run();
    harness.get_by_label("- Layer").click();
    harness.run();
    assert!(
        harness.state().editor.document.layer(&first).is_some(),
        "the wrong layer was deleted"
    );
    assert_eq!(
        harness.state().editor.document.operations.len(),
        1,
        "the shared cut entry was deleted"
    );
}

#[test]
fn the_cut_switches_wait_for_a_cut_entry() {
    use egui_kittest::kittest::{NodeT as _, Queryable as _};

    let mut harness = ui_harness(ui_test_app());
    harness.run();
    let air = harness.get_by_role_and_label(egui::accesskit::Role::CheckBox, "Air");
    assert!(
        air.accesskit_node().is_disabled(),
        "Air is clickable without a cut entry"
    );
    let output = harness.get_by_role_and_label(egui::accesskit::Role::CheckBox, "Output");
    assert!(
        output.accesskit_node().is_disabled(),
        "Output is clickable without a cut entry"
    );

    harness
        .get_by_role_and_label(egui::accesskit::Role::Button, "Drawing colour #1e88e5")
        .click_accesskit();
    harness.run();
    let air = harness.get_by_role_and_label(egui::accesskit::Role::CheckBox, "Air");
    assert!(
        !air.accesskit_node().is_disabled(),
        "Air stayed disabled after the cut entry was created"
    );
}

#[test]
fn the_row_menu_selects_every_object_of_its_colour() {
    use egui_kittest::kittest::Queryable as _;

    let mut app = ui_test_app();
    app.editor
        .add_rectangle(Point2::new(10.0, 10.0), Point2::new(20.0, 20.0));
    app.editor
        .add_rectangle(Point2::new(30.0, 10.0), Point2::new(40.0, 20.0));
    let layer = app.editor.active_layer_id.clone();
    for object in &mut app.editor.document.objects {
        object.color = Some("#e53935".to_string());
    }
    app.editor.document.layer_mut(&layer).unwrap().color = "#e53935".to_string();
    app.editor.selection.clear();
    let mut harness = ui_harness(app);
    harness.run();
    harness.get_by_label("Layer 1").click_secondary();
    harness.run();
    harness.get_by_label("Select this colour").click_accesskit();
    harness.run();
    assert_eq!(
        harness.state().editor.selection.len(),
        2,
        "select this colour missed an object"
    );
}

#[test]
fn the_row_menu_adds_and_edits_the_cut_entry() {
    use egui_kittest::kittest::Queryable as _;

    let app = ui_test_app();
    let layer = app.editor.active_layer_id.clone();
    let mut harness = ui_harness(app);
    harness.run();
    harness.get_by_label("Layer 1").click_secondary();
    harness.run();
    assert!(
        harness.query_by_label("Layer Layer 1").is_none(),
        "the row menu repeats the Layer prefix"
    );
    assert!(
        harness.query_by_label("Add cut entry").is_some(),
        "the row menu has no cut entry action"
    );
    harness.get_by_label("Add cut entry").click_accesskit();
    harness.run();
    harness.get_by_label("OK").click_accesskit();
    harness.run();
    assert_eq!(
        harness.state().editor.document.operations.len(),
        1,
        "the row menu did not create the cut entry"
    );
    assert!(
        harness
            .state()
            .editor
            .document
            .layer(&layer)
            .expect("the layer is missing")
            .operation_id
            .is_some(),
        "the layer did not keep its cut entry"
    );

    harness.get_by_label("Layer 1").click_secondary();
    harness.run();
    for label in ["Speed (mm/min)", "Power (%)", "Passes", "DPI"] {
        assert!(
            harness.query_by_label(label).is_some(),
            "the row menu is missing {label}"
        );
    }
    harness.get_by_label("Cancel").click_accesskit();
    harness.run();
    assert!(
        harness.query_by_label("Speed (mm/min)").is_none(),
        "Cancel did not close the row settings"
    );
}

#[test]
fn the_row_settings_cancel_and_ok_control_the_edit() {
    use egui_kittest::kittest::Queryable as _;

    let mut app = ui_test_app();
    app.editor
        .add_rectangle(Point2::new(10.0, 10.0), Point2::new(30.0, 25.0));
    let mut harness = ui_harness(app);
    harness.run();
    harness
        .get_by_role_and_label(egui::accesskit::Role::Button, "Drawing colour #e53935")
        .click_accesskit();
    harness.run();

    harness.get_by_label("Layer 1").click_secondary();
    harness.run();
    harness
        .get_all_by_role(egui::accesskit::Role::ComboBox)
        .last()
        .expect("the dialog mode combo is missing")
        .click();
    harness.run();
    harness
        .get_by_role_and_label(egui::accesskit::Role::Button, "Fill")
        .click();
    harness.run();
    harness.get_by_label("Cancel").click_accesskit();
    harness.run();
    let red = |harness: &egui_kittest::Harness<'static, BeamBurinApp>| {
        harness
            .state()
            .editor
            .document
            .operations
            .iter()
            .find(|entry| entry.params.color == "#e53935")
            .map(|entry| entry.op_type)
    };
    assert_eq!(
        red(&harness),
        Some(crate::operations::OperationType::OutlineCutting),
        "Cancel kept the edit"
    );

    harness.get_by_label("Layer 1").click_secondary();
    harness.run();
    harness
        .get_all_by_role(egui::accesskit::Role::ComboBox)
        .last()
        .expect("the dialog mode combo is missing")
        .click();
    harness.run();
    harness
        .get_by_role_and_label(egui::accesskit::Role::Button, "Fill")
        .click();
    harness.run();
    harness.get_by_label("OK").click_accesskit();
    harness.run();
    assert_eq!(
        red(&harness),
        Some(crate::operations::OperationType::Fill),
        "OK dropped the edit"
    );
    assert!(harness.state_mut().editor.undo().is_some());
    assert_eq!(
        red(&harness),
        Some(crate::operations::OperationType::OutlineCutting),
        "the dialog edit is not undoable"
    );
}

#[test]
fn drawing_after_deleting_the_cut_entry_creates_it_again() {
    use egui_kittest::kittest::Queryable as _;

    let mut app = ui_test_app();
    let layer = app.editor.active_layer_id.clone();
    app.editor
        .add_rectangle(Point2::new(10.0, 10.0), Point2::new(30.0, 25.0));
    let mut harness = ui_harness(app);
    harness.run();
    harness.get_by_label("Layer 1").click_secondary();
    harness.run();
    harness.get_by_label("Delete cut entry").click_accesskit();
    harness.run();
    assert!(
        harness.state().editor.document.operations.is_empty(),
        "the cut entry was not deleted"
    );

    harness
        .state_mut()
        .editor
        .add_rectangle(Point2::new(40.0, 10.0), Point2::new(60.0, 25.0));
    harness.run();
    let document = &harness.state().editor.document;
    assert_eq!(
        document.operations.len(),
        1,
        "drawing did not create the cut entry"
    );
    let entry = &document.operations[0];
    assert_eq!(
        entry.op_type,
        crate::operations::OperationType::OutlineCutting,
        "the new entry does not use the default mode"
    );
    assert_eq!(entry.params.speed, 1000.0);
    assert_eq!(entry.params.power, 20.0);
    assert_eq!(
        document.layer(&layer).unwrap().operation_id.as_deref(),
        Some(entry.id.as_str()),
        "the layer was not bound to the new entry"
    );
    assert!(
        document
            .objects
            .iter()
            .all(|object| object.operation_id.as_deref() == Some(entry.id.as_str())),
        "an object was left without a cut entry"
    );
}

#[test]
fn enter_confirms_the_row_settings() {
    use egui_kittest::kittest::Queryable as _;

    let mut harness = ui_harness(ui_test_app());
    harness.run();
    harness.get_by_label("Layer 1").click_secondary();
    harness.run();
    assert!(harness.query_by_label("Cancel").is_some());
    harness.key_press(egui::Key::Enter);
    harness.run();
    assert!(
        harness.query_by_label("Cancel").is_none(),
        "Enter did not confirm the dialog"
    );
}

#[test]
fn the_row_settings_warns_about_a_shared_cut_entry() {
    use egui_kittest::kittest::Queryable as _;

    let mut app = ui_test_app();
    let first = app.editor.active_layer_id.clone();
    let second = app.editor.document.add_layer("Layer 2").id.clone();
    let operation = app.editor.ensure_operation_for_colour("#000000");
    for layer in [&first, &second] {
        app.editor.document.layer_mut(layer).unwrap().operation_id = Some(operation.clone());
    }
    let mut harness = ui_harness(app);
    harness.run();
    harness.get_by_label("Layer 1").click_secondary();
    harness.run();
    assert!(
        harness
            .query_by_label_contains("Shared with Layer 2")
            .is_some(),
        "the dialog does not say the entry is shared"
    );
}

#[test]
fn the_palette_marks_a_custom_colour() {
    use egui_kittest::kittest::Queryable as _;

    let mut app = ui_test_app();
    let layer = app.editor.active_layer_id.clone();
    app.editor.document.layer_mut(&layer).unwrap().color = "#123456".to_string();
    let mut harness = ui_harness(app);
    harness.run();
    assert!(
        harness.query_by_label("Custom colour #123456").is_some(),
        "the palette does not mark the custom colour"
    );
}

#[test]
fn the_row_settings_delete_layer_is_undoable() {
    use egui_kittest::kittest::Queryable as _;

    let mut app = ui_test_app();
    app.editor.document.add_layer("Layer 2");
    let second = app.editor.document.layers[1].id.clone();
    app.editor.active_layer_id = second;
    let mut harness = ui_harness(app);
    harness.run();
    harness.get_by_label("Layer 2").click_secondary();
    harness.run();
    harness.get_by_label("Delete layer").click_accesskit();
    harness.run();
    assert_eq!(
        harness.state().editor.document.layers.len(),
        1,
        "the layer was not deleted"
    );
    assert!(
        harness.state_mut().editor.undo().is_some(),
        "the dialog delete is not undoable"
    );
    assert_eq!(
        harness.state().editor.document.layers.len(),
        2,
        "undo did not restore the layer"
    );
}

#[test]
fn the_row_menu_opens_the_full_cut_settings() {
    use egui_kittest::kittest::Queryable as _;

    let mut app = ui_test_app();
    app.editor
        .add_rectangle(Point2::new(10.0, 10.0), Point2::new(30.0, 25.0));
    let mut harness = ui_harness(app);
    harness.run();
    harness
        .get_by_role_and_label(egui::accesskit::Role::Button, "Drawing colour #e53935")
        .click_accesskit();
    harness.run();
    harness.get_by_label("Layer 1").click_secondary();
    harness.run();
    harness.get_by_label("All settings...").click_accesskit();
    harness.run();
    assert!(
        harness.state().cuts_open,
        "the full cut settings did not open"
    );
    assert!(
        harness.query_by_label("Cut settings").is_some(),
        "the Cuts window has no settings"
    );
}

#[test]
fn the_preview_window_opens_with_playback_controls() {
    use egui_kittest::kittest::Queryable as _;

    let mut harness = ui_harness(ui_test_app());
    harness.run();
    harness.get_by_label("Operations").click();
    harness.run();
    harness.get_by_label("Preview and G-code...").click();
    harness.run();
    assert!(
        harness.state().preview_open,
        "the preview window did not open"
    );
    for label in ["Play", "Stop", "Refresh", "Speed"] {
        assert!(
            harness.query_by_label(label).is_some(),
            "the preview is missing {label}"
        );
    }
}

#[test]
fn exporting_gcode_from_the_dialog_writes_the_file() {
    use egui_kittest::kittest::Queryable as _;

    let directory = ui_temp_path("export_dir");
    std::fs::create_dir_all(&directory).expect("cannot create the temporary directory");
    let mut app = ui_test_app();
    app.preferences.general.default_directory = directory.display().to_string();
    let layer = app.editor.active_layer_id.clone();
    app.editor
        .add_rectangle(Point2::new(5.0, 5.0), Point2::new(25.0, 20.0));
    let operation = app.editor.add_operation(
        "Cut".to_string(),
        crate::operations::OperationType::OutlineCutting,
    );
    app.editor
        .assign_operation_to_layer(&layer, Some(operation.as_str()));
    let mut harness = ui_harness(app);
    harness.run();
    harness.get_by_label("Export G-code...").click();
    harness.run();
    harness.get_by_label("Export").click();
    harness.step();
    wait_for_task(&mut harness);
    let path = directory.join("untitled.nc");
    assert!(
        path.is_file(),
        "the export did not write {}",
        path.display()
    );
    let text = std::fs::read_to_string(&path).expect("cannot read the exported file");
    assert!(text.contains("G21"), "the export is missing G21");
    assert!(
        harness.state().status.contains("Export G-code"),
        "the export was not reported: {}",
        harness.state().status
    );
}

#[test]
fn importing_an_svg_from_the_dialog_loads_its_objects() {
    use egui_kittest::kittest::Queryable as _;

    let assets = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/examples");
    let directory = ui_temp_path("import_dir");
    std::fs::create_dir_all(&directory).expect("cannot create the temporary directory");
    std::fs::copy(
        assets.join("corpus_shapes.svg"),
        directory.join("drawing.svg"),
    )
    .expect("cannot copy the SVG fixture");
    let mut app = ui_test_app();
    app.preferences.general.default_directory = directory.display().to_string();
    let mut harness = ui_harness(app);
    harness.run();
    harness.get_by_label("File").click();
    harness.run();
    harness.get_by_label("Import SVG or DXF...").click();
    harness.run();
    harness
        .get_all_by_label("Open")
        .find(|node| node.rect().min.y > 90.0)
        .expect("the dialog Open button is missing")
        .click();
    harness.step();
    wait_for_task(&mut harness);
    assert!(
        !harness.state().editor.document.objects.is_empty(),
        "the import produced no objects"
    );
}

#[test]
fn single_key_tool_shortcuts_change_the_tool() {
    let mut harness = ui_harness(ui_test_app());
    harness.run();
    for (key, tool) in [
        (egui::Key::R, crate::editor::Tool::Rect),
        (egui::Key::E, crate::editor::Tool::Ellipse),
        (egui::Key::L, crate::editor::Tool::Line),
        (egui::Key::V, crate::editor::Tool::Select),
    ] {
        harness.key_press(key);
        harness.run();
        assert_eq!(
            harness.state().editor.tool,
            tool,
            "{key:?} did not select {tool:?}"
        );
    }
}

#[test]
fn delete_removes_the_selected_object() {
    let mut app = ui_test_app();
    app.editor
        .add_rectangle(Point2::new(10.0, 10.0), Point2::new(30.0, 25.0));
    let mut harness = ui_harness(app);
    harness.run();
    assert_eq!(harness.state().editor.document.objects.len(), 1);
    harness.key_press(egui::Key::Delete);
    harness.run();
    assert_eq!(
        harness.state().editor.document.objects.len(),
        0,
        "Delete did not remove the selection"
    );
}

#[test]
fn the_command_palette_runs_the_first_match() {
    use egui_kittest::kittest::Queryable as _;

    let mut harness = ui_harness(ui_test_app());
    harness.run();
    harness.get_by_label("View").click();
    harness.run();
    harness.get_by_label("Command palette...").click();
    harness.run();
    let before = harness.state().editor.canvas.grid_visible;
    harness
        .get_all_by_role(egui::accesskit::Role::TextInput)
        .last()
        .expect("the palette entry field is missing")
        .type_text("toggle grid");
    harness.run();
    harness.key_press(egui::Key::Enter);
    harness.run();
    assert_ne!(
        harness.state().editor.canvas.grid_visible,
        before,
        "the palette did not run the grid command"
    );
}

#[test]
fn the_palette_closes_the_selected_open_path() {
    use egui_kittest::kittest::Queryable as _;

    let mut app = ui_test_app();
    let layer = app.editor.document.layers[0].id.clone();
    let open = crate::geometry::Path::from_points(&[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0)], false);
    let id = {
        let object = crate::document::Object::new(layer, crate::document::Shape::Path(open));
        let id = object.id.clone();
        app.editor.document.add_object(object);
        id
    };
    app.editor.selection.clear();
    app.editor.selection.add(id.clone());
    app.palette_open = true;
    app.palette_focus = true;
    app.palette_query.clear();
    let mut harness = ui_harness(app);
    harness.run();
    assert_eq!(harness.state().editor.selection.len(), 1);
    harness
        .get_all_by_role(egui::accesskit::Role::TextInput)
        .last()
        .expect("the palette entry field is missing")
        .type_text("close paths");
    harness.run();
    assert_eq!(
        harness.state().editor.selection.len(),
        1,
        "the selection was lost while typing: {}",
        harness.state().status
    );
    harness.key_press(egui::Key::Enter);
    harness.run();
    let app = harness.state();
    let object = app
        .editor
        .document
        .object(&id)
        .expect("the path disappeared");
    assert!(
        object.shape.to_path().is_closed(),
        "the palette did not close the path: {}",
        app.status
    );
}

#[test]
fn opening_the_palette_keeps_the_selection() {
    use egui_kittest::kittest::Queryable as _;

    let mut app = ui_test_app();
    let id = app
        .editor
        .add_rectangle(Point2::new(10.0, 10.0), Point2::new(30.0, 25.0))
        .expect("the rectangle was not added");
    app.editor.selection.clear();
    app.editor.selection.add(id);
    let mut harness = ui_harness(app);
    harness.run();
    harness.get_by_label("View").click();
    harness.run();
    harness.get_by_label("Command palette...").click();
    harness.run();
    assert!(harness.state().palette_open, "the palette did not open");
    assert_eq!(
        harness.state().editor.selection.len(),
        1,
        "opening the palette cleared the selection: {}",
        harness.state().status
    );
}

#[test]
fn the_image_properties_can_reload_the_file() {
    use egui_kittest::kittest::Queryable as _;

    let directory = ui_temp_path("reload_image_dir");
    std::fs::create_dir_all(&directory).expect("cannot create the temporary directory");
    let path = directory.join("photo.png");
    image::RgbaImage::from_pixel(8, 8, image::Rgba([30, 120, 220, 255]))
        .save(&path)
        .expect("cannot write the PNG fixture");
    let document = crate::import::raster::import_raster(&path).expect("cannot import the PNG");
    let id = document.objects[0].id.clone();
    let mut app = ui_test_app();
    app.editor.set_document(document);
    app.editor.selection.clear();
    app.editor.selection.add(id);
    let mut harness = ui_harness(app);
    harness.run();
    harness.get_by_label("Reload").click_accesskit();
    harness.run();
    assert!(
        harness.state().status.contains("Reloaded the image"),
        "the reload did not run: {}",
        harness.state().status
    );
}

#[test]
fn the_toolbar_opens_the_image_import_dialog() {
    use egui_kittest::kittest::Queryable as _;

    let mut harness = ui_harness(ui_test_app());
    harness.run();
    harness.get_by_label("Import image...").click_accesskit();
    harness.run();
    let app = harness.state();
    let dialog = app.dialog.as_ref().expect("the dialog did not open");
    assert_eq!(dialog.mode, FileMode::ImportImage);
    assert!(
        harness.query_by_label_contains("PNG").is_some(),
        "the image filter hint is missing"
    );
}

#[test]
fn the_draw_menu_inserts_a_barcode() {
    use egui_kittest::kittest::Queryable as _;

    let mut harness = ui_harness(ui_test_app());
    harness.run();
    harness.get_by_label("Draw").click_accesskit();
    harness.run();
    harness.get_by_label("Barcode...").click_accesskit();
    harness.run();
    assert!(
        harness.state().barcode_open,
        "the barcode window did not open"
    );
    harness.get_by_label("Insert").click_accesskit();
    harness.run();
    assert!(
        harness.state().editor.document.objects.len() > 10,
        "the barcode did not create bars"
    );
    assert!(
        !harness.state().barcode_open,
        "the barcode window stayed open"
    );
    assert!(matches!(
        harness.state().editor.document.operations.as_slice(),
        [_, ..]
    ));
    assert!(
        harness
            .state()
            .editor
            .document
            .objects
            .iter()
            .any(|object| object.name == "Barcode text"),
        "the human-readable label is missing"
    );
}

#[test]
fn a_broken_svg_is_rejected_without_touching_the_document() {
    use egui_kittest::kittest::Queryable as _;

    let directory = ui_temp_path("bad_import_dir");
    std::fs::create_dir_all(&directory).expect("cannot create the temporary directory");
    std::fs::write(directory.join("drawing.svg"), "<svg><g>")
        .expect("cannot write the broken fixture");
    let mut app = ui_test_app();
    app.preferences.general.default_directory = directory.display().to_string();
    let mut harness = ui_harness(app);
    harness.run();
    harness.get_by_label("File").click();
    harness.run();
    harness.get_by_label("Import SVG or DXF...").click();
    harness.run();
    harness
        .get_all_by_label("Open")
        .find(|node| node.rect().min.y > 90.0)
        .expect("the dialog Open button is missing")
        .click();
    harness.step();
    wait_for_task(&mut harness);
    assert!(
        harness.state().editor.document.objects.is_empty(),
        "the broken SVG added objects"
    );
    assert!(
        harness.state().status.contains("Import failed"),
        "the failure was not reported: {}",
        harness.state().status
    );
}

#[test]
fn the_property_plus_moves_the_object() {
    use egui_kittest::kittest::Queryable as _;

    let mut app = ui_test_app();
    app.editor
        .add_rectangle(Point2::new(10.0, 10.0), Point2::new(30.0, 25.0));
    let mut harness = ui_harness(app);
    harness.run();
    let before = harness.state().editor.document.objects[0].transform.tx;
    harness
        .get_all_by_label("+")
        .find(|node| node.rect().min.x > 700.0)
        .expect("the first property plus control is missing")
        .click_accesskit();
    harness.run();
    let after = harness.state().editor.document.objects[0].transform.tx;
    assert!(
        (after - before - 1.0).abs() < 0.001,
        "the X plus moved the object by {}",
        after - before
    );
}

#[test]
fn preview_play_and_stop_change_the_playback_state() {
    use egui_kittest::kittest::Queryable as _;

    let mut harness = ui_harness(ui_test_app());
    harness.run();
    harness.get_by_label("Operations").click();
    harness.run();
    harness.get_by_label("Preview and G-code...").click();
    harness.run();
    harness.get_by_label("Play").click();
    harness.run();
    assert!(
        harness.state().playback_playing,
        "Play did not start the playback"
    );
    harness.get_by_label("Pause").click_accesskit();
    harness.run();
    assert!(
        !harness.state().playback_playing,
        "Pause did not stop the playback"
    );
    harness.get_by_label("Play").click_accesskit();
    harness.run();
    harness.get_by_label("Stop").click();
    harness.run();
    assert!(
        !harness.state().playback_playing,
        "Stop did not end the playback"
    );
    assert_eq!(harness.state().playback_progress, 0.0);
}

#[test]
fn a_preference_checkbox_updates_the_preferences() {
    use egui_kittest::kittest::Queryable as _;

    let mut harness = ui_harness(ui_test_app());
    harness.run();
    harness.get_by_label("View").click();
    harness.run();
    harness.get_by_label("Preferences...").click();
    harness.run();
    assert!(
        harness.state().preferences.general.confirm_destructive,
        "confirmations should start enabled"
    );
    harness
        .get_by_label("Confirm before discarding unsaved changes")
        .click_accesskit();
    harness.run();
    assert!(
        !harness.state().preferences.general.confirm_destructive,
        "the checkbox did not change the preference"
    );
    let path = harness.state().preferences_path.clone();
    assert!(path.is_file(), "the preferences were not written to disk");
    let saved = crate::config::preferences::AppPreferences::load_or_default(&path)
        .expect("cannot read the saved preferences");
    assert!(
        !saved.general.confirm_destructive,
        "the saved preferences kept the old value"
    );
}

#[test]
fn the_cuts_window_lists_the_output_options() {
    use egui_kittest::kittest::Queryable as _;

    let mut harness = ui_harness(ui_test_app());
    harness.run();
    harness.get_by_label("Operations").click();
    harness.run();
    harness.get_by_label("Cuts / Layers...").click();
    harness.run();
    assert!(harness.state().cuts_open, "the cuts window did not open");
    for label in [
        "G-code dialect",
        "Decimals",
        "Arcs (G2/G3)",
        "CRLF line endings",
    ] {
        assert!(
            harness.query_by_label(label).is_some(),
            "the cuts window is missing {label}"
        );
    }
}

#[test]
fn the_cuts_window_sets_the_job_origin() {
    use egui_kittest::kittest::Queryable as _;

    let mut harness = ui_harness(ui_test_app());
    harness.run();
    harness.get_by_label("Operations").click();
    harness.run();
    harness.get_by_label("Cuts / Layers...").click();
    harness.run();
    let radios: Vec<_> = harness
        .get_all_by_role(egui::accesskit::Role::RadioButton)
        .collect();
    assert_eq!(radios.len(), 9, "the origin grid is not three by three");
    radios[4].click_accesskit();
    harness.run();
    assert_eq!(
        harness.state().gcode_origin,
        crate::gcode::JobOrigin::Centre,
        "the centre origin was not selected"
    );
}

#[test]
fn the_width_plus_resizes_the_object() {
    use egui_kittest::kittest::Queryable as _;

    let mut app = ui_test_app();
    app.editor
        .add_rectangle(Point2::new(10.0, 10.0), Point2::new(30.0, 25.0));
    let mut harness = ui_harness(app);
    harness.run();
    let before = harness
        .state()
        .editor
        .selected_bounds()
        .expect("the object is not selected");
    let before_width = before.1.x - before.0.x;
    harness
        .get_all_by_label("+")
        .filter(|node| node.rect().min.x > 700.0)
        .nth(2)
        .expect("the width plus control is missing")
        .click_accesskit();
    harness.run();
    let after = harness
        .state()
        .editor
        .selected_bounds()
        .expect("the object is not selected");
    let after_width = after.1.x - after.0.x;
    assert!(
        (after_width - before_width - 1.0).abs() < 0.01,
        "the width plus changed the width by {}",
        after_width - before_width
    );
}

#[test]
fn the_layer_menu_duplicates_a_layer_with_its_objects() {
    use egui_kittest::kittest::Queryable as _;

    let mut app = ui_test_app();
    let layer = app.editor.active_layer_id.clone();
    app.editor
        .add_rectangle(Point2::new(10.0, 10.0), Point2::new(30.0, 25.0));
    let operation = app.editor.add_operation(
        "Cut".to_string(),
        crate::operations::OperationType::OutlineCutting,
    );
    app.editor
        .assign_operation_to_layer(&layer, Some(operation.as_str()));
    let mut harness = ui_harness(app);
    harness.run();
    harness.get_by_label("Layer 1").click_secondary();
    harness.run();
    assert!(
        harness.query_by_label("Duplicate layer").is_some(),
        "the row menu did not open"
    );
    harness.get_by_label("Duplicate layer").click_accesskit();
    harness.run();
    assert_eq!(
        harness.state().editor.document.layers.len(),
        2,
        "Duplicate layer did not add a layer"
    );
    assert_eq!(
        harness.state().editor.document.objects.len(),
        2,
        "the copied layer did not copy its objects"
    );
}

#[test]
fn estimate_reports_a_job_for_the_current_document() {
    use egui_kittest::kittest::Queryable as _;

    let mut app = ui_test_app();
    let layer = app.editor.active_layer_id.clone();
    app.editor
        .add_rectangle(Point2::new(10.0, 10.0), Point2::new(30.0, 25.0));
    let operation = app.editor.add_operation(
        "Cut".to_string(),
        crate::operations::OperationType::OutlineCutting,
    );
    app.editor
        .assign_operation_to_layer(&layer, Some(operation.as_str()));
    let mut harness = ui_harness(app);
    harness.run();
    harness.get_by_label("Estimate").click_accesskit();
    harness.run();
    let job = harness
        .state()
        .job
        .as_ref()
        .expect("Estimate did not build a job");
    assert!(job.paths > 0, "the job has no paths");
    assert!(job.lines > 0, "the job has no lines");
    assert!(
        harness.query_by_label_contains("Paths:").is_some(),
        "the job report is not shown"
    );
}

#[test]
fn the_cuts_window_toggles_output_options() {
    use egui_kittest::kittest::Queryable as _;

    let mut harness = ui_harness(ui_test_app());
    harness.run();
    harness.get_by_label("Operations").click();
    harness.run();
    harness.get_by_label("Cuts / Layers...").click();
    harness.run();
    assert!(harness.state().gcode_comments);
    harness.get_by_label("Comments").click_accesskit();
    harness.run();
    assert!(!harness.state().gcode_comments, "Comments did not turn off");
    assert!(!harness.state().gcode_arcs);
    harness.get_by_label("Arcs (G2/G3)").click_accesskit();
    harness.run();
    assert!(harness.state().gcode_arcs, "Arcs (G2/G3) did not turn on");
}

#[test]
fn the_property_y_and_height_controls_resize_the_object() {
    use egui_kittest::kittest::Queryable as _;

    let mut app = ui_test_app();
    app.editor
        .add_rectangle(Point2::new(10.0, 10.0), Point2::new(30.0, 25.0));
    let mut harness = ui_harness(app);
    harness.run();
    let before = harness
        .state()
        .editor
        .selected_bounds()
        .expect("no selection");
    let before_height = before.1.y - before.0.y;
    harness
        .get_all_by_label("+")
        .filter(|node| node.rect().min.x > 700.0)
        .nth(1)
        .expect("the Y plus control is missing")
        .click_accesskit();
    harness.run();
    assert!(
        (harness.state().editor.document.objects[0].transform.ty - before.0.y - 1.0).abs() < 0.01,
        "the Y plus did not move the object down"
    );
    harness
        .get_all_by_label("+")
        .filter(|node| node.rect().min.x > 700.0)
        .nth(3)
        .expect("the height plus control is missing")
        .click_accesskit();
    harness.run();
    let after = harness
        .state()
        .editor
        .selected_bounds()
        .expect("no selection");
    let after_height = after.1.y - after.0.y;
    assert!(
        (after_height - before_height - 1.0).abs() < 0.01,
        "the height plus changed the height by {}",
        after_height - before_height
    );
}

#[test]
fn saving_over_an_existing_file_asks_before_replacing() {
    use egui_kittest::kittest::Queryable as _;

    let directory = ui_temp_path("replace_dir");
    std::fs::create_dir_all(&directory).expect("cannot create the temporary directory");
    let target = directory.join("untitled.bwp");
    std::fs::write(&target, "keep me").expect("cannot create the existing project");
    let mut app = ui_test_app();
    app.preferences.general.default_directory = directory.display().to_string();
    let mut harness = ui_harness(app);
    harness.run();
    harness.get_by_label("File").click();
    harness.run();
    harness.get_by_label("Save as...").click();
    harness.run();
    harness
        .get_all_by_label("Save")
        .find(|node| node.rect().min.y > 90.0)
        .expect("the dialog Save button is missing")
        .click_accesskit();
    harness.run();
    assert!(
        harness.state().pending_file.is_some(),
        "the replace prompt did not open"
    );
    assert!(
        harness.query_by_label("Replace").is_some(),
        "the Replace button is missing"
    );
    harness.get_by_label("Cancel").click_accesskit();
    harness.run();
    assert!(
        harness.state().pending_file.is_none(),
        "Cancel did not dismiss the replace prompt"
    );
    assert_eq!(
        std::fs::read_to_string(&target).expect("the file disappeared"),
        "keep me",
        "Cancel overwrote the existing file"
    );
}

#[test]
fn the_licenses_window_shows_the_notices() {
    use egui_kittest::kittest::Queryable as _;

    let mut harness = ui_harness(ui_test_app());
    harness.run();
    harness.get_by_label("Help").click();
    harness.run();
    harness.get_by_label("Licenses...").click();
    // The notices are a large text block; a fixed number of steps avoids
    // waiting for a canvas repaint that the modal window keeps requesting.
    harness.run_steps(3);
    assert!(
        harness.state().licenses_open,
        "the licenses window did not open"
    );
    assert!(
        harness
            .query_by_label_contains("Third-party notices")
            .is_some(),
        "the notices are not shown"
    );
    assert!(
        harness.query_by_label_contains("DejaVu Sans").is_some(),
        "the bundled font licence is missing"
    );
}

#[test]
fn a_saved_project_opens_through_the_dialog() {
    use egui_kittest::kittest::Queryable as _;

    let directory = ui_temp_path("open_dir");
    std::fs::create_dir_all(&directory).expect("cannot create the temporary directory");
    let path = directory.join("untitled.bwp");
    let document = crate::document::Document {
        name: "Loaded project".to_string(),
        ..Default::default()
    };
    crate::document::serialization::save_to_file(&document, &path)
        .expect("cannot write the project fixture");
    let mut app = ui_test_app();
    app.preferences.general.default_directory = directory.display().to_string();
    let mut harness = ui_harness(app);
    harness.run();
    harness.get_by_label("File").click();
    harness.run();
    harness.get_by_label("Open...").click();
    harness.run();
    harness
        .get_all_by_label("Open")
        .find(|node| node.rect().min.y > 90.0)
        .expect("the dialog Open button is missing")
        .click_accesskit();
    harness.step();
    wait_for_task(&mut harness);
    assert_eq!(
        harness.state().editor.document.name,
        "Loaded project",
        "the project was not loaded"
    );
    assert_eq!(
        harness.state().current_path.as_deref(),
        Some(path.as_path()),
        "the current path was not set"
    );
}

#[test]
fn the_theme_preference_switches_to_light() {
    use egui_kittest::kittest::Queryable as _;

    let mut harness = ui_harness(ui_test_app());
    harness.run();
    assert!(
        harness.state().dark_mode,
        "the default theme should be dark"
    );
    harness.get_by_label("View").click();
    harness.run();
    harness.get_by_label("Dark mode").click_accesskit();
    harness.run();
    assert!(
        !harness.state().dark_mode,
        "the Dark mode toggle did not switch the live theme"
    );
    let ctx = harness.ctx.clone();
    harness.state_mut().advance(&ctx);
    assert_eq!(
        harness.state().preferences.general.theme,
        "light",
        "the theme preference did not follow the toggle"
    );
    let path = harness.state().preferences_path.clone();
    let saved = crate::config::preferences::AppPreferences::load_or_default(&path)
        .expect("cannot read the saved preferences");
    assert_eq!(
        saved.general.theme, "light",
        "the saved preferences kept the old theme"
    );
}

#[test]
fn save_as_adds_the_project_extension() {
    use egui_kittest::kittest::Queryable as _;

    let directory = ui_temp_path("extension_dir");
    std::fs::create_dir_all(&directory).expect("cannot create the temporary directory");
    let mut app = ui_test_app();
    app.preferences.general.default_directory = directory.display().to_string();
    let mut harness = ui_harness(app);
    harness.run();
    harness.get_by_label("File").click();
    harness.run();
    harness.get_by_label("Save as...").click();
    harness.run();
    harness
        .state_mut()
        .dialog
        .as_mut()
        .expect("the save dialog did not open")
        .file_name = "plan".to_string();
    harness
        .get_all_by_label("Save")
        .find(|node| node.rect().min.y > 90.0)
        .expect("the dialog Save button is missing")
        .click_accesskit();
    harness.step();
    wait_for_task(&mut harness);
    assert!(
        directory.join("plan.bwp").is_file(),
        "the save dialog did not append .bwp"
    );
}

#[test]
fn the_import_dialog_rejects_an_unknown_extension() {
    use egui_kittest::kittest::Queryable as _;

    let directory = ui_temp_path("reject_dir");
    std::fs::create_dir_all(&directory).expect("cannot create the temporary directory");
    let mut app = ui_test_app();
    app.preferences.general.default_directory = directory.display().to_string();
    let mut harness = ui_harness(app);
    harness.run();
    harness.get_by_label("File").click();
    harness.run();
    harness.get_by_label("Import SVG or DXF...").click();
    harness.run();
    harness
        .state_mut()
        .dialog
        .as_mut()
        .expect("the import dialog did not open")
        .file_name = "notes.txt".to_string();
    harness
        .get_all_by_label("Open")
        .find(|node| node.rect().min.y > 90.0)
        .expect("the dialog Open button is missing")
        .click_accesskit();
    harness.run();
    let dialog = harness
        .state()
        .dialog
        .as_ref()
        .expect("the import dialog closed on an invalid name");
    assert!(
        dialog.error.is_some(),
        "the import dialog did not report the extension"
    );
}

#[test]
fn the_recovery_prompt_loads_the_autosaved_draft() {
    use egui_kittest::kittest::Queryable as _;

    let mut app = ui_test_app();
    let document = crate::document::Document {
        name: "Recovered draft".to_string(),
        ..Default::default()
    };
    crate::document::serialization::save_recovery(&document, &app.recovery_path)
        .expect("cannot write the recovery fixture");
    app.recovery_prompt_open = true;
    let mut harness = ui_harness(app);
    harness.run();
    harness.get_by_label("Open draft").click();
    harness.run();
    assert!(
        !harness.state().recovery_prompt_open,
        "the prompt stayed open"
    );
    assert_eq!(
        harness.state().editor.document.name,
        "Recovered draft",
        "the draft was not loaded"
    );
}

#[test]
fn exporting_svg_from_the_dialog_writes_the_file() {
    use egui_kittest::kittest::Queryable as _;

    let directory = ui_temp_path("svg_export_dir");
    std::fs::create_dir_all(&directory).expect("cannot create the temporary directory");
    let mut app = ui_test_app();
    app.preferences.general.default_directory = directory.display().to_string();
    app.editor
        .add_rectangle(Point2::new(5.0, 5.0), Point2::new(25.0, 20.0));
    let mut harness = ui_harness(app);
    harness.run();
    harness.get_by_label("File").click();
    harness.run();
    harness.get_by_label("Export SVG...").click();
    harness.run();
    assert!(
        harness.state().export_svg_open,
        "the SVG export options did not open"
    );
    harness
        .get_by_role_and_label(egui::accesskit::Role::CheckBox, "Text as paths")
        .click_accesskit();
    harness.run();
    assert!(
        harness.state().export_svg_text_paths,
        "the text-as-paths switch did not apply"
    );
    harness.get_by_label("Export").click();
    harness.run();
    harness.get_by_label("Export").click();
    harness.step();
    wait_for_task(&mut harness);
    let path = directory.join("untitled.svg");
    assert!(path.is_file(), "the SVG export did not write the file");
    let text = std::fs::read_to_string(&path).expect("cannot read the SVG");
    assert!(text.contains("<svg"), "the export is not an SVG");
}

#[test]
fn exporting_the_selection_as_svg_crops_the_viewbox() {
    use egui_kittest::kittest::Queryable as _;

    let directory = ui_temp_path("svg_selection_dir");
    std::fs::create_dir_all(&directory).expect("cannot create the temporary directory");
    let mut app = ui_test_app();
    app.preferences.general.default_directory = directory.display().to_string();
    app.editor
        .add_rectangle(Point2::new(5.0, 5.0), Point2::new(25.0, 20.0));
    let id = app.editor.document.objects[0].id.clone();
    app.editor.selection.clear();
    app.editor.selection.add(id);
    let mut harness = ui_harness(app);
    harness.run();
    harness.get_by_label("File").click();
    harness.run();
    harness.get_by_label("Export selection ⏵").click();
    harness.run();
    harness.get_by_label("As SVG...").click();
    harness.run();
    assert!(
        harness.state().export_svg_selection,
        "the selection area was not preselected"
    );
    harness.get_by_label("Export").click();
    harness.run();
    harness.get_by_label("Export").click();
    harness.step();
    wait_for_task(&mut harness);
    let path = directory.join("untitled.svg");
    let text = std::fs::read_to_string(&path).expect("cannot read the SVG");
    assert!(
        text.contains("width=\"20mm\" height=\"15mm\" viewBox=\"0 0 20 15\""),
        "the viewBox does not match the selection: {text}"
    );
}

#[test]
fn exporting_the_selection_as_pdf_writes_only_the_selected_object() {
    use egui_kittest::kittest::Queryable as _;

    let directory = ui_temp_path("pdf_selection_dir");
    std::fs::create_dir_all(&directory).expect("cannot create the temporary directory");
    let mut app = ui_test_app();
    app.preferences.general.default_directory = directory.display().to_string();
    app.editor
        .add_rectangle(Point2::new(5.0, 5.0), Point2::new(15.0, 12.0));
    app.editor
        .add_rectangle(Point2::new(30.0, 5.0), Point2::new(40.0, 12.0));
    let first = app.editor.document.objects[0].id.clone();
    app.editor.selection.clear();
    app.editor.selection.add(first);
    let mut harness = ui_harness(app);
    harness.run();
    harness.get_by_label("File").click();
    harness.run();
    harness.get_by_label("Export selection ⏵").click();
    harness.run();
    harness.get_by_label("As PDF...").click();
    harness.run();
    harness.get_by_label("Export").click();
    harness.step();
    wait_for_task(&mut harness);
    let path = directory.join("untitled.pdf");
    let text = std::fs::read_to_string(&path).expect("cannot read the PDF");
    assert!(
        text.contains("/MediaBox [0 0 28.346457 19.84252]"),
        "the PDF page should match the selection: {text}"
    );
    assert_eq!(
        text.matches("S\n").count(),
        1,
        "the PDF should contain only the selected rectangle"
    );
}

#[test]
fn exporting_dxf_from_the_dialog_writes_the_file() {
    use egui_kittest::kittest::Queryable as _;

    let directory = ui_temp_path("dxf_export_dir");
    std::fs::create_dir_all(&directory).expect("cannot create the temporary directory");
    let mut app = ui_test_app();
    app.preferences.general.default_directory = directory.display().to_string();
    app.editor
        .add_rectangle(Point2::new(5.0, 5.0), Point2::new(25.0, 20.0));
    let mut harness = ui_harness(app);
    harness.run();
    harness.get_by_label("File").click();
    harness.run();
    harness.get_by_label("Export DXF...").click();
    harness.run();
    assert!(
        harness.state().export_dxf_open,
        "the DXF export options did not open"
    );
    harness.get_by_label("Export").click();
    harness.run();
    harness.get_by_label("Export").click();
    harness.step();
    wait_for_task(&mut harness);
    let path = directory.join("untitled.dxf");
    assert!(path.is_file(), "the DXF export did not write the file");
    let text = std::fs::read_to_string(&path).expect("cannot read the DXF");
    assert!(text.contains("SECTION"), "the export is not a DXF");
    assert!(text.contains("ENTITIES"), "the DXF has no entities");
}

#[test]
fn exporting_text_as_dxf_paths_writes_polylines() {
    use egui_kittest::kittest::Queryable as _;

    let directory = ui_temp_path("dxf_text_dir");
    std::fs::create_dir_all(&directory).expect("cannot create the temporary directory");
    let mut app = ui_test_app();
    app.preferences.general.default_directory = directory.display().to_string();
    app.editor.add_text(Point2::new(10.0, 10.0));
    let mut harness = ui_harness(app);
    harness.run();
    harness.get_by_label("File").click();
    harness.run();
    harness.get_by_label("Export DXF...").click();
    harness.run();
    harness
        .get_by_role_and_label(egui::accesskit::Role::CheckBox, "Text as paths")
        .click_accesskit();
    harness.run();
    assert!(
        harness.state().export_dxf_text_paths,
        "the text-as-paths switch did not apply"
    );
    harness.get_by_label("Export").click();
    harness.run();
    harness.get_by_label("Export").click();
    harness.step();
    wait_for_task(&mut harness);
    let path = directory.join("untitled.dxf");
    let text = std::fs::read_to_string(&path).expect("cannot read the DXF");
    assert!(
        text.contains("LWPOLYLINE"),
        "the outlined text was not written: {text}"
    );
}

#[test]
fn the_dialog_can_show_unsupported_files() {
    use egui_kittest::kittest::Queryable as _;

    let directory = ui_temp_path("show_all_dir");
    std::fs::create_dir_all(&directory).expect("cannot create the temporary directory");
    std::fs::write(directory.join("notes.txt"), "not a drawing")
        .expect("cannot write the unsupported file");
    let mut app = ui_test_app();
    app.preferences.general.default_directory = directory.display().to_string();
    let mut harness = ui_harness(app);
    harness.run();
    harness.get_by_label("File").click();
    harness.run();
    harness.get_by_label("Import SVG or DXF...").click();
    harness.run();
    assert!(
        harness.query_by_label("notes.txt").is_none(),
        "an unsupported file was listed before Show all files"
    );
    harness.get_by_label("Show all files").click_accesskit();
    harness.run();
    assert!(
        harness.query_by_label("notes.txt").is_some(),
        "Show all files did not list the text file"
    );
}

#[test]
fn importing_a_png_creates_an_image_object() {
    use egui_kittest::kittest::Queryable as _;

    let directory = ui_temp_path("image_import_dir");
    std::fs::create_dir_all(&directory).expect("cannot create the temporary directory");
    let path = directory.join("drawing.png");
    image::RgbaImage::from_pixel(4, 4, image::Rgba([220, 40, 40, 255]))
        .save(&path)
        .expect("cannot write the PNG fixture");
    let mut app = ui_test_app();
    app.preferences.general.default_directory = directory.display().to_string();
    let mut harness = ui_harness(app);
    harness.run();
    harness.get_by_label("File").click();
    harness.run();
    harness.get_by_label("Import SVG or DXF...").click();
    harness.run();
    harness
        .state_mut()
        .dialog
        .as_mut()
        .expect("the import dialog did not open")
        .file_name = "drawing.png".to_string();
    harness
        .get_all_by_label("Open")
        .find(|node| node.rect().min.y > 90.0)
        .expect("the dialog Open button is missing")
        .click_accesskit();
    harness.step();
    wait_for_task(&mut harness);
    let document = &harness.state().editor.document;
    assert_eq!(document.objects.len(), 1, "the image was not imported");
    assert!(
        matches!(document.objects[0].shape, crate::document::Shape::Image(_)),
        "the imported object is not an image"
    );
}

#[test]
fn exporting_an_image_from_the_dialog_writes_a_png() {
    use egui_kittest::kittest::Queryable as _;

    let directory = ui_temp_path("png_export_dir");
    std::fs::create_dir_all(&directory).expect("cannot create the temporary directory");
    let mut app = ui_test_app();
    app.preferences.general.default_directory = directory.display().to_string();
    app.editor
        .add_rectangle(Point2::new(5.0, 5.0), Point2::new(25.0, 20.0));
    let mut harness = ui_harness(app);
    harness.run();
    harness.get_by_label("File").click();
    harness.run();
    harness.get_by_label("Export image...").click();
    harness.run();
    assert!(
        harness.state().export_image_open,
        "the image export options did not open"
    );
    harness.get_by_label("Export").click();
    harness.run();
    harness.get_by_label("Export").click();
    harness.step();
    wait_for_task(&mut harness);
    let path = directory.join("untitled.png");
    assert!(path.is_file(), "the image export did not write the file");
    let bytes = std::fs::read(&path).expect("cannot read the PNG");
    assert!(
        bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
        "the file is not a PNG"
    );
}

#[test]
fn exporting_the_selection_as_an_image_crops_to_it() {
    use egui_kittest::kittest::Queryable as _;

    let directory = ui_temp_path("png_selection_export_dir");
    std::fs::create_dir_all(&directory).expect("cannot create the temporary directory");
    let mut app = ui_test_app();
    app.preferences.general.default_directory = directory.display().to_string();
    app.editor
        .add_rectangle(Point2::new(5.0, 5.0), Point2::new(25.0, 20.0));
    let id = app.editor.document.objects[0].id.clone();
    app.editor.selection.clear();
    app.editor.selection.add(id);
    let mut harness = ui_harness(app);
    harness.run();
    harness.get_by_label("File").click();
    harness.run();
    harness.get_by_label("Export selection ⏵").click();
    harness.run();
    harness.get_by_label("As image...").click();
    harness.run();
    assert!(
        harness.state().export_image_selection,
        "the selection area was not preselected"
    );
    harness.get_by_label("Export").click();
    harness.run();
    harness.get_by_label("Export").click();
    harness.step();
    wait_for_task(&mut harness);
    let path = directory.join("untitled.png");
    assert!(path.is_file(), "the image export did not write the file");
    let (width, height) = image::image_dimensions(&path).expect("cannot read the PNG size");
    // 20 x 15 mm at 96 dpi is about 76 x 57 px, not the full 300 x 200 mm page.
    assert!(
        (75..=77).contains(&width) && (56..=58).contains(&height),
        "the PNG is {width} x {height} px instead of the selection size"
    );
}

#[test]
fn the_image_export_options_make_a_transparent_png() {
    use egui_kittest::kittest::Queryable as _;

    let directory = ui_temp_path("png_transparent_dir");
    std::fs::create_dir_all(&directory).expect("cannot create the temporary directory");
    let mut app = ui_test_app();
    app.preferences.general.default_directory = directory.display().to_string();
    app.editor
        .add_rectangle(Point2::new(5.0, 5.0), Point2::new(25.0, 20.0));
    let id = app.editor.document.objects[0].id.clone();
    app.editor.selection.clear();
    app.editor.selection.add(id);
    let mut harness = ui_harness(app);
    harness.run();
    harness.get_by_label("File").click();
    harness.run();
    harness.get_by_label("Export image...").click();
    harness.run();
    harness
        .get_by_role_and_label(egui::accesskit::Role::CheckBox, "Transparent")
        .click_accesskit();
    harness.run();
    assert!(harness.state().export_image_transparent);
    harness.get_by_label("Export").click();
    harness.run();
    harness.get_by_label("Export").click();
    harness.step();
    wait_for_task(&mut harness);
    let path = directory.join("untitled.png");
    let picture = image::open(&path).expect("cannot read the PNG").to_rgba8();
    assert_eq!(
        picture.get_pixel(0, 0)[3],
        0,
        "the background is not transparent"
    );
}

#[test]
fn the_command_palette_filters_by_query() {
    use egui_kittest::kittest::Queryable as _;

    let mut harness = ui_harness(ui_test_app());
    harness.run();
    harness.get_by_label("View").click();
    harness.run();
    harness.get_by_label("Command palette...").click();
    harness.run();
    harness
        .get_all_by_role(egui::accesskit::Role::TextInput)
        .last()
        .expect("the palette entry field is missing")
        .type_text("grid");
    harness.run();
    assert!(
        harness.query_by_label_contains("Toggle grid").is_some(),
        "the palette did not keep the grid command"
    );
}

#[test]
fn a_focused_text_field_blocks_tool_shortcuts() {
    use egui_kittest::kittest::Queryable as _;

    let mut harness = ui_harness(ui_test_app());
    harness.run();
    harness.get_by_label("View").click();
    harness.run();
    harness.get_by_label("Command palette...").click();
    harness.run();
    harness.key_press(egui::Key::R);
    harness.run();
    assert_eq!(
        harness.state().editor.tool,
        crate::editor::Tool::Select,
        "a shortcut fired while the palette field had focus"
    );
}

#[test]
fn the_cut_settings_toggle_fill_and_power_options() {
    use egui_kittest::kittest::Queryable as _;

    let mut app = ui_test_app();
    let operation = app
        .editor
        .add_operation("Cut".to_string(), crate::operations::OperationType::Fill);
    app.active_operation = Some(operation.clone());
    let mut harness = ui_harness(app);
    harness.run();
    harness.get_by_label("Operations").click();
    harness.run();
    harness.get_by_label("Cuts / Layers...").click();
    harness.run();
    assert!(
        harness
            .state()
            .editor
            .document
            .operations
            .iter()
            .find(|entry| entry.id == operation)
            .expect("the operation is missing")
            .params
            .fill_alternate
    );
    harness
        .get_by_role_and_label(egui::accesskit::Role::CheckBox, "Bi-directional fill")
        .click_accesskit();
    harness.run();
    harness
        .get_by_role_and_label(egui::accesskit::Role::CheckBox, "Cross-hatch")
        .click_accesskit();
    harness.run();
    harness
        .get_by_role_and_label(egui::accesskit::Role::CheckBox, "Constant power")
        .click_accesskit();
    harness.run();
    let params = &harness
        .state()
        .editor
        .document
        .operations
        .iter()
        .find(|entry| entry.id == operation)
        .expect("the operation is missing")
        .params;
    assert!(
        !params.fill_alternate,
        "Bi-directional fill did not turn off"
    );
    assert!(params.cross_hatch, "Cross-hatch did not turn on");
    assert!(params.constant_power, "Constant power did not turn on");
}

#[test]
fn the_image_adjust_window_edits_the_settings() {
    use egui_kittest::kittest::Queryable as _;

    let directory = ui_temp_path("adjust_dir");
    std::fs::create_dir_all(&directory).expect("cannot create the temporary directory");
    let path = directory.join("photo.png");
    image::RgbaImage::from_pixel(8, 8, image::Rgba([30, 120, 220, 255]))
        .save(&path)
        .expect("cannot write the PNG fixture");
    let document = crate::import::raster::import_raster(&path).expect("cannot import the PNG");
    let id = document
        .objects
        .first()
        .expect("the import produced no object")
        .id
        .clone();
    let mut app = ui_test_app();
    app.editor.set_document(document);
    app.image_adjust = Some(id.clone());
    let mut harness = ui_harness(app);
    harness.run();
    assert!(
        harness.query_by_label_contains("burned").is_some(),
        "the burn coverage is missing"
    );
    for label in [
        "Target DPI",
        "Image mode",
        "Line interval (mm)",
        "Contrast",
        "Brightness",
        "Negative image",
        "Auto levels",
        "Grey mix",
        "Despeckle",
        "Enhance denoise",
        "Histogram",
    ] {
        assert!(
            harness.query_all_by_label(label).next().is_some(),
            "the image window is missing {label}"
        );
    }
    harness
        .get_by_role_and_label(egui::accesskit::Role::CheckBox, "Negative image")
        .click_accesskit();
    harness.run();
    harness
        .get_by_role_and_label(egui::accesskit::Role::CheckBox, "3×3 median")
        .click_accesskit();
    harness.run();
    harness
        .get_by_role_and_label(egui::accesskit::Role::CheckBox, "Invert display")
        .click_accesskit();
    harness.run();
    let object = harness
        .state()
        .editor
        .document
        .objects
        .iter()
        .find(|object| object.id == id)
        .expect("the image disappeared");
    let crate::document::Shape::Image(image) = &object.shape else {
        panic!("the object is not an image");
    };
    assert!(image.settings.invert, "the negative switch did not apply");
    assert!(
        image.settings.despeckle,
        "the despeckle switch did not apply"
    );
    assert!(
        harness.state().image_invert_display,
        "the invert-display switch did not apply"
    );
}

#[test]
fn the_image_adjust_auto_levels_uses_the_histogram() {
    use egui_kittest::kittest::Queryable as _;

    let directory = ui_temp_path("auto_levels_dir");
    std::fs::create_dir_all(&directory).expect("cannot create the temporary directory");
    let path = directory.join("gradient.png");
    let mut picture = image::RgbaImage::new(16, 16);
    for (x, _y, pixel) in picture.enumerate_pixels_mut() {
        let value = 10 + x * 230 / 15;
        let value = u8::try_from(value).unwrap_or(255);
        *pixel = image::Rgba([value, value, value, 255]);
    }
    picture.save(&path).expect("cannot write the PNG fixture");
    let document = crate::import::raster::import_raster(&path).expect("cannot import the PNG");
    let id = document
        .objects
        .first()
        .expect("the import produced no object")
        .id
        .clone();
    let mut app = ui_test_app();
    app.editor.set_document(document);
    app.image_adjust = Some(id.clone());
    let mut harness = ui_harness(app);
    harness.run();
    harness.get_by_label("Auto levels").click_accesskit();
    harness.run();
    let object = harness
        .state()
        .editor
        .document
        .objects
        .iter()
        .find(|object| object.id == id)
        .expect("the image disappeared");
    let crate::document::Shape::Image(image) = &object.shape else {
        panic!("the object is not an image");
    };
    assert_eq!(image.settings.black_clip, 10);
    assert_eq!(image.settings.white_clip, 240);
}

#[test]
fn the_image_adjust_target_sets_the_working_size_and_interval() {
    use egui_kittest::kittest::Queryable as _;

    let directory = ui_temp_path("target_image_dir");
    std::fs::create_dir_all(&directory).expect("cannot create the temporary directory");
    let path = directory.join("photo.png");
    image::RgbaImage::from_pixel(8, 8, image::Rgba([30, 120, 220, 255]))
        .save(&path)
        .expect("cannot write the PNG fixture");
    let document = crate::import::raster::import_raster(&path).expect("cannot import the PNG");
    let id = document
        .objects
        .first()
        .expect("the import produced no object")
        .id
        .clone();
    let mut app = ui_test_app();
    app.editor.set_document(document);
    app.image_target_width_mm = 100.0;
    app.image_target_dpi = 254.0;
    app.image_adjust = Some(id.clone());
    let mut harness = ui_harness(app);
    harness.run();
    harness.get_by_label("Apply target").click_accesskit();
    harness.run();
    let object = harness
        .state()
        .editor
        .document
        .objects
        .iter()
        .find(|object| object.id == id)
        .expect("the image disappeared");
    let crate::document::Shape::Image(image) = &object.shape else {
        panic!("the object is not an image");
    };
    assert_eq!(image.settings.target_width_px, 1000);
    assert_eq!(image.settings.target_height_px, 1000);
    assert!(
        (image.settings.line_interval_mm - 0.1).abs() < 1e-5,
        "the line interval is {}",
        image.settings.line_interval_mm
    );
}

#[test]
fn the_image_adjust_window_applies_every_preset() {
    use egui_kittest::kittest::Queryable as _;

    let directory = ui_temp_path("preset_dir");
    std::fs::create_dir_all(&directory).expect("cannot create the temporary directory");
    let path = directory.join("photo.png");
    image::RgbaImage::from_pixel(8, 8, image::Rgba([30, 120, 220, 255]))
        .save(&path)
        .expect("cannot write the PNG fixture");
    let document = crate::import::raster::import_raster(&path).expect("cannot import the PNG");
    let id = document
        .objects
        .first()
        .expect("the import produced no object")
        .id
        .clone();
    let mut app = ui_test_app();
    app.editor.set_document(document);
    app.image_adjust = Some(id.clone());
    let mut harness = ui_harness(app);
    harness.run();
    for (label, mode, contrast) in [
        ("Photo", crate::objects::ImageMode::Jarvis, 1.15),
        ("Wood", crate::objects::ImageMode::Stucki, 1.25),
        ("Metal", crate::objects::ImageMode::Atkinson, 1.4),
        ("Acrylic", crate::objects::ImageMode::FloydSteinberg, 1.1),
        ("Newsprint", crate::objects::ImageMode::Newsprint, 1.2),
        ("Sketch", crate::objects::ImageMode::Sketch, 1.3),
    ] {
        harness.get_by_label(label).click_accesskit();
        harness.run();
        let object = harness
            .state()
            .editor
            .document
            .objects
            .iter()
            .find(|object| object.id == id)
            .expect("the image disappeared");
        let crate::document::Shape::Image(image) = &object.shape else {
            panic!("the object is not an image");
        };
        assert_eq!(
            image.settings.mode, mode,
            "the {label} preset did not apply"
        );
        assert!(
            (image.settings.contrast - contrast).abs() < f32::EPSILON,
            "the {label} preset kept contrast {}",
            image.settings.contrast
        );
    }
}

#[test]
fn escape_cancels_the_image_adjustments() {
    use egui_kittest::kittest::Queryable as _;

    let directory = ui_temp_path("escape_image_dir");
    std::fs::create_dir_all(&directory).expect("cannot create the temporary directory");
    let path = directory.join("photo.png");
    image::RgbaImage::from_pixel(8, 8, image::Rgba([30, 120, 220, 255]))
        .save(&path)
        .expect("cannot write the PNG fixture");
    let document = crate::import::raster::import_raster(&path).expect("cannot import the PNG");
    let id = document
        .objects
        .first()
        .expect("the import produced no object")
        .id
        .clone();
    let mut app = ui_test_app();
    app.editor.set_document(document);
    app.image_adjust_snapshot = Some(app.editor.document.clone());
    app.image_adjust = Some(id.clone());
    let mut harness = ui_harness(app);
    harness.run();
    harness
        .get_by_role_and_label(egui::accesskit::Role::CheckBox, "Negative image")
        .click_accesskit();
    harness.run();
    // The dialog applies the settings live, so the document already changed.
    let app = harness.state();
    assert!(image_is_negative(&app.editor.document, &id));
    harness.key_press(egui::Key::Escape);
    harness.run();
    let app = harness.state();
    assert!(app.image_adjust.is_none(), "the window stayed open");
    assert!(
        !image_is_negative(&app.editor.document, &id),
        "escape kept the adjustment"
    );
    assert!(app.image_adjust_snapshot.is_none());
}

#[test]
fn cancel_restores_every_image_touched_by_apply_to_selected() {
    use egui_kittest::kittest::Queryable as _;

    let directory = ui_temp_path("cancel_all_images_dir");
    std::fs::create_dir_all(&directory).expect("cannot create the temporary directory");
    let path = directory.join("photo.png");
    image::RgbaImage::from_pixel(8, 8, image::Rgba([30, 120, 220, 255]))
        .save(&path)
        .expect("cannot write the PNG fixture");
    let mut document = crate::import::raster::import_raster(&path).expect("cannot import the PNG");
    let layer = document.layers[0].id.clone();
    let second = crate::document::Object::new(layer, document.objects[0].shape.clone());
    let second_id = second.id.clone();
    document.add_object(second);
    let first_id = document.objects[0].id.clone();
    let mut app = ui_test_app();
    app.editor.set_document(document);
    app.editor.selection.add(first_id.clone());
    app.editor.selection.add(second_id.clone());
    app.image_adjust_snapshot = Some(app.editor.document.clone());
    app.image_adjust = Some(first_id.clone());
    let mut harness = ui_harness(app);
    harness.run();
    harness
        .get_by_role_and_label(egui::accesskit::Role::CheckBox, "Negative image")
        .click_accesskit();
    harness.run();
    harness
        .get_by_label("Apply to selected images")
        .click_accesskit();
    harness.run();
    let app = harness.state();
    assert!(image_is_negative(&app.editor.document, &first_id));
    assert!(
        image_is_negative(&app.editor.document, &second_id),
        "the second image was not touched"
    );
    harness.get_by_label("Cancel").click_accesskit();
    harness.run();
    let app = harness.state();
    assert!(app.image_adjust.is_none(), "the window stayed open");
    assert!(!image_is_negative(&app.editor.document, &first_id));
    assert!(
        !image_is_negative(&app.editor.document, &second_id),
        "cancel kept the settings on the second image"
    );
}

#[test]
fn the_image_preview_can_zoom() {
    use egui_kittest::kittest::Queryable as _;

    let directory = ui_temp_path("zoom_image_dir");
    std::fs::create_dir_all(&directory).expect("cannot create the temporary directory");
    let path = directory.join("photo.png");
    image::RgbaImage::from_pixel(8, 8, image::Rgba([30, 120, 220, 255]))
        .save(&path)
        .expect("cannot write the PNG fixture");
    let document = crate::import::raster::import_raster(&path).expect("cannot import the PNG");
    let id = document
        .objects
        .first()
        .expect("the import produced no object")
        .id
        .clone();
    let mut app = ui_test_app();
    app.editor.set_document(document);
    app.image_adjust = Some(id);
    let mut harness = ui_harness(app);
    harness.run();
    harness.get_by_value("Fit").click_accesskit();
    harness.run();
    harness
        .get_by_role_and_label(egui::accesskit::Role::Button, "2×")
        .click_accesskit();
    harness.run();
    assert!(
        (harness.state().image_preview_zoom - 2.0).abs() < f32::EPSILON,
        "the preview zoom did not change"
    );
}

#[test]
fn compare_with_original_shows_the_opening_settings_without_losing_the_edit() {
    use egui_kittest::kittest::Queryable as _;

    let directory = ui_temp_path("compare_image_dir");
    std::fs::create_dir_all(&directory).expect("cannot create the temporary directory");
    let path = directory.join("photo.png");
    image::RgbaImage::from_pixel(8, 8, image::Rgba([30, 120, 220, 255]))
        .save(&path)
        .expect("cannot write the PNG fixture");
    let document = crate::import::raster::import_raster(&path).expect("cannot import the PNG");
    let id = document
        .objects
        .first()
        .expect("the import produced no object")
        .id
        .clone();
    let mut app = ui_test_app();
    app.editor.set_document(document);
    app.image_adjust_snapshot = Some(app.editor.document.clone());
    app.image_adjust = Some(id.clone());
    let mut harness = ui_harness(app);
    harness.run();
    let mode_of = |app: &BeamBurinApp| {
        let object = app
            .editor
            .document
            .object(&id)
            .expect("the image disappeared");
        let crate::document::Shape::Image(image) = &object.shape else {
            panic!("the object is not an image");
        };
        image.settings.mode
    };
    harness.get_by_label("Wood").click_accesskit();
    harness.run();
    assert_eq!(mode_of(harness.state()), crate::objects::ImageMode::Stucki);

    harness
        .get_by_role_and_label(egui::accesskit::Role::CheckBox, "Compare with original")
        .click_accesskit();
    harness.run();
    assert_eq!(
        mode_of(harness.state()),
        crate::objects::ImageMode::Grayscale,
        "compare did not show the opening settings"
    );
    assert_eq!(
        harness
            .state()
            .image_adjust_edit
            .expect("the edit was lost")
            .mode,
        crate::objects::ImageMode::Stucki
    );

    harness
        .get_by_role_and_label(egui::accesskit::Role::CheckBox, "Compare with original")
        .click_accesskit();
    harness.run();
    assert_eq!(
        mode_of(harness.state()),
        crate::objects::ImageMode::Stucki,
        "the edit did not come back"
    );

    harness
        .get_by_role_and_label(egui::accesskit::Role::CheckBox, "Compare with original")
        .click_accesskit();
    harness.run();
    harness.get_by_label("OK").click_accesskit();
    harness.run();
    let app = harness.state();
    assert!(app.image_adjust.is_none(), "the window stayed open");
    assert_eq!(
        mode_of(app),
        crate::objects::ImageMode::Stucki,
        "closing while comparing lost the edit"
    );
    assert!(app.editor.history.can_undo());
}

#[test]
fn the_image_mask_can_be_cleared_and_cancelled() {
    use egui_kittest::kittest::Queryable as _;

    let directory = ui_temp_path("mask_image_dir");
    std::fs::create_dir_all(&directory).expect("cannot create the temporary directory");
    let path = directory.join("photo.png");
    image::RgbaImage::from_pixel(8, 8, image::Rgba([30, 120, 220, 255]))
        .save(&path)
        .expect("cannot write the PNG fixture");
    let mut document = crate::import::raster::import_raster(&path).expect("cannot import the PNG");
    let id = document
        .objects
        .first()
        .expect("the import produced no object")
        .id
        .clone();
    let crop = crate::objects::ImageCrop {
        x: 1,
        y: 1,
        width: 4,
        height: 4,
    };
    let crate::document::Shape::Image(image) = &mut document
        .object_mut(&id)
        .expect("the image is missing")
        .shape
    else {
        panic!("the object is not an image");
    };
    image.crop = Some(crop);
    let mut app = ui_test_app();
    app.editor.set_document(document);
    app.image_adjust_snapshot = Some(app.editor.document.clone());
    app.image_adjust = Some(id.clone());
    let mut harness = ui_harness(app);
    harness.run();
    assert!(
        harness.query_by_label("Mask").is_some(),
        "the mask controls are missing"
    );
    harness.get_by_label("Full image").click_accesskit();
    harness.run();
    let crop_of = |app: &BeamBurinApp| {
        let object = app
            .editor
            .document
            .object(&id)
            .expect("the image disappeared");
        let crate::document::Shape::Image(image) = &object.shape else {
            panic!("the object is not an image");
        };
        image.crop
    };
    assert!(
        crop_of(harness.state()).is_none(),
        "Full image kept the mask"
    );
    harness.key_press(egui::Key::Escape);
    harness.run();
    assert_eq!(
        crop_of(harness.state()),
        Some(crop),
        "Cancel did not restore the mask"
    );
}

#[test]
fn the_image_adjust_duplicates_and_deletes_a_preset() {
    use egui_kittest::kittest::Queryable as _;

    let directory = ui_temp_path("preset_copy_dir");
    std::fs::create_dir_all(&directory).expect("cannot create the temporary directory");
    let path = directory.join("photo.png");
    image::RgbaImage::from_pixel(8, 8, image::Rgba([30, 120, 220, 255]))
        .save(&path)
        .expect("cannot write the PNG fixture");
    let document = crate::import::raster::import_raster(&path).expect("cannot import the PNG");
    let id = document
        .objects
        .first()
        .expect("the import produced no object")
        .id
        .clone();
    let mut app = ui_test_app();
    app.editor.set_document(document);
    app.image_adjust = Some(id);
    app.selected_image_profile = Some("builtin-photo-wood".to_string());
    let mut harness = ui_harness(app);
    harness.run();
    let before = harness.state().profiles.images.len();
    harness.get_by_label("Duplicate preset").click_accesskit();
    harness.run();
    assert_eq!(
        harness.state().profiles.images.len(),
        before + 1,
        "the preset was not duplicated"
    );
    let copy = harness
        .state()
        .profiles
        .images
        .last()
        .expect("no profile was added");
    assert!(!copy.builtin, "the copy is still marked built-in");
    assert!(copy.name.contains("copy"), "the copy kept the old name");
    harness.get_by_label("Delete preset").click_accesskit();
    harness.run();
    assert_eq!(
        harness.state().profiles.images.len(),
        before,
        "the copied preset was not deleted"
    );
}

#[test]
fn the_image_adjust_can_save_the_current_settings_as_a_profile() {
    use egui_kittest::kittest::Queryable as _;

    let directory = ui_temp_path("save_profile_dir");
    std::fs::create_dir_all(&directory).expect("cannot create the temporary directory");
    let path = directory.join("photo.png");
    image::RgbaImage::from_pixel(8, 8, image::Rgba([30, 120, 220, 255]))
        .save(&path)
        .expect("cannot write the PNG fixture");
    let document = crate::import::raster::import_raster(&path).expect("cannot import the PNG");
    let id = document
        .objects
        .first()
        .expect("the import produced no object")
        .id
        .clone();
    let mut app = ui_test_app();
    app.editor.set_document(document);
    app.image_adjust = Some(id.clone());
    let before = app.profiles.images.len();
    let mut harness = ui_harness(app);
    harness.run();
    harness.get_by_label("Wood").click_accesskit();
    harness.run();
    harness.get_by_label("Save as profile").click_accesskit();
    harness.run();
    let app = harness.state();
    assert_eq!(
        app.profiles.images.len(),
        before + 1,
        "the settings were not saved as a profile"
    );
    let profile = app.profiles.images.last().expect("no profile was added");
    assert!(!profile.builtin);
    assert!(profile.name.starts_with("Image profile"));
    assert_eq!(profile.settings.mode, crate::objects::ImageMode::Stucki);
    assert!(
        (profile.settings.contrast - 1.25).abs() < f32::EPSILON,
        "the profile kept contrast {}",
        profile.settings.contrast
    );
    assert_eq!(app.profile_message, "Profiles saved");
}

#[test]
fn the_image_adjust_shows_the_cut_entry_and_the_estimate() {
    use egui_kittest::kittest::Queryable as _;

    let directory = ui_temp_path("cut_entry_image_dir");
    std::fs::create_dir_all(&directory).expect("cannot create the temporary directory");
    let path = directory.join("photo.png");
    image::RgbaImage::from_pixel(8, 8, image::Rgba([30, 120, 220, 255]))
        .save(&path)
        .expect("cannot write the PNG fixture");
    let mut document = crate::import::raster::import_raster(&path).expect("cannot import the PNG");
    let id = document
        .objects
        .first()
        .expect("the import produced no object")
        .id
        .clone();
    let mut operation = crate::operations::Operation::new(
        "Photo engraving",
        crate::operations::OperationType::ImageProcessing,
    );
    operation.params.speed = 1000.0;
    operation.params.power = 20.0;
    operation.params.passes = 2;
    let operation_id = operation.id.clone();
    document.operations.push(operation);
    document
        .object_mut(&id)
        .expect("the image is missing")
        .operation_id = Some(operation_id);
    let mut app = ui_test_app();
    app.editor.set_document(document);
    app.image_adjust = Some(id);
    let mut harness = ui_harness(app);
    harness.run();
    assert!(
        harness.query_by_label_contains("mm/min").is_some(),
        "the cut entry summary is missing"
    );
    assert!(
        harness.query_by_label_contains("Estimated:").is_some(),
        "the engraving estimate is missing"
    );
}

#[test]
fn the_layer_menu_deletes_an_empty_layer() {
    use egui_kittest::kittest::Queryable as _;

    let mut app = ui_test_app();
    let second = app.editor.document.add_layer("Layer 2").id.clone();
    app.editor.active_layer_id = second.clone();
    let mut harness = ui_harness(app);
    harness.run();
    harness.get_by_label("Layer 2").click_secondary();
    harness.run();
    assert!(
        harness.query_by_label("Delete layer").is_some(),
        "the row menu did not open"
    );
    harness.get_by_label("Delete layer").click_accesskit();
    harness.run();
    assert_eq!(
        harness.state().editor.document.layers.len(),
        1,
        "the empty layer was not deleted"
    );
    assert!(
        harness.state().editor.document.layer(&second).is_none(),
        "the deleted layer is still present"
    );
}

#[test]
fn the_material_test_and_registration_windows_open() {
    use egui_kittest::kittest::Queryable as _;

    let mut harness = ui_harness(ui_test_app());
    harness.run();
    harness.get_by_label("Operations").click();
    harness.run();
    harness.get_by_label("Material test...").click();
    harness.run();
    assert!(harness.state().material_test_open);
    for label in ["Columns / rows", "Speed range (mm/min)", "Power range (%)"] {
        assert!(
            harness.query_by_label(label).is_some(),
            "the material test is missing {label}"
        );
    }
    harness.key_press(egui::Key::Escape);
    harness.run();

    harness.get_by_label("Operations").click();
    harness.run();
    harness.get_by_label("Registration marks...").click();
    harness.run();
    assert!(harness.state().regmarks_open);
    for label in ["Size (mm)", "Margin (mm)", "Style"] {
        assert!(
            harness.query_by_label(label).is_some(),
            "the registration marks are missing {label}"
        );
    }
}

#[test]
fn the_drawing_colour_strip_colours_the_selected_layer() {
    use egui_kittest::kittest::Queryable as _;

    let mut app = ui_test_app();
    let layer = app.editor.active_layer_id.clone();
    app.editor
        .add_rectangle(Point2::new(10.0, 10.0), Point2::new(30.0, 25.0));
    let mut harness = ui_harness(app);
    harness.run();
    harness
        .get_by_role_and_label(egui::accesskit::Role::Button, "Drawing colour #e53935")
        .click_accesskit();
    harness.run();
    assert_eq!(
        harness
            .state()
            .editor
            .document
            .layer(&layer)
            .expect("the layer is missing")
            .color,
        "#e53935",
        "the palette did not colour the selected layer"
    );
    let document = &harness.state().editor.document;
    let operation = document
        .operations
        .iter()
        .find(|operation| operation.params.color == "#e53935")
        .expect("the palette did not create a cut entry");
    assert_eq!(
        document.objects[0].operation_id.as_deref(),
        Some(operation.id.as_str()),
        "the object did not adopt the colour's cut entry"
    );
    assert!(
        harness.state().status.contains("new cut entry"),
        "the new cut entry was not reported: {}",
        harness.state().status
    );
}

#[test]
fn the_palette_reuses_an_existing_cut_entry() {
    use egui_kittest::kittest::Queryable as _;

    let mut app = ui_test_app();
    let operation = app.editor.add_operation(
        "Red cut".to_string(),
        crate::operations::OperationType::OutlineCutting,
    );
    if let Some(entry) = app
        .editor
        .document
        .operations
        .iter_mut()
        .find(|entry| entry.id == operation)
    {
        entry.params.color = "#e53935".to_string();
    }
    let layer = app.editor.active_layer_id.clone();
    app.editor.document.layer_mut(&layer).unwrap().color = "#e53935".to_string();
    app.editor
        .add_rectangle(Point2::new(10.0, 10.0), Point2::new(30.0, 25.0));
    let mut harness = ui_harness(app);
    harness.run();
    harness
        .get_by_role_and_label(egui::accesskit::Role::Button, "Drawing colour #e53935")
        .click_accesskit();
    harness.run();
    let document = &harness.state().editor.document;
    assert_eq!(
        document.operations.len(),
        1,
        "an existing cut entry should be reused"
    );
    assert_eq!(
        document.objects[0].operation_id.as_deref(),
        Some(operation.as_str())
    );
    assert!(
        !harness.state().status.contains("new cut entry"),
        "the reused cut entry was reported as new"
    );
}

#[test]
fn the_palette_creates_a_cut_entry_for_the_active_layer() {
    use egui_kittest::kittest::Queryable as _;

    let mut app = ui_test_app();
    let layer = app.editor.active_layer_id.clone();
    app.editor.selection.clear();
    let mut harness = ui_harness(app);
    harness.run();
    harness
        .get_by_role_and_label(egui::accesskit::Role::Button, "Drawing colour #1e88e5")
        .click_accesskit();
    harness.run();
    let document = &harness.state().editor.document;
    assert_eq!(
        document.layer(&layer).expect("the layer is missing").color,
        "#1e88e5"
    );
    assert_eq!(
        document.operations.len(),
        1,
        "choosing a colour should create its cut entry"
    );
    assert_eq!(document.operations[0].params.color, "#1e88e5");
}
