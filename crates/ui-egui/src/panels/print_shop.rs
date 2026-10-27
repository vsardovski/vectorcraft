//! Window → Print Shop (Тонекс 2000 ЕООД): is the open file ready for prepress?
//!
//! Pick the production service (digital, offset, wide-format, stickers with contour cut, laser,
//! CNC, DTF/DTG) and the panel shows `printshop.preflight`: a verdict (ready / review / back to
//! design), each finding with the objects it is about (click to select them), the artboards'
//! sizes and the cut lines' length. Below: Make Cut Contour (`printshop.cutContour.make`) and
//! Save Report (the Bulgarian text report, to send to the client or attach to the order).
//!
//! The panel is the shop's own: its text is Bulgarian when the interface is, English otherwise,
//! and comes from the engine (`messageBg` / `message`), so it is not in the shared catalogs.

use egui::{Color32, RichText, Ui};
use serde_json::{Value, json};
use vectorcraft_engine::cmd::printshop::PROFILES;

use super::{pstate, set_pstate};
use crate::theme::Tokens;
use crate::{VectorcraftApp, widgets};

/// The panel id (Window → Print Shop).
pub const ID: &str = "printShop";

/// Is the interface Bulgarian?
fn bg() -> bool {
    crate::i18n::current().code() == "bg"
}

/// `bg` text in a Bulgarian interface, `en` otherwise.
fn bi<'a>(en: &'a str, bg_text: &'a str) -> &'a str {
    if bg() { bg_text } else { en }
}

/// The preflight report and what it was made for: (document uid, revision, profile, selection only).
#[derive(Clone, Default)]
struct Cache {
    key: Option<(u64, u64, String, bool)>,
    report: Value,
}

fn report(app: &mut VectorcraftApp, ctx: &egui::Context, profile: &str, sel_only: bool) -> Option<Value> {
    let st = app.session.active()?;
    let key = Some((st.uid, st.revision, profile.to_string(), sel_only));
    let mut c: Cache = pstate(ctx, "printshop-cache");
    if c.key != key {
        c.report = app.session.execute("printshop.preflight", &json!({ "profile": profile, "selectionOnly": sel_only })).ok()?;
        c.key = key;
        set_pstate(ctx, "printshop-cache", c.clone());
    }
    Some(c.report)
}

/// The colour of a verdict or finding level.
fn level_color(t: &Tokens, level: &str) -> Color32 {
    match level {
        "error" | "design" => t.error,
        "warning" | "review" => t.warning,
        "ready" => Color32::from_rgb(52, 168, 83),
        _ => t.text_dim,
    }
}

