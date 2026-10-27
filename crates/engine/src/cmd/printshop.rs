//! Print shop tools (Тонекс 2000 ЕООД): file preflight per production service, cut contours for
//! stickers and labels, and cut statistics (linear metres of cut, pieces, areas) for quotes.
//!
//! - `printshop.profiles`: the production services a file can be checked for.
//! - `printshop.preflight`: is the file ready for prepress, or must it go back to design? Each
//!   finding has an English `message` and a Bulgarian `messageBg` (the shop's colleagues read
//!   the Bulgarian; the control channel, MCP and tests read either), the objects it is about and
//!   a verdict: `ready`, `review` (warnings only) or `design` (errors).
//! - `printshop.cutStats`: the cut lines (strokes in a `CutContour`-style spot colour, or every
//!   stroked path for laser and CNC) measured: count, open/closed, length, pieces and areas.
//! - `printshop.cutContour.make`: a contour around each selected object (or around all of them),
//!   offset outward, stroked 0.25 pt in the `CutContour` spot colour on a `CutContour` layer.
//!
//! Everything here is a read of the document or one undo step; no panics on any input.

use std::collections::BTreeSet;

use serde_json::{Value, json};
use vectorcraft_color::swatch::Swatch;
use vectorcraft_color::{BlendMode, Color, Paint};
use vectorcraft_doc::appearance::AppearanceItem;
use vectorcraft_doc::{Appearance, ColorMode, Document, LayerColor, Node, NodeId, NodeKind, Shared};
use vectorcraft_geom::{FillRule, PathData, Rect, Shape};
use vectorcraft_pathops as po;

use super::edit::selected_roots;
use super::*;

/// Points per millimetre.
pub const MM: f64 = 72.0 / 25.4;
/// The spot colour cutting plotters and print-and-cut RIPs read as the contour cut line.
pub const CUT_CONTOUR: &str = "CutContour";
/// Swatch names (lower case, spaces and dashes dropped) read as cut lines by
/// [`is_cut_paint`]: the common RIP names, and the Bulgarian ones colleagues use.
const CUT_NAMES: [&str; 12] =
    ["cutcontour", "cut", "thrucut", "kisscut", "diecut", "cutline", "contourcut", "perfcut", "crease", "рязане", "контур", "щанца"];
/// At most this many pieces are listed one by one in `cutStats` (the totals count all).
const MAX_PIECES: usize = 500;
/// How close (pt) an object's edge must be to the trim to count as touching it.
const EDGE_TOL: f64 = 0.75;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            query "printshop.profiles",
            "Production Profiles",
            [],
            None,
            "{} → {profiles: [{id, label, labelBg, minPpi, recommendedPpi, bleedMm, cmyk, cutLine, vectorOnly, minStrokePt, maxInk}]} the production services printshop.preflight checks a file for",
            always,
            profiles
        ),
        cmd!(
            query "printshop.preflight",
            "Preflight for Production…",
            [],
            None,
            "{profile?: offset|digital|wideFormat|sticker|laser|cnc|dtf (default digital), selectionOnly?: false, minPpi?, recommendedPpi?, bleedMm? (override the profile's), format?: \"text\" (→ {text}: the Bulgarian report to save or send)} check the document for that production service → {profile, verdict: ready|review|design, verdictBg, errors, warnings, findings: [{code, level: error|warning|info, message, messageBg, ids, values}], artboards: [{name, widthMm, heightMm, areaM2}], cut: as printshop.cutStats}",
            has_doc,
            preflight
        ),
        cmd!(
            query "printshop.cutStats",
            "Cut Line Statistics",
            [],
            None,
            "{mode?: \"cutContour\" (strokes in a cut spot colour: CutContour, Cut, Thru-cut, Kiss-cut…, default)|\"allStrokes\" (every stroked path: laser, CNC, plotter), names?: [extra swatch names read as cut lines], selectionOnly?: false} → {count, closed, open, lengthMm, lengthM, boundsMm: {x, y, width, height}|null, piecesAreaM2: the pieces' bounding boxes, enclosedAreaM2: inside the closed cut lines, pieces: [{id, widthMm, heightMm, lengthMm, closed}] (first 500)}",
            has_doc,
            cut_stats_cmd
        ),
        cmd!(
            "printshop.cutContour.make",
            "Make Cut Contour…",
            [],
            None,
            "{offset?: mm outward (default 2; 0 = on the art's outline; negative insets), each?: true (one contour per selected object; false: one around the whole selection), name?: spot swatch name (default \"CutContour\", 100% magenta, created when missing), weight?: pt (0.25), layer?: true (put the contours on a top \"CutContour\" layer; false: above each object)} outline the selected art (paths, type, images by their bounds), unite and offset it, and stroke it with the cut spot colour, no fill → {ids, swatch, layer}",
            has_selection,
            make_cut_contour
        ),
    ]
}

// ---------- profiles ----------

/// One production service and what its files need.
#[derive(Clone, Copy, Debug)]
pub struct Profile {
    pub id: &'static str,
    pub label: &'static str,
    pub label_bg: &'static str,
    /// Images below this effective resolution are errors (0: no raster check).
    pub min_ppi: f64,
    /// Images below this are warnings.
    pub recommended_ppi: f64,
    /// Bleed the document setup must give on every side (0: none needed).
    pub bleed_mm: f64,
    /// Process colour (CMYK) is expected: RGB documents and colours are warned about.
    pub cmyk: bool,
    /// A cut line (CutContour) must be present.
    pub cut_line: bool,
    /// Vector only: images are errors and live type must be outlined.
    pub vector_only: bool,
    /// Strokes thinner than this (pt) are warned about (0: no check; cut lines never are).
    pub min_stroke_pt: f64,
    /// Total ink limit in percent for CMYK colours (0: no check).
    pub max_ink: f64,
}

