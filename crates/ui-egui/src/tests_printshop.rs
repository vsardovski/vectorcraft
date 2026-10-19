//! The Тонекс print-shop edition in the UI: the Print Shop shortcut set, function-key tools, the
//! Object › Print Shop menu and its dialogs, and the Print Shop workspace with its color palette.

use egui::{Event, Modifiers};
use serde_json::json;
use vectorcraft_doc::NodeId;
use vectorcraft_engine::Session;

use crate::{VectorcraftApp, menus, shortcut_editor, shortcuts, workspaces};

fn app() -> VectorcraftApp {
    let mut app = VectorcraftApp::new(Session::new(), Default::default());
    app.run("file.new", json!({"width": 300, "height": 300})).unwrap();
    app
}

fn press(app: &mut VectorcraftApp, ctx: &egui::Context, key: egui::Key, modifiers: Modifiers) {
    shortcut_editor::sync(&app.ui);
    let mut events = vec![Event::ModifiersChanged(modifiers), Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers }];
    // A letter key types its letter too, as a keyboard does.
    if !(modifiers.command || modifiers.ctrl || modifiers.alt) && key.name().len() == 1 {
        let text = if modifiers.shift { key.name().to_string() } else { key.name().to_lowercase() };
        events.push(Event::Text(text));
    }
    ctx.run_ui(egui::RawInput { events, ..Default::default() }, |ui| shortcuts::handle(app, ui.ctx())).textures_delta.clear();
}

fn with_print_shop_keys(app: &mut VectorcraftApp) {
    app.run("shortcuts.preset", json!({"name": shortcut_editor::PRINT_SHOP})).unwrap();
    shortcut_editor::sync(&app.ui);
}

#[test]
fn every_print_shop_binding_is_taken_and_none_clash() {
    let set = shortcut_editor::preset(shortcut_editor::PRINT_SHOP).unwrap();
    for (key, chord) in shortcut_editor::PRINT_SHOP_KEYS {
        assert_eq!(
            shortcut_editor::effective_in(&set, key).and_then(|s| shortcut_editor::normalize(&s)),
            shortcut_editor::normalize(chord),
            "{key} → {chord}"
        );
    }
    let clashes = shortcut_editor::all_conflicts(&set);
    assert!(clashes.is_empty(), "{clashes:?}");
    // The other two sets are still the registry defaults.
    assert!(shortcut_editor::preset("VectorCraft Defaults").unwrap().is_empty());
}

#[test]
fn function_keys_pick_tools_and_single_letters_align() {
    let mut app = app();
    with_print_shop_keys(&mut app);
    let ctx = egui::Context::default();
    for (key, tool) in [
        (egui::Key::F5, "pencil"),
        (egui::Key::F6, "rectangle"),
        (egui::Key::F7, "ellipse"),
        (egui::Key::F8, "type"),
        (egui::Key::F10, "directSelection"),
    ] {
        press(&mut app, &ctx, key, Modifiers::NONE);
        assert_eq!(app.session.tool_id(), tool, "{key:?}");
    }
    app.select_tool("selection");
    let a = app.run("shape.rectangle", json!({"x": 10, "y": 10, "width": 20, "height": 20})).unwrap()["id"].as_u64().unwrap();
    let b = app.run("shape.rectangle", json!({"x": 80, "y": 60, "width": 40, "height": 30})).unwrap()["id"].as_u64().unwrap();
    app.run("select.set", json!({"ids": [a, b]})).unwrap();
    press(&mut app, &ctx, egui::Key::T, Modifiers::NONE);
    let top = |app: &VectorcraftApp, id: u64| app.session.active().unwrap().doc.bounds_of(&[NodeId(id)], false).unwrap().y0;
    assert_eq!(top(&app, a), top(&app, b), "T aligns tops");
    assert_eq!(app.session.tool_id(), "selection", "T no longer picks the Type tool");
    press(&mut app, &ctx, egui::Key::P, Modifiers::NONE);
    let bb = app.session.active().unwrap().doc.bounds_of(&[NodeId(a), NodeId(b)], false).unwrap();
    assert_eq!((bb.center().x, bb.center().y), (150.0, 150.0), "P centers to the page");
}

#[test]
fn ctrl_shift_d_opens_step_and_repeat_and_ok_steps() {
    let mut app = app();
    with_print_shop_keys(&mut app);
    let ctx = egui::Context::default();
    let id = app.run("shape.rectangle", json!({"x": 0, "y": 0, "width": 20, "height": 10})).unwrap()["id"].as_u64().unwrap();
    app.run("select.set", json!({"ids": [id]})).unwrap();
    press(&mut app, &ctx, egui::Key::D, Modifiers::COMMAND | Modifiers::SHIFT);
    let d = app.ui.dialog.as_ref().expect("the Step and Repeat dialog");
    assert_eq!(d.str("__command"), "printshop.stepAndRepeat");
    let out = app.run("printshop.stepAndRepeat", json!({"copies": 3, "hMode": "spacing", "hDistance": 2})).unwrap();
    assert_eq!(out["ids"].as_array().unwrap().len(), 3);
}