pub fn show(app: &mut VectorcraftApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    if app.session.active().is_none() {
        widgets::dim_label(ui, bi("No document", "Няма отворен документ"));
        return;
    }
    let ctx = ui.ctx().clone();
    let mut profile: String = pstate(&ctx, "printshop-profile");
    if !PROFILES.iter().any(|p| p.id == profile) {
        profile = "digital".into();
    }
    let mut sel_only: bool = pstate(&ctx, "printshop-sel");

    // The service and the scope.
    let labels: Vec<&str> = PROFILES.iter().map(|p| if bg() { p.label_bg } else { p.label }).collect();
    let at = PROFILES.iter().position(|p| p.id == profile).unwrap_or(0);
    let current = labels.get(at).copied().unwrap_or_default();
    ui.horizontal(|ui| {
        ui.label(RichText::new(bi("Service", "Услуга")).size(12.0).color(t.text_dim));
        if let Some(i) = widgets::dropdown_names(ui, "printshop-profile", current, &labels, 200.0)
            && let Some(p) = PROFILES.get(i)
        {
            profile = p.id.to_string();
            set_pstate(&ctx, "printshop-profile", profile.clone());
        }
    });
    if ui.checkbox(&mut sel_only, RichText::new(bi("Selection only", "Само избраното")).size(12.0)).changed() {
        set_pstate(&ctx, "printshop-sel", sel_only);
    }
    let Some(r) = report(app, &ctx, &profile, sel_only) else {
        widgets::dim_label(ui, bi("The check could not run", "Проверката не можа да се изпълни"));
        return;
    };
    widgets::divider(ui);

    // The verdict.
    let verdict = r["verdict"].as_str().unwrap_or_default();
    let verdict_text = if bg() {
        r["verdictBg"].as_str().unwrap_or_default().to_string()
    } else {
        match verdict {
            "design" => "Back to design: the file has errors".to_string(),
            "review" => "Review: printable, with warnings".to_string(),
            _ => "Ready for prepress".to_string(),
        }
    };
    let color = level_color(&t, verdict);
    egui::Frame::new().fill(color.gamma_multiply(0.18)).stroke(egui::Stroke::new(1.0, color)).corner_radius(4.0).inner_margin(8.0).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(RichText::new(verdict_text).size(13.0).strong().color(t.text_strong));
        let counts = format!(
            "{}: {}   {}: {}",
            bi("Errors", "Грешки"),
            r["errors"].as_u64().unwrap_or(0),
            bi("Warnings", "Забележки"),
            r["warnings"].as_u64().unwrap_or(0)
        );
        ui.label(RichText::new(counts).size(11.5).color(t.text_dim));
    });

    // The findings.
    let mut select: Option<Value> = None;
    egui::ScrollArea::vertical().id_salt("printshop-findings").max_height(300.0).show(ui, |ui| {
        let findings = r["findings"].as_array().map_or(&[][..], Vec::as_slice);
        if findings.is_empty() {
            widgets::dim_label(ui, bi("No findings", "Няма забележки"));
        }
        for (k, f) in findings.iter().enumerate() {
            let level = f["level"].as_str().unwrap_or("info");
            let text = if bg() { f["messageBg"].as_str() } else { f["message"].as_str() }.unwrap_or_default();
            ui.horizontal_top(|ui| {
                let (dot, _) = ui.allocate_exact_size(egui::vec2(10.0, 16.0), egui::Sense::hover());
                ui.painter().circle_filled(dot.center(), 4.0, level_color(&t, level));
                let ids = f["ids"].as_array().filter(|ids| !ids.is_empty());
                let width = ui.available_width() - if ids.is_some() { 64.0 } else { 0.0 };
                ui.add_sized([width.max(80.0), 0.0], egui::Label::new(RichText::new(text).size(12.0).color(t.text)).wrap());
                if let Some(ids) = ids
                    && ui.push_id(("printshop-select", k), |ui| ui.small_button(bi("Select", "Покажи"))).inner.clicked()
                {
                    select = Some(Value::Array(ids.clone()));
                }
            });
        }
    });
    if let Some(ids) = select {
        app.run("select.set", json!({ "ids": ids })).ok();
    }
    widgets::divider(ui);

    // Sizes and cut lines.
    ui.label(RichText::new(bi("Artboards", "Формат")).size(12.5).color(t.text));
    for a in r["artboards"].as_array().map_or(&[][..], Vec::as_slice) {
        let value = format!(
            "{} × {} mm · {} m²",
            a["widthMm"].as_f64().unwrap_or(0.0),
            a["heightMm"].as_f64().unwrap_or(0.0),
            a["areaM2"].as_f64().unwrap_or(0.0)
        );
        super::doc_info::row(ui, a["name"].as_str().unwrap_or_default(), value);
    }
    let cut = &r["cut"];
    if cut["count"].as_u64().unwrap_or(0) > 0 {
        ui.add_space(4.0);
        ui.label(RichText::new(bi("Cut lines", "Рязане")).size(12.5).color(t.text));
        super::doc_info::row(
            ui,
            bi("Lines", "Линии"),
            format!("{} ({} {})", cut["count"].as_u64().unwrap_or(0), cut["closed"].as_u64().unwrap_or(0), bi("closed", "затворени")),
        );
        super::doc_info::row(ui, bi("Length", "Дължина"), format!("{:.2} {}", cut["lengthM"].as_f64().unwrap_or(0.0), bi("m", "л.м.")));
        super::doc_info::row(ui, bi("Pieces area", "Площ на детайлите"), format!("{} m²", cut["piecesAreaM2"].as_f64().unwrap_or(0.0)));
    }
    widgets::divider(ui);

    // Make Cut Contour.
    ui.label(RichText::new(bi("Cut contour", "Контур за рязане")).size(12.5).color(t.text));
    let mut offset: f64 = pstate::<Option<f64>>(&ctx, "printshop-offset").unwrap_or(2.0);
    let mut all: bool = pstate(&ctx, "printshop-all");
    ui.horizontal(|ui| {
        ui.label(RichText::new(bi("Offset", "Отстояние")).size(12.0).color(t.text_dim));
        if let Some(v) = widgets::plain_field(ui, "printshop-offset-field", offset, "mm", 1, 64.0)
            && v.is_finite()
        {
            offset = v.clamp(-500.0, 500.0);
            set_pstate(&ctx, "printshop-offset", Some(offset));
        }
    });
    if ui.checkbox(&mut all, RichText::new(bi("One contour around everything", "Един общ контур около всичко")).size(12.0)).changed()
    {
        set_pstate(&ctx, "printshop-all", all);
    }
    let has_selection = app.session.active().is_some_and(|d| !d.selection.is_empty());
    ui.horizontal(|ui| {
        if widgets::primary_button_enabled(ui, bi("Make Cut Contour", "Направи контур"), has_selection).clicked() {
            match app.run("printshop.cutContour.make", json!({ "offset": offset, "each": !all })) {
                Ok(v) => {
                    let n = v["ids"].as_array().map_or(0, Vec::len);
                    let done = format!("{} {n}", bi("Cut contours made:", "Направени контури за рязане:"));
                    app.status(done);
                }
                Err(e) => app.status(e),
            }
        }
        if widgets::secondary_button(ui, bi("Save Report…", "Запиши доклад…")).clicked() {
            let saved = save_report(app, &profile, sel_only);
            app.status(saved.unwrap_or_else(|e| e));
        }
    });
    if !has_selection {
        widgets::dim_label(ui, bi("Select the art to outline", "Изберете обектите за контура"));
    }
}