pub const PROFILES: [Profile; 7] = [
    Profile {
        id: "digital",
        label: "Digital print",
        label_bg: "Дигитален печат",
        min_ppi: 150.0,
        recommended_ppi: 250.0,
        bleed_mm: 2.0,
        cmyk: true,
        cut_line: false,
        vector_only: false,
        min_stroke_pt: 0.25,
        max_ink: 320.0,
    },
    Profile {
        id: "offset",
        label: "Offset print",
        label_bg: "Офсетов печат",
        min_ppi: 200.0,
        recommended_ppi: 300.0,
        bleed_mm: 3.0,
        cmyk: true,
        cut_line: false,
        vector_only: false,
        min_stroke_pt: 0.25,
        max_ink: 300.0,
    },
    Profile {
        id: "wideFormat",
        label: "Wide-format print",
        label_bg: "Широкоформатен печат",
        min_ppi: 50.0,
        recommended_ppi: 100.0,
        bleed_mm: 0.0,
        cmyk: false,
        cut_line: false,
        vector_only: false,
        min_stroke_pt: 0.5,
        max_ink: 0.0,
    },
    Profile {
        id: "sticker",
        label: "Stickers with contour cut",
        label_bg: "Стикери с контурно рязане",
        min_ppi: 100.0,
        recommended_ppi: 150.0,
        bleed_mm: 0.0,
        cmyk: false,
        cut_line: true,
        vector_only: false,
        min_stroke_pt: 0.25,
        max_ink: 0.0,
    },
    Profile {
        id: "laser",
        label: "Laser cutting and engraving",
        label_bg: "Лазерно рязане и гравиране",
        min_ppi: 0.0,
        recommended_ppi: 0.0,
        bleed_mm: 0.0,
        cmyk: false,
        cut_line: false,
        vector_only: true,
        min_stroke_pt: 0.0,
        max_ink: 0.0,
    },
    Profile {
        id: "cnc",
        label: "CNC milling",
        label_bg: "CNC фрезоване",
        min_ppi: 0.0,
        recommended_ppi: 0.0,
        bleed_mm: 0.0,
        cmyk: false,
        cut_line: false,
        vector_only: true,
        min_stroke_pt: 0.0,
        max_ink: 0.0,
    },
    Profile {
        id: "dtf",
        label: "DTF / DTG textile print",
        label_bg: "DTF / DTG печат върху текстил",
        min_ppi: 150.0,
        recommended_ppi: 250.0,
        bleed_mm: 0.0,
        cmyk: false,
        cut_line: false,
        vector_only: false,
        min_stroke_pt: 0.5,
        max_ink: 0.0,
    },
];

/// Profile `id` (the default `digital` when `None`).
pub fn profile(id: Option<&str>) -> Option<Profile> {
    let id = id.unwrap_or("digital");
    PROFILES.iter().copied().find(|p| p.id.eq_ignore_ascii_case(id))
}

fn profile_json(p: &Profile) -> Value {
    json!({
        "id": p.id, "label": p.label, "labelBg": p.label_bg, "minPpi": p.min_ppi, "recommendedPpi": p.recommended_ppi,
        "bleedMm": p.bleed_mm, "cmyk": p.cmyk, "cutLine": p.cut_line, "vectorOnly": p.vector_only,
        "minStrokePt": p.min_stroke_pt, "maxInk": p.max_ink,
    })
}

fn profiles(_: &mut Session, _: &Value) -> Result<Value> {
    Ok(json!({ "profiles": PROFILES.iter().map(profile_json).collect::<Vec<_>>() }))
}

// ---------- walking the printable art ----------

/// The printable leaves under `n`: visible objects outside hidden, template and non-printing
/// layers, guides left out. Groups and layers are opened; every other object is one leaf.
fn printable_leaves<'a>(n: &'a Node, out: &mut Vec<&'a Node>) {
    if !n.visible {
        return;
    }
    match &n.kind {
        NodeKind::Layer { printable, template, children, .. } => {
            if *printable && !*template {
                for c in children {
                    printable_leaves(c, out);
                }
            }
        }
        NodeKind::Group { children, .. } => {
            for c in children {
                printable_leaves(c, out);
            }
        }
        NodeKind::Path { guide: true, .. } => {}
        _ => out.push(n),
    }
}

/// The roots to check: the selected objects, or every layer.
fn roots(st: &crate::DocState, selection_only: bool) -> Vec<&Node> {
    if selection_only {
        st.selection.objects.iter().filter_map(|id| st.doc.node(*id)).collect()
    } else {
        st.doc.layers.iter().map(|l| &**l).collect()
    }
}

fn normalized(name: &str) -> String {
    name.chars().filter(|c| !matches!(c, ' ' | '-' | '_')).flat_map(char::to_lowercase).collect()
}

/// Is `paint` a cut-line colour: a solid linked to a swatch with one of the [`CUT_NAMES`] (or
/// one of `extra`)?
pub fn is_cut_paint(paint: &Paint, extra: &[String]) -> bool {
    let Paint::Solid { swatch: Some(name), .. } = paint else { return false };
    let n = normalized(name);
    CUT_NAMES.contains(&n.as_str()) || extra.iter().any(|e| normalized(e) == n)
}

/// The visible stroke layers of `n` that paint something.
fn strokes(n: &Node) -> impl Iterator<Item = &vectorcraft_doc::appearance::StrokeLayer> {
    n.appearance.items.iter().filter_map(|i| match i {
        AppearanceItem::Stroke(s) if s.visible && !s.paint.is_none() && s.width > 0.0 => Some(s),
        _ => None,
    })
}

/// Is `n` a cut line: one of its strokes is in a cut colour.
fn is_cut_node(n: &Node, extra: &[String]) -> bool {
    strokes(n).any(|s| is_cut_paint(&s.paint, extra))
}

/// The visible fill and stroke paints of `n` with their overprint flag.
fn paints(n: &Node) -> impl Iterator<Item = (&Paint, bool)> {
    n.appearance.items.iter().filter_map(|i| match i {
        AppearanceItem::Fill(f) if f.visible => Some((&f.paint, f.overprint)),
        AppearanceItem::Stroke(s) if s.visible && s.width > 0.0 => Some((&s.paint, s.overprint)),
        _ => None,
    })
}

/// The geometry a cut line or contour is measured on: paths and compound paths as drawn, type by
/// its glyph outlines.
fn geometry(n: &Node) -> Option<PathData> {
    match &n.kind {
        NodeKind::Path { .. } | NodeKind::Compound { .. } | NodeKind::Text(_) => super::pathops::node_path(n).map(|(p, _)| p),
        _ => None,
    }
}

fn rect_path(r: Rect) -> PathData {
    PathData::from_bezpath(&r.to_path(0.1))
}

fn mm(pt: f64) -> f64 {
    if pt.is_finite() { (pt / MM * 100.0).round() / 100.0 } else { 0.0 }
}

