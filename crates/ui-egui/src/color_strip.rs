//! Window › Color Palette: a column of the document's colour swatches down the window's right edge,
//! where print-shop operators expect it. Click a chip to fill the selection with that swatch,
//! right-click to stroke it; the crossed chip at the top is None. The column scrolls when the
//! swatches don't fit.

use egui::{PointerButton, Sense, Stroke, Ui, vec2};
use serde_json::json;
use vectorcraft_color::Paint;

use crate::VectorcraftApp;
use crate::theme::Tokens;

/// A chip's side (points).
pub const CHIP: f32 = 18.0;
/// The strip's width: a chip and its margins.
pub const WIDTH: f32 = CHIP + 10.0;
/// The most chips listed (a document can hold any number of swatches).
const MAX_CHIPS: usize = 500;

/// One chip: the swatch's name (`None` for the None chip) and its paint.
#[derive(Clone, Debug, PartialEq)]
pub struct Chip {
    pub name: Option<String>,
    pub paint: Paint,
}

/// The chips the strip shows: None, then the active document's solid colour swatches in the
/// Swatches panel's order (Registration, gradients and patterns left out).
pub fn chips(app: &VectorcraftApp) -> Vec<Chip> {
    let mut v = vec![Chip { name: None, paint: Paint::None }];
    if let Some(d) = app.session.active() {
        v.extend(
            d.doc
                .swatches_iter()
                .filter(|w| !w.is_reserved() && w.paint.color().is_some())
                .take(MAX_CHIPS)
                .map(|w| Chip { name: Some(w.name.clone()), paint: w.paint.clone() }),
        );
    }
    v
}

/// What a click on `chip` runs: the fill (`stroke` false) or the stroke of the selection.
pub fn action(chip: &Chip, stroke: bool) -> (&'static str, serde_json::Value) {
    let id = if stroke { "paint.setStroke" } else { "paint.setFill" };
    match &chip.name {
        Some(name) => (id, json!({ "swatch": name })),
        None => (id, json!({ "none": true })),
    }
}

/// Draw the strip as a right-edge panel (call before the dock, so it sits outside it).
pub fn show(app: &mut VectorcraftApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    let chips = chips(app);
    let mut clicked = None;
    egui::Panel::right("color_strip")
        .resizable(false)
        .exact_size(WIDTH)
        .frame(egui::Frame::NONE.fill(t.panel).inner_margin(egui::Margin::symmetric(5, 6)).stroke(Stroke::new(1.0, t.border)))
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("color-strip")
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                .auto_shrink([true, false])
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing = vec2(0.0, 2.0);
                    for chip in &chips {
                        let (rect, r) = ui.allocate_exact_size(vec2(CHIP, CHIP), Sense::click());
                        crate::widgets::swatch_tile(ui, rect, &chip.paint, false, r.hovered());
                        let name = chip.name.clone().unwrap_or_else(|| tl!("None").to_string());
                        let r = r.on_hover_text(format!("{name}\n{}", tl!("Click: fill · Right-click: stroke")));
                        if r.clicked() {
                            clicked = Some((chip.clone(), false));
                        } else if r.clicked_by(PointerButton::Secondary) {
                            clicked = Some((chip.clone(), true));
                        }
                    }
                });
        });
    if let Some((chip, stroke)) = clicked {
        let (id, p) = action(&chip, stroke);
        if let Err(e) = app.run(id, p) {
            app.status(e);
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use vectorcraft_doc::NodeId;
    use vectorcraft_engine::Session;

    use super::*;

    fn app() -> VectorcraftApp {
        let mut app = VectorcraftApp::new(Session::new(), Default::default());
        app.run("file.new", json!({"width": 300, "height": 300})).unwrap();
        app
    }

    #[test]
    fn none_comes_first_then_the_solid_swatches_without_registration() {
        let app = app();
        let chips = chips(&app);
        assert_eq!(chips.first().map(|c| c.name.clone()), Some(None));
        assert!(chips.len() > 1, "a new document has colour swatches");
        assert!(chips.iter().skip(1).all(|c| c.paint.color().is_some() && !c.paint.is_registration()));
    }

    #[test]
    fn a_click_fills_and_a_right_click_strokes_the_selection() {
        let mut app = app();
        let id = app.run("shape.rectangle", json!({"x": 10, "y": 10, "width": 50, "height": 50})).unwrap()["id"].as_u64().unwrap();
        app.run("select.set", json!({"ids": [id]})).unwrap();
        let chip = chips(&app).into_iter().nth(1).unwrap();
        let (cmd, p) = action(&chip, false);
        app.run(cmd, p).unwrap();
        let (cmd, p) = action(&Chip { name: None, paint: Paint::None }, true);
        app.run(cmd, p).unwrap();
        let d = &app.session.active().unwrap().doc;
        let n = d.node(NodeId(id)).unwrap();
        // A process swatch gives its colour (a global one would stay linked by name).
        assert_eq!(n.appearance.fill_paint().color(), chip.paint.color());
        assert!(n.appearance.stroke().is_none_or(|s| s.paint.is_none()));
    }

    #[test]
    fn the_strip_draws_a_chip_per_swatch() {
        let mut app = app();
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let n = chips(&app).len();
        let mut shown = 0;
        for _ in 0..2 {
            let mut out = ctx.run_ui(
                egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(400.0, 2000.0))), ..Default::default() },
                |ui| show(&mut app, ui),
            );
            out.textures_delta.clear();
            shown = out.shapes.len();
        }
        assert!(shown >= n, "{shown} shapes for {n} chips");
    }
}