/// Write the Bulgarian text report next to the document (a picked file; the web downloads it).
fn save_report(app: &mut VectorcraftApp, profile: &str, sel_only: bool) -> Result<String, String> {
    let r = app
        .session
        .execute("printshop.preflight", &json!({ "profile": profile, "selectionOnly": sel_only, "format": "text" }))
        .map_err(|e| e.to_string())?;
    let text = r["text"].as_str().unwrap_or_default().to_string();
    let title = app.session.active().map(|d| d.title()).unwrap_or_default();
    let stem = std::path::Path::new(&title).file_stem().map_or_else(|| "Untitled".into(), |s| s.to_string_lossy().into_owned());
    let path = crate::io::write_named(app, None, &format!("{stem} - {}.txt", bi("preflight", "проверка")), text.as_bytes())?;
    Ok(format!("{} {path}", bi("Saved", "Записано:")))
}

pub fn menu(app: &mut VectorcraftApp, ui: &mut Ui) {
    let profile: String = pstate(ui.ctx(), "printshop-profile");
    let profile = if profile.is_empty() { "digital".to_string() } else { profile };
    let sel_only: bool = pstate(ui.ctx(), "printshop-sel");
    if widgets::menu_item_name(ui, bi("Save Report…", "Запиши доклад…"), app.session.active().is_some(), false) {
        let saved = save_report(app, &profile, sel_only);
        app.status(saved.unwrap_or_else(|e| e));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_profile_has_both_labels() {
        for p in PROFILES {
            assert!(!p.label.is_empty() && !p.label_bg.is_empty(), "{}", p.id);
        }
        assert!(PROFILES.iter().any(|p| p.id == "digital"));
    }

    /// The panel draws with and without a document and with findings, for every service.
    #[test]
    fn draws_headless() {
        let mut app = VectorcraftApp::new(vectorcraft_engine::Session::new(), Default::default());
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| show(&mut app, ui));
        out.textures_delta.clear();
        app.session.execute("file.new", &json!({"width": 100, "height": 100})).unwrap();
        app.session.execute("shape.rectangle", &json!({"x": 0, "y": 0, "width": 100, "height": 100})).unwrap();
        app.session.execute("text.create", &json!({"x": 10, "y": 40, "text": "Тонекс"})).unwrap();
        for p in PROFILES {
            set_pstate(&ctx, "printshop-profile", p.id.to_string());
            let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
                show(&mut app, ui);
                menu(&mut app, ui);
            });
            out.textures_delta.clear();
        }
        let c: Cache = pstate(&ctx, "printshop-cache");
        assert!(c.report["findings"].as_array().is_some_and(|f| !f.is_empty()));
    }
}