fn m2(pt2: f64) -> f64 {
    if pt2.is_finite() { (pt2 / (MM * MM) / 1e6 * 10000.0).round() / 10000.0 } else { 0.0 }
}

fn rect_mm(r: Rect) -> Value {
    json!({ "x": mm(r.x0), "y": mm(r.y0), "width": mm(r.width()), "height": mm(r.height()) })
}

// ---------- cut statistics ----------

/// Cut lines measured (see `printshop.cutStats`).
#[derive(Debug, Default)]
pub struct CutStats {
    pub count: usize,
    pub closed: usize,
    pub open: usize,
    /// Total length in points.
    pub length: f64,
    pub bounds: Option<Rect>,
    /// Sum of the pieces' bounding-box areas, pt².
    pub pieces_area: f64,
    /// Area inside the closed cut lines, pt².
    pub enclosed_area: f64,
    /// (id, bounds, length, closed) of the first [`MAX_PIECES`].
    pub pieces: Vec<(NodeId, Rect, f64, bool)>,
    /// The cut lines that are filled (a cut line should have no fill).
    pub filled: Vec<NodeId>,
    /// The open cut lines.
    pub open_ids: Vec<NodeId>,
}

impl CutStats {
    pub fn json(&self) -> Value {
        json!({
            "count": self.count,
            "closed": self.closed,
            "open": self.open,
            "lengthMm": mm(self.length),
            "lengthM": if self.length.is_finite() { (self.length / MM / 1000.0 * 1000.0).round() / 1000.0 } else { 0.0 },
            "boundsMm": self.bounds.map(rect_mm),
            "piecesAreaM2": m2(self.pieces_area),
            "enclosedAreaM2": m2(self.enclosed_area),
            "pieces": self.pieces.iter().map(|(id, b, l, closed)| json!({
                "id": id.0, "widthMm": mm(b.width()), "heightMm": mm(b.height()), "lengthMm": mm(*l), "closed": closed,
            })).collect::<Vec<_>>(),
        })
    }
}

/// Measure the cut lines among `leaves`: those in a cut colour, or (`all_strokes`) every stroked
/// path.
pub fn cut_stats(leaves: &[&Node], all_strokes: bool, extra: &[String]) -> CutStats {
    let mut c = CutStats::default();
    for n in leaves {
        let cut = if all_strokes { strokes(n).next().is_some() } else { is_cut_node(n, extra) };
        if !cut || (matches!(n.kind, NodeKind::Text(_)) && !all_strokes) {
            continue;
        }
        let Some(path) = geometry(n) else { continue };
        let length = path.length();
        let length = if length.is_finite() { length } else { 0.0 };
        let closed = !path.subpaths.is_empty() && path.subpaths.iter().all(|s| s.closed);
        let Some(b) = path.bounds() else { continue };
        c.count += 1;
        c.length += length;
        if closed {
            c.closed += 1;
            let a = po::area(&path, FillRule::EvenOdd);
            if a.is_finite() {
                c.enclosed_area += a;
            }
        } else {
            c.open += 1;
            c.open_ids.push(n.id);
        }
        let a = b.area();
        if a.is_finite() {
            c.pieces_area += a;
        }
        c.bounds = Some(c.bounds.map_or(b, |u| u.union(b)));
        if c.pieces.len() < MAX_PIECES {
            c.pieces.push((n.id, b, length, closed));
        }
        if n.appearance.items.iter().any(|i| matches!(i, AppearanceItem::Fill(f) if f.visible && !f.paint.is_none())) {
            c.filled.push(n.id);
        }
    }
    c
}

fn extra_names(p: &Value) -> Vec<String> {
    p.get("names").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).take(64).map(str::to_string).collect()
}

fn cut_stats_cmd(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "printshop.cutStats";
    let all = match str_param(p, "mode") {
        None | Some("cutContour") => false,
        Some("allStrokes") => true,
        Some(m) => return Err(bad(C, format!("mode `{m}`: cutContour or allStrokes"))),
    };
    let extra = extra_names(p);
    let st = s.doc()?;
    let mut leaves = vec![];
    for r in roots(st, bool_or(p, "selectionOnly", false)) {
        printable_leaves(r, &mut leaves);
    }
    Ok(cut_stats(&leaves, all, &extra).json())
}

// ---------- preflight ----------

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Level {
    Error,
    Warning,
    Info,
}

impl Level {
    fn id(self) -> &'static str {
        match self {
            Level::Error => "error",
            Level::Warning => "warning",
            Level::Info => "info",
        }
    }
}

struct Finding {
    code: &'static str,
    level: Level,
    message: String,
    message_bg: String,
    ids: Vec<u64>,
    values: Value,
}

#[derive(Default)]
struct Findings(Vec<Finding>);

impl Findings {
    fn add(&mut self, code: &'static str, level: Level, en: String, bg: String, ids: &[NodeId], values: Value) {
        self.0.push(Finding { code, level, message: en, message_bg: bg, ids: ids.iter().take(1000).map(|i| i.0).collect(), values });
    }
}

fn round1(v: f64) -> f64 {
    if v.is_finite() { (v * 10.0).round() / 10.0 } else { 0.0 }
}

/// The effective resolution (ppi, the lower of the two directions) an image prints at.
fn image_ppi(im: &vectorcraft_doc::ImageObject) -> Option<f64> {
    let [a, b, c, d, _, _] = im.xf.as_coeffs();
    let (sx, sy) = (a.hypot(b), c.hypot(d));
    if !(sx.is_finite() && sy.is_finite()) || sx <= 1e-12 || sy <= 1e-12 {
        return None;
    }
    Some((72.0 / sx).min(72.0 / sy))
}

fn is_white(c: &Color) -> bool {
    match *c {
        Color::Rgb { r, g, b } => r >= 0.999 && g >= 0.999 && b >= 0.999,
        Color::Cmyk { c, m, y, k } => c + m + y + k <= 0.001,
        Color::Gray { k } => k <= 0.001,
        Color::Lab { l, .. } => l >= 99.9,
    }
}