#[test]
fn object_menu_holds_the_print_shop_commands() {
    let app = app();
    let entries = menus::menu_entries(&app);
    for label in ["Step and Repeat…", "Add Cut Contour…", "Convert to Curves", "Combine", "Break Apart", "Center to Page"] {
        assert!(entries.iter().any(|e| e.path.first().map(String::as_str) == Some("Object") && e.label == label), "{label}");
    }
    // The menu item opens the parameter dialog rather than running.
    let step = entries.iter().find(|e| e.label == "Step and Repeat…").unwrap();
    let (cmd, p) = menus::click_target(&step.label, step.command.as_deref().unwrap(), &step.params);
    assert_eq!(cmd, "ui.paramDialog");
    assert_eq!(p["command"], "printshop.stepAndRepeat");
}

#[test]
fn the_print_shop_workspace_shows_the_color_palette() {
    let mut app = app();
    assert!(!app.ui.color_strip);
    let w = workspaces::find(&app.ui, workspaces::PRINT_SHOP).unwrap();
    workspaces::apply(&mut app.ui, &w);
    assert!(app.ui.color_strip && app.ui.control_bar && app.ui.view.rulers);
    app.run("window.colorPalette", json!({})).unwrap();
    assert!(!app.ui.color_strip);
    let essentials = workspaces::find(&app.ui, workspaces::ESSENTIALS).unwrap();
    workspaces::apply(&mut app.ui, &essentials);
    assert!(!app.ui.color_strip);
}

#[test]
fn add_cut_contour_opens_its_dialog_and_ok_adds_the_spot_line() {
    let mut app = app();
    let id = app.run("shape.rectangle", json!({"x": 10, "y": 10, "width": 40, "height": 20})).unwrap()["id"].as_u64().unwrap();
    app.run("select.set", json!({"ids": [id]})).unwrap();
    // Without params (a key or the command palette) the command opens its dialog.
    menus::invoke(&mut app, "printshop.cutContour", json!({}));
    let d = app.ui.dialog.as_ref().expect("the Add Cut Contour dialog");
    assert_eq!(d.str("__command"), "printshop.cutContour");
    assert_eq!(d.str("shape"), "outline");
    // What OK runs: the dialog's fields as params.
    let out = app.run("printshop.cutContour", json!({"shape": "box", "offset": 3, "weight": 0.25})).unwrap();
    let doc = &app.session.active().unwrap().doc;
    assert!(doc.swatch("CutContour").is_some_and(|w| w.spot));
    let b = doc.bounds_of(&[NodeId(out["id"].as_u64().unwrap())], false).unwrap();
    // 3 pt out from the visible bounds (the 1 pt stroke takes in half a point).
    assert_eq!((b.x0, b.y0, b.x1, b.y1), (6.5, 6.5, 53.5, 33.5));
}

/// Whether `shapes` paint a rectangle filled with `color`.
fn paints_rect(shapes: &[egui::Shape], color: egui::Color32) -> bool {
    shapes.iter().any(|s| match s {
        egui::Shape::Rect(r) => r.fill == color,
        egui::Shape::Vec(v) => paints_rect(v, color),
        _ => false,
    })
}

#[test]
fn the_status_bar_shows_the_fill_with_the_color_palette_on() {
    let mut app = app();
    let id = app.run("shape.rectangle", json!({"x": 10, "y": 10, "width": 40, "height": 20})).unwrap()["id"].as_u64().unwrap();
    app.run("select.set", json!({"ids": [id]})).unwrap();
    app.run("paint.setFill", json!({"color": "#00ff00"})).unwrap();
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let draw = |app: &mut VectorcraftApp| {
        let screen = Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 800.0)));
        let mut shapes = vec![];
        for _ in 0..2 {
            let mut out = ctx.run_ui(egui::RawInput { screen_rect: screen, ..Default::default() }, |ui| crate::chrome::status_bar(app, ui));
            out.textures_delta.clear();
            shapes = out.shapes.into_iter().map(|c| c.shape).collect();
        }
        paints_rect(&shapes, egui::Color32::from_rgb(0, 255, 0))
    };
    assert!(!draw(&mut app), "off by default");
    app.ui.color_strip = true;
    assert!(draw(&mut app), "the fill chip shows the selection's green");
}