fn preflight(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "printshop.preflight";
    let Some(mut prof) = profile(str_param(p, "profile")) else {
        let ids: Vec<&str> = PROFILES.iter().map(|p| p.id).collect();
        return Err(bad(C, format!("profile `{}`: one of {}", str_param(p, "profile").unwrap_or_default(), ids.join(", "))));
    };
    check_numbers(p, &["minPpi", "recommendedPpi", "bleedMm"], C)?;
    if let Some(v) = p.get("minPpi").and_then(Value::as_f64).filter(|v| v.is_finite()) {
        prof.min_ppi = v.clamp(0.0, 10_000.0);
    }
    if let Some(v) = p.get("recommendedPpi").and_then(Value::as_f64).filter(|v| v.is_finite()) {
        prof.recommended_ppi = v.clamp(0.0, 10_000.0);
    }
    if let Some(v) = p.get("bleedMm").and_then(Value::as_f64).filter(|v| v.is_finite()) {
        prof.bleed_mm = v.clamp(0.0, 100.0);
    }
    let text = match str_param(p, "format") {
        None | Some("json") => false,
        Some("text") => true,
        Some(f) => return Err(bad(C, format!("format `{f}`: text or json"))),
    };
    let selection_only = bool_or(p, "selectionOnly", false);
    let st = s.doc()?;
    let d = &st.doc;
    let mut leaves = vec![];
    for r in roots(st, selection_only) {
        printable_leaves(r, &mut leaves);
    }
    let mut f = Findings::default();
    check(d, &prof, &leaves, &mut f);
    let cut = cut_stats(&leaves, prof.vector_only, &[]);
    cut_findings(&prof, &cut, &mut f);
    f.0.sort_by_key(|x| x.level);
    let errors = f.0.iter().filter(|x| x.level == Level::Error).count();
    let warnings = f.0.iter().filter(|x| x.level == Level::Warning).count();
    let (verdict, verdict_bg) = if errors > 0 {
        ("design", "Към дизайн: файлът има грешки и не е готов за предпечат")
    } else if warnings > 0 {
        ("review", "За преглед: файлът може да се печата, но има забележки")
    } else {
        ("ready", "Готов за предпечат")
    };
    let artboards: Vec<Value> = d
        .artboards
        .iter()
        .map(|a| json!({ "name": a.name, "widthMm": mm(a.rect.width()), "heightMm": mm(a.rect.height()), "areaM2": m2(a.rect.area()) }))
        .collect();
    let title = st.title();
    if text {
        return Ok(json!({ "text": report_bg(&title, &prof, verdict_bg, &f, &artboards, &cut) }));
    }
    Ok(json!({
        "profile": profile_json(&prof),
        "verdict": verdict,
        "verdictBg": verdict_bg,
        "errors": errors,
        "warnings": warnings,
        "findings": f.0.iter().map(|x| json!({
            "code": x.code, "level": x.level.id(), "message": x.message, "messageBg": x.message_bg, "ids": x.ids, "values": x.values,
        })).collect::<Vec<_>>(),
        "artboards": artboards,
        "cut": cut.json(),
    }))
}

/// The document-wide and per-object checks of profile `prof` over `leaves`.
fn check(d: &Document, prof: &Profile, leaves: &[&Node], f: &mut Findings) {
    if d.artboards.is_empty() {
        f.add("no_artboard", Level::Error, "The document has no artboard".into(), "Документът няма работна област (формат)".into(), &[], json!({}));
    }
    if leaves.is_empty() {
        f.add("empty", Level::Error, "Nothing printable in the document".into(), "Няма нищо за печат във файла".into(), &[], json!({}));
        return;
    }
    if prof.cmyk && d.color_mode == ColorMode::Rgb {
        f.add(
            "doc_rgb",
            Level::Warning,
            "The document is in RGB; this service prints CMYK (colours will shift)".into(),
            "Документът е в RGB, а печатът е CMYK – цветовете ще се променят".into(),
            &[],
            json!({}),
        );
    }
    // Bleed set up in File → Document Setup.
    let need = prof.bleed_mm * MM;
    let have = d.setup.bleed.iter().copied().fold(f64::INFINITY, f64::min);
    let have = if have.is_finite() { have } else { 0.0 };
    if need > 0.0 && have + 0.01 < need {
        f.add(
            "bleed_setup",
            Level::Error,
            format!("Bleed is {} mm; {} mm needed on every side", mm(have), prof.bleed_mm),
            format!("Обрезът (bleed) е {} мм – нужни са {} мм от всяка страна", mm(have), prof.bleed_mm),
            &[],
            json!({ "haveMm": mm(have), "needMm": prof.bleed_mm }),
        );
    }
    let mut at_edge = vec![];
    let mut off_board = vec![];
    let mut thin = vec![];
    let mut thinnest = f64::INFINITY;
    let mut rgb = vec![];
    let mut ink = vec![];
    let mut most_ink: f64 = 0.0;
    let mut white_op = vec![];
    let mut spots = BTreeSet::new();
    let mut live_text = vec![];
    let mut small_text = vec![];
    let mut missing_fonts = BTreeSet::new();
    let mut transparency = vec![];
    let mut images_vector = vec![];
    let db = vectorcraft_text::FontDb::global();
    for n in leaves {
        let cut = is_cut_node(n, &[]);
        // Placement against the artboards.
        if let Some(b) = n.visual_bounds() {
            let bleed = need.max(have);
            let on_board = d.artboards.iter().any(|a| {
                let r = a.rect.inflate(bleed, bleed);
                b.x1 > r.x0 && b.x0 < r.x1 && b.y1 > r.y0 && b.y0 < r.y1
            });
            if !on_board && !d.artboards.is_empty() {
                off_board.push(n.id);
            } else if need > 0.0 && !cut {
                // Art that reaches the trim edge but stops there: it shows a white hairline when cut.
                let touches = d.artboards.iter().any(|a| {
                    let r = a.rect;
                    let edge = |v: f64, e: f64| (v - e).abs() <= EDGE_TOL;
                    edge(b.x0, r.x0) || edge(b.x1, r.x1) || edge(b.y0, r.y0) || edge(b.y1, r.y1)
                });
                if touches {
                    at_edge.push(n.id);
                }
            }
        }
        // Strokes.
        for st in strokes(n) {
            if is_cut_paint(&st.paint, &[]) {
                continue;
            }
            if prof.min_stroke_pt > 0.0 && st.width < prof.min_stroke_pt {
                thin.push(n.id);
                thinnest = thinnest.min(st.width);
            }
        }
        // Colours.
        let mut is_rgb = false;
        let mut over_ink = false;
        for (paint, overprint) in paints(n) {
            let colors: Vec<Color> = match paint {
                Paint::Solid { color, .. } => vec![*color],
                Paint::Gradient(g) => g.gradient.stops.iter().map(|s| s.color).collect(),
                _ => vec![],
            };
            if let Paint::Solid { swatch: Some(name), .. } = paint
                && d.swatch(name).is_some_and(|w| w.spot)
                && !is_cut_paint(paint, &[])
            {
                spots.insert(name.clone());
            }
            for c in &colors {
                if matches!(c, Color::Rgb { .. }) && !matches!(paint, Paint::Solid { swatch: Some(_), .. }) {
                    is_rgb = true;
                }
                if let Color::Cmyk { c, m, y, k } = *c {
                    let total = f64::from(c + m + y + k) * 100.0;
                    if prof.max_ink > 0.0 && total > prof.max_ink + 0.5 {
                        over_ink = true;
                        most_ink = most_ink.max(total);
                    }
                }
                if overprint && is_white(c) {
                    white_op.push(n.id);
                }
            }
        }
        if is_rgb {
            rgb.push(n.id);
        }
        if over_ink {
            ink.push(n.id);
        }
        if n.opacity < 1.0 || n.blend != BlendMode::Normal || !n.appearance.effects.is_empty() {
            transparency.push(n.id);
        }
        match &n.kind {
            NodeKind::Image(im) => {
                if prof.vector_only {
                    images_vector.push(n.id);
                } else if let Some(ppi) = image_ppi(im)
                    && prof.min_ppi > 0.0
                {
                    let name = im.link.as_ref().map_or_else(|| n.display_name(), |l| l.name().to_string());
                    let v = json!({ "name": name, "ppi": round1(ppi), "widthPx": im.width, "heightPx": im.height, "minPpi": prof.min_ppi, "recommendedPpi": prof.recommended_ppi });
                    if ppi + 0.5 < prof.min_ppi {
                        f.add(
                            "image_ppi_low",
                            Level::Error,
                            format!("Image {name} prints at {} ppi (at least {} needed)", round1(ppi).round(), prof.min_ppi),
                            format!("Изображението {name} е {} ppi при печат (нужни са поне {})", round1(ppi).round(), prof.min_ppi),
                            &[n.id],
                            v,
                        );
                    } else if ppi + 0.5 < prof.recommended_ppi {
                        f.add(
                            "image_ppi",
                            Level::Warning,
                            format!("Image {name} prints at {} ppi ({} recommended)", round1(ppi).round(), prof.recommended_ppi),
                            format!("Изображението {name} е {} ppi при печат (препоръчително {})", round1(ppi).round(), prof.recommended_ppi),
                            &[n.id],
                            v,
                        );
                    }
                }
            }
            NodeKind::Text(t) => {
                live_text.push(n.id);
                for run in &t.runs {
                    let family = &run.style.font_family;
                    let style = &run.style.font_style;
                    if !matches!(db.resolve(family, style), Some((_, m)) if m != vectorcraft_text::FontMatch::Missing) {
                        missing_fonts.insert(format!("{family} {style}"));
                    }
                }
                let [a, b, c, dd, _, _] = t.xf.as_coeffs();
                let scale = ((a * dd - b * c).abs()).sqrt();
                let scale = if scale.is_finite() && scale > 0.0 { scale } else { 1.0 };
                if t.runs.iter().any(|r| r.style.size * scale < 5.0 && !r.text.trim().is_empty()) {
                    small_text.push(n.id);
                }
            }
            _ => {}
        }
    }
    if !images_vector.is_empty() {
        f.add(
            "image_in_vector",
            Level::Error,
            format!("{} raster image(s): this service needs vector paths only", images_vector.len()),
            format!("{} растерни изображения – за тази услуга са нужни само векторни контури", images_vector.len()),
            &images_vector,
            json!({ "count": images_vector.len() }),
        );
    }
    if !missing_fonts.is_empty() {
        let names: Vec<String> = missing_fonts.into_iter().collect();
        f.add(
            "font_missing",
            Level::Error,
            format!("Missing fonts: {}", names.join(", ")),
            format!("Липсващи шрифтове: {} – поискайте шрифта или текст в криви", names.join(", ")),
            &[],
            json!({ "fonts": names }),
        );
    }
    if !live_text.is_empty() {
        let (level, en, bg) = if prof.vector_only {
            (Level::Error, "live type must be converted to outlines for cutting", "текстът трябва да е преобразуван в криви за рязане")
        } else {
            (Level::Warning, "live type: convert to outlines before sending to prepress", "текстът не е в криви – преобразувайте преди предпечат")
        };
        f.add(
            "text_live",
            level,
            format!("{} text object(s): {en}", live_text.len()),
            format!("{} текстови обекта: {bg}", live_text.len()),
            &live_text,
            json!({ "count": live_text.len() }),
        );
    }
    if !small_text.is_empty() && !prof.vector_only {
        f.add(
            "text_small",
            Level::Warning,
            format!("{} text object(s) smaller than 5 pt", small_text.len()),
            format!("{} текста са под 5 pt – може да не се четат", small_text.len()),
            &small_text,
            json!({ "count": small_text.len(), "minPt": 5 }),
        );
    }
    if !thin.is_empty() {
        f.add(
            "stroke_thin",
            Level::Warning,
            format!(
                "{} object(s) with strokes thinner than {} pt (thinnest {} pt)",
                thin.len(),
                prof.min_stroke_pt,
                (thinnest * 100.0).round() / 100.0
            ),
            format!(
                "{} обекта с линии под {} pt (≈{} мм), най-тънката {} pt – може да не се отпечатат",
                thin.len(),
                prof.min_stroke_pt,
                mm(prof.min_stroke_pt),
                (thinnest * 100.0).round() / 100.0
            ),
            &thin,
            json!({ "count": thin.len(), "minPt": prof.min_stroke_pt, "thinnestPt": thinnest }),
        );
    }
    if prof.cmyk && !rgb.is_empty() {
        f.add(
            "rgb_color",
            Level::Warning,
            format!("{} object(s) in RGB colours", rgb.len()),
            format!("{} обекта са с RGB цветове – ще бъдат преобразувани в CMYK", rgb.len()),
            &rgb,
            json!({ "count": rgb.len() }),
        );
    }
    if !ink.is_empty() {
        f.add(
            "ink_total",
            Level::Warning,
            format!("{} object(s) above {}% total ink (up to {}%)", ink.len(), prof.max_ink, most_ink.round()),
            format!("{} обекта са над {}% общо мастило (до {}%) – риск от размазване", ink.len(), prof.max_ink, most_ink.round()),
            &ink,
            json!({ "count": ink.len(), "maxInk": prof.max_ink, "mostInk": most_ink.round() }),
        );
    }
    if !white_op.is_empty() {
        white_op.dedup();
        f.add(
            "white_overprint",
            Level::Error,
            format!("{} white object(s) set to overprint: they disappear in print", white_op.len()),
            format!("{} бели обекта са на overprint – при печат ще изчезнат", white_op.len()),
            &white_op,
            json!({ "count": white_op.len() }),
        );
    }
    if prof.cmyk && !spots.is_empty() {
        let names: Vec<String> = spots.into_iter().collect();
        f.add(
            "spot_color",
            Level::Info,
            format!("Spot colours (printed as CMYK): {}", names.join(", ")),
            format!("Спот (Pantone) цветове – ще се печатат като CMYK: {}", names.join(", ")),
            &[],
            json!({ "spots": names }),
        );
    }
    if !at_edge.is_empty() {
        f.add(
            "bleed_object",
            Level::Warning,
            format!("{} object(s) end exactly at the trim edge: extend them into the bleed", at_edge.len()),
            format!("{} обекта свършват точно на ръба на формата – издърпайте ги в обреза", at_edge.len()),
            &at_edge,
            json!({ "count": at_edge.len() }),
        );
    }
    if !off_board.is_empty() {
        f.add(
            "off_artboard",
            Level::Warning,
            format!("{} object(s) outside every artboard (they won't print)", off_board.len()),
            format!("{} обекта са извън работната област и няма да се отпечатат", off_board.len()),
            &off_board,
            json!({ "count": off_board.len() }),
        );
    }
    if !transparency.is_empty() && prof.id == "offset" {
        f.add(
            "transparency",
            Level::Info,
            format!("{} object(s) with transparency or effects (flattened on export)", transparency.len()),
            format!("{} обекта с прозрачност или ефекти – проверете PDF-а след експорт", transparency.len()),
            &transparency,
            json!({ "count": transparency.len() }),
        );
    }
}

/// Findings about the cut lines.
fn cut_findings(prof: &Profile, cut: &CutStats, f: &mut Findings) {
    if prof.cut_line && cut.count == 0 {
        f.add(
            "cut_missing",
            Level::Error,
            format!("No cut line: stroke the contour with the spot colour {CUT_CONTOUR} (Print Shop → Make Cut Contour)"),
            format!("Няма линия за рязане – контурът трябва да е в спот цвят {CUT_CONTOUR} (Печатница → Контур за рязане)"),
            &[],
            json!({}),
        );
    }
    if (prof.cut_line || prof.vector_only) && !cut.open_ids.is_empty() {
        f.add(
            "cut_open",
            Level::Warning,
            format!("{} open cut line(s): the piece won't come free", cut.open_ids.len()),
            format!("{} отворени линии за рязане – детайлът няма да се отдели", cut.open_ids.len()),
            &cut.open_ids,
            json!({ "count": cut.open_ids.len() }),
        );
    }
    if !prof.vector_only && !cut.filled.is_empty() {
        f.add(
            "cut_filled",
            Level::Warning,
            format!("{} cut line(s) have a fill: a cut line should be a stroke only", cut.filled.len()),
            format!("{} линии за рязане имат запълване – трябва да са само контур", cut.filled.len()),
            &cut.filled,
            json!({ "count": cut.filled.len() }),
        );
    }
    if cut.count > 0 {
        let lm = cut.length / MM / 1000.0;
        f.add(
            "cut_length",
            Level::Info,
            format!("Cut lines: {} ({} closed), {:.2} m in total", cut.count, cut.closed, lm),
            format!("Линии за рязане: {} ({} затворени), общо {:.2} л.м.", cut.count, cut.closed, lm),
            &[],
            json!({ "count": cut.count, "lengthM": (lm * 1000.0).round() / 1000.0 }),
        );
    }
}

/// The Bulgarian text report of a preflight.
fn report_bg(title: &str, prof: &Profile, verdict: &str, f: &Findings, artboards: &[Value], cut: &CutStats) -> String {
    let mut out = format!("ПРОВЕРКА НА ФАЙЛ: {title}\nУслуга: {}\nРезултат: {verdict}\n", prof.label_bg);
    out.push_str("\nФОРМАТ\n");
    for a in artboards {
        out.push_str(&format!(
            "{}: {} × {} мм ({} м²)\n",
            a["name"].as_str().unwrap_or_default(),
            a["widthMm"].as_f64().unwrap_or(0.0),
            a["heightMm"].as_f64().unwrap_or(0.0),
            a["areaM2"].as_f64().unwrap_or(0.0)
        ));
    }
    for (level, title) in [(Level::Error, "ГРЕШКИ"), (Level::Warning, "ЗАБЕЛЕЖКИ"), (Level::Info, "ИНФОРМАЦИЯ")] {
        let rows: Vec<&Finding> = f.0.iter().filter(|x| x.level == level).collect();
        if rows.is_empty() {
            continue;
        }
        out.push_str(&format!("\n{title}\n"));
        for r in rows {
            out.push_str(&format!("- {}\n", r.message_bg));
        }
    }
    if cut.count > 0 {
        out.push_str(&format!(
            "\nРЯЗАНЕ\nЛинии: {}, дължина: {:.2} л.м., площ на детайлите: {} м²\n",
            cut.count,
            cut.length / MM / 1000.0,
            m2(cut.pieces_area)
        ));
    }
    out
}

// ---------- Make Cut Contour ----------

/// The outline of `n` a contour goes around: paths and type as drawn, everything else (images,
/// symbols, blends, meshes…) by its bounds; groups by their parts.
fn outline_parts(n: &Node, out: &mut Vec<PathData>) {
    if !n.visible {
        return;
    }
    match &n.kind {
        NodeKind::Layer { children, .. } | NodeKind::Group { clip: false, children } => {
            for c in children {
                outline_parts(c, out);
            }
        }
        // A clip group shows only inside its clipping path (its first child).
        NodeKind::Group { clip: true, children } => {
            if let Some(clip) = children.first() {
                outline_parts(clip, out);
            }
        }
        NodeKind::Path { guide: true, .. } => {}
        _ => match geometry(n) {
            Some(p) if p.subpaths.iter().any(|s| s.closed) => out.push(p),
            // An open path is outlined by its stroke region: its bounds stand in.
            _ => {
                if let Some(b) = n.visual_bounds().filter(|b| b.area() > 0.0) {
                    out.push(rect_path(b));
                }
            }
        },
    }
}

fn make_cut_contour(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "printshop.cutContour.make";
    check_numbers(p, &["offset", "weight"], C)?;
    let offset_mm = p.get("offset").and_then(Value::as_f64).unwrap_or(2.0);
    if !offset_mm.is_finite() || offset_mm.abs() > 500.0 {
        return Err(bad(C, "offset: -500..500 mm"));
    }
    let weight = p.get("weight").and_then(Value::as_f64).unwrap_or(0.25);
    if !weight.is_finite() || weight <= 0.0 || weight > 50.0 {
        return Err(bad(C, "weight: above 0 and at most 50 pt"));
    }
    let name = str_param(p, "name").map(str::trim).filter(|n| !n.is_empty()).unwrap_or(CUT_CONTOUR).to_string();
    if name.chars().count() > 120 {
        return Err(bad(C, "name: at most 120 characters"));
    }
    let each = bool_or(p, "each", true);
    let on_layer = bool_or(p, "layer", true);
    let roots = selected_roots(s)?;
    let delta = offset_mm * MM;
    let magenta = Color::cmyk(0.0, 1.0, 0.0, 0.0);
    let (ids, layer) = s.edit("Make Cut Contour", |d, sel| {
        // The outlines to unite: one set per object, or one for all.
        let mut sets: Vec<(NodeId, Vec<PathData>)> = vec![];
        for id in &roots {
            let Some(n) = d.node(*id) else { continue };
            let mut parts = vec![];
            outline_parts(n, &mut parts);
            if parts.is_empty() {
                continue;
            }
            match sets.last_mut() {
                Some((_, all)) if !each => all.extend(parts),
                _ => sets.push((*id, parts)),
            }
        }
        if sets.is_empty() {
            return Err(bad(C, "select art with an outline"));
        }
        // The cut colour: a global spot swatch, created when missing.
        if d.swatch(&name).is_none() {
            d.swatches.push(Swatch { name: name.clone(), paint: Paint::solid(magenta), global: true, spot: true });
        }
        let color = d.global_color(&name).unwrap_or(magenta);
        let stroke = Paint::Solid { color, swatch: Some(name.clone()), tint: 1.0 };
        let mut made = vec![];
        for (src, parts) in sets {
            let refs: Vec<(&PathData, FillRule)> = parts.iter().map(|p| (p, FillRule::NonZero)).collect();
            let united = po::unite_all(&refs);
            let path = if delta.abs() < 1e-9 { united } else { po::offset_path(&united, delta, po::Join::Round, 4.0) };
            if path.is_empty() {
                continue;
            }
            let mut n = super::pathops::shape_node(d, path, None);
            n.appearance = Shared::new(Appearance::basic(Paint::None, stroke.clone(), weight));
            n.name = Some(name.clone());
            made.push((src, n));
        }
        if made.is_empty() {
            return Err(bad(C, "the offset leaves nothing (an inset larger than the art)"));
        }
        let mut ids = vec![];
        let mut layer = None;
        if on_layer {
            let found = d.layers.iter().find(|l| matches!(l.kind, NodeKind::Layer { .. }) && l.name.as_deref() == Some(CUT_CONTOUR)).map(|l| l.id);
            let lid = match found {
                Some(id) => id,
                None => {
                    let lid = d.alloc_id();
                    d.insert(None, usize::MAX, Node::layer(lid, CUT_CONTOUR, LayerColor::Preset(1)))?;
                    lid
                }
            };
            for (_, n) in made {
                ids.push(d.insert(Some(lid), usize::MAX, n)?);
            }
            layer = Some(lid);
        } else {
            let mut above = vec![];
            for (src, n) in made {
                ids.push(n.id);
                above.push((src, vec![n]));
            }
            d.insert_beside(above, false)?;
        }
        sel.set(ids.iter().copied());
        Ok((ids, layer))
    })?;
    Ok(json!({ "ids": ids.iter().map(|i| i.0).collect::<Vec<_>>(), "swatch": name, "layer": layer.map(|l| l.0) }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(units_mm: bool) -> Session {
        let mut s = Session::new();
        let mut p = json!({"width": 100.0 * MM, "height": 50.0 * MM});
        if units_mm {
            p["units"] = json!("Millimeters");
        }
        s.execute("file.new", &p).unwrap();
        s
    }

    fn codes(v: &Value) -> Vec<String> {
        v["findings"].as_array().unwrap().iter().map(|f| f["code"].as_str().unwrap().to_string()).collect()
    }

    #[test]
    fn profiles_are_listed_and_unknown_ones_rejected() {
        let mut s = session(false);
        let v = s.execute("printshop.profiles", &json!({})).unwrap();
        assert_eq!(v["profiles"].as_array().unwrap().len(), PROFILES.len());
        assert!(s.execute("printshop.preflight", &json!({"profile": "nope"})).is_err());
        assert!(s.execute("printshop.preflight", &json!({"format": "xml"})).is_err());
    }

    #[test]
    fn empty_document_goes_back_to_design() {
        let mut s = session(false);
        let v = s.execute("printshop.preflight", &json!({})).unwrap();
        assert_eq!(v["verdict"], "design");
        assert!(codes(&v).contains(&"empty".to_string()));
    }

    #[test]
    fn digital_print_wants_bleed_and_flags_art_at_the_trim_edge() {
        let mut s = session(false);
        // A background exactly the artboard's size: no bleed.
        s.execute("shape.rectangle", &json!({"x": 0, "y": 0, "width": 100.0 * MM, "height": 50.0 * MM})).unwrap();
        s.execute("paint.setStroke", &json!({"none": true})).unwrap();
        let v = s.execute("printshop.preflight", &json!({"profile": "digital"})).unwrap();
        let c = codes(&v);
        assert!(c.contains(&"bleed_setup".to_string()), "{c:?}");
        assert!(c.contains(&"bleed_object".to_string()), "{c:?}");
        assert_eq!(v["verdict"], "design");
        assert_eq!(v["artboards"][0]["widthMm"], 100.0);
        // Wide format needs no bleed.
        let v = s.execute("printshop.preflight", &json!({"profile": "wideFormat"})).unwrap();
        assert!(!codes(&v).iter().any(|c| c.starts_with("bleed")));
    }

    #[test]
    fn thin_strokes_and_off_artboard_art_are_warned_about() {
        let mut s = session(false);
        s.execute("shape.rectangle", &json!({"x": 20, "y": 20, "width": 40, "height": 40})).unwrap();
        s.execute("stroke.set", &json!({"weight": 0.1})).unwrap();
        s.execute("shape.rectangle", &json!({"x": 5000, "y": 5000, "width": 10, "height": 10})).unwrap();
        let v = s.execute("printshop.preflight", &json!({"profile": "wideFormat"})).unwrap();
        let c = codes(&v);
        assert!(c.contains(&"stroke_thin".to_string()) && c.contains(&"off_artboard".to_string()), "{c:?}");
        assert_eq!(v["verdict"], "review");
        let thin = v["findings"].as_array().unwrap().iter().find(|f| f["code"] == "stroke_thin").unwrap();
        assert!(thin["messageBg"].as_str().unwrap().contains("линии"));
    }

    #[test]
    fn stickers_need_a_cut_contour_and_make_one() {
        let mut s = session(false);
        let r = s.execute("shape.ellipse", &json!({"x": 20, "y": 20, "width": 100, "height": 60})).unwrap()["id"].clone();
        let v = s.execute("printshop.preflight", &json!({"profile": "sticker"})).unwrap();
        assert!(codes(&v).contains(&"cut_missing".to_string()));
        s.execute("select.set", &json!({"ids": [r]})).unwrap();
        let made = s.execute("printshop.cutContour.make", &json!({"offset": 2})).unwrap();
        assert_eq!(made["swatch"], CUT_CONTOUR);
        assert_eq!(made["ids"].as_array().unwrap().len(), 1);
        // One undo step, a spot swatch and a CutContour layer.
        let st = s.doc().unwrap();
        assert!(st.doc.swatch(CUT_CONTOUR).is_some_and(|w| w.spot && w.global));
        assert!(st.doc.layers.iter().any(|l| l.name.as_deref() == Some(CUT_CONTOUR)));
        let v = s.execute("printshop.preflight", &json!({"profile": "sticker"})).unwrap();
        assert!(!codes(&v).contains(&"cut_missing".to_string()), "{:?}", codes(&v));
        // The contour is 2 mm outside the ellipse: 100 pt + 2 × 2 mm wide.
        let cut = s.execute("printshop.cutStats", &json!({})).unwrap();
        assert_eq!(cut["count"], 1);
        assert_eq!(cut["closed"], 1);
        let w = cut["pieces"][0]["widthMm"].as_f64().unwrap();
        assert!((w - (100.0 / MM + 4.0)).abs() < 0.2, "{w}");
        assert!(cut["lengthMm"].as_f64().unwrap() > 0.0);
        s.execute("edit.undo", &json!({})).unwrap();
        assert!(s.doc().unwrap().doc.swatch(CUT_CONTOUR).is_none());
    }

    #[test]
    fn one_contour_around_the_whole_selection() {
        let mut s = session(false);
        let a = s.execute("shape.rectangle", &json!({"x": 10, "y": 10, "width": 20, "height": 20})).unwrap()["id"].clone();
        let b = s.execute("shape.rectangle", &json!({"x": 28, "y": 10, "width": 20, "height": 20})).unwrap()["id"].clone();
        s.execute("select.set", &json!({"ids": [a, b]})).unwrap();
        let each = s.execute("printshop.cutContour.make", &json!({"offset": 1})).unwrap();
        assert_eq!(each["ids"].as_array().unwrap().len(), 2);
        s.execute("edit.undo", &json!({})).unwrap();
        s.execute("select.set", &json!({"ids": [a, b]})).unwrap();
        let all = s.execute("printshop.cutContour.make", &json!({"offset": 1, "each": false, "layer": false})).unwrap();
        assert_eq!(all["ids"].as_array().unwrap().len(), 1);
        assert!(all["layer"].is_null());
    }

    #[test]
    fn laser_needs_vectors_and_measures_every_stroke() {
        let mut s = session(false);
        s.execute("shape.line", &json!({"x1": 0, "y1": 10, "x2": 72, "y2": 10})).unwrap();
        s.execute("shape.rectangle", &json!({"x": 10, "y": 20, "width": 72, "height": 72})).unwrap();
        s.execute("text.create", &json!({"x": 10, "y": 120, "text": "Тонекс"})).unwrap();
        let v = s.execute("printshop.preflight", &json!({"profile": "laser"})).unwrap();
        let text = v["findings"].as_array().unwrap().iter().find(|f| f["code"] == "text_live").unwrap();
        assert_eq!(text["level"], "error");
        assert!(codes(&v).contains(&"cut_open".to_string()));
        let all = s.execute("printshop.cutStats", &json!({"mode": "allStrokes"})).unwrap();
        // The line (1 in = 25.4 mm) and the square (4 in) at least; the type's outline too.
        assert!(all["lengthMm"].as_f64().unwrap() >= 25.4 * 5.0 - 0.1, "{all}");
        assert!(s.execute("printshop.cutStats", &json!({"mode": "nope"})).is_err());
    }

    #[test]
    fn text_report_is_bulgarian() {
        let mut s = session(false);
        s.execute("shape.rectangle", &json!({"x": 20, "y": 20, "width": 40, "height": 40})).unwrap();
        let t = s.execute("printshop.preflight", &json!({"profile": "offset", "format": "text"})).unwrap();
        let t = t["text"].as_str().unwrap();
        assert!(t.contains("ПРОВЕРКА НА ФАЙЛ") && t.contains("Офсетов печат"), "{t}");
    }

    #[test]
    fn cut_names_are_matched_loosely() {
        let p = |n: &str| Paint::Solid { color: Color::BLACK, swatch: Some(n.into()), tint: 1.0 };
        assert!(is_cut_paint(&p("CutContour"), &[]));
        assert!(is_cut_paint(&p("Thru-Cut"), &[]));
        assert!(is_cut_paint(&p("cut contour"), &[]));
        assert!(!is_cut_paint(&p("Pantone 485"), &[]));
        assert!(is_cut_paint(&p("Zund"), &["zund".into()]));
        assert!(!is_cut_paint(&Paint::solid(Color::BLACK), &[]));
    }
}
