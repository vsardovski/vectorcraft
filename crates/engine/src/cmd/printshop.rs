//! Print-shop commands of the Тонекс edition, modelled on CorelDRAW's public documentation:
//!
//! - `printshop.stepAndRepeat`: Edit › Step and Repeat, after
//!   https://product.corel.com/help/CorelDRAW/540111130/Main/EN/Documentation/CorelDRAW-Copy-duplicate-and-delete-objects.html
//!   (horizontal and vertical settings, each "No offset", "Offset" or "Spacing between objects",
//!   with a distance and a direction, and a number of copies).
//! - `printshop.cutContour`: a cut line for print-and-cut plotters: a hairline in a spot colour
//!   named `CutContour` (the name RIPs such as Roland VersaWorks look for), overprinting, around
//!   the selection's outline or its box.
//! - `printshop.align.*`, `printshop.distribute.*`, `printshop.centerToPage`: the one-key Align
//!   and Distribute commands, after the default keys in the public list of shortcuts
//!   (https://community.coreldraw.com/collaborate/w/howto/8/complete-list-of-keyboard-shortcuts).
//! - `printshop.convertToCurves`: Object › Convert to Curves (type and live shapes to paths).
//!
//! Only public documentation was used; nothing was observed in, measured from or copied out of
//! CorelDRAW itself (see `docs/corel-edition.md`).

use serde_json::{Value, json};
use vectorcraft_color::{Color, Paint, Swatch};
use vectorcraft_doc::{Appearance, AppearanceItem, NodeId, NodeKind, StrokeLayer};
use vectorcraft_geom::Rect;

use super::edit::selected_roots;
use super::*;

/// The most copies one Step and Repeat makes.
pub const MAX_COPIES: u64 = 1000;
/// The spot colour a cut line is painted with by default.
pub const CUT_CONTOUR: &str = "CutContour";
/// The cut line's default weight (pt): a hairline.
const CUT_WEIGHT: f64 = 0.25;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            "printshop.stepAndRepeat",
            "Step and Repeat…",
            ["Edit"],
            None,
            "{copies: 1..1000, horizontal?: {mode?: \"none\"|\"offset\"|\"spacing\" (default none), distance?: pt (offset: signed, + is right; spacing: the gap between copies' boxes), direction?: \"right\"|\"left\" (spacing; default right)}, vertical?: {mode?, distance?: pt (offset: signed, + is up), direction?: \"up\"|\"down\" (spacing; default down)}, hMode?, hDistance?, hDirection?, vMode?, vDistance?, vDirection? (the same, flat, as the dialog gives them; the nested objects win), ids?: [id…] (default: the selection)} make `copies` copies of the selection, each stepped from the one before (the reference app's Step and Repeat: Offset moves by the distance, Spacing between objects by the selection's width or height plus the distance), each copy above the one before, in one undo step; the copies are selected → {ids}",
            has_selection,
            step_and_repeat
        ),
        cmd!(
            "printshop.cutContour",
            "Add Cut Contour",
            ["Object"],
            None,
            "{shape?: \"outline\"|\"box\" (default outline: around the united outline of the selection, type outlined first; box: around its visible bounds), offset?: pt (outwards; negative insets; default 0), radius?: pt (box corners; default 0), name?: spot colour name (default \"CutContour\"; made as a 100% magenta spot colour when the document has no swatch of that name), weight?: pt (default 0.25), ids?: [id…] (default: the selection)} add a cut line for print-and-cut: an unfilled path stroked with the spot colour, overprinting, above the art, in one undo step; the cut line is selected → {id, name}",
            has_selection,
            cut_contour
        ),
        // The one-key Align and Distribute commands of CorelDRAW's default shortcuts (L, R, T,
        // B, C, E, P and Shift with them), so a single key can run each one without parameters.
        cmd!(
            "printshop.align.left",
            "Align Left",
            ["Object", "Align and Distribute"],
            None,
            "{} align the selection's left edges (to the key object when there is one, else to the selection's bounds)",
            has_selection,
            |s, _| s.execute("object.align", &json!({"horizontal": "left"}))
        ),
        cmd!(
            "printshop.align.right",
            "Align Right",
            ["Object", "Align and Distribute"],
            None,
            "{} align the selection's right edges",
            has_selection,
            |s, _| s.execute("object.align", &json!({"horizontal": "right"}))
        ),
        cmd!(
            "printshop.align.top",
            "Align Top",
            ["Object", "Align and Distribute"],
            None,
            "{} align the selection's top edges",
            has_selection,
            |s, _| s.execute("object.align", &json!({"vertical": "top"}))
        ),
        cmd!(
            "printshop.align.bottom",
            "Align Bottom",
            ["Object", "Align and Distribute"],
            None,
            "{} align the selection's bottom edges",
            has_selection,
            |s, _| s.execute("object.align", &json!({"vertical": "bottom"}))
        ),
        cmd!(
            "printshop.align.centerHorizontal",
            "Align Centers Vertically",
            ["Object", "Align and Distribute"],
            None,
            "{} line the selection's centers up on one vertical line (the C key of the Print Shop shortcuts)",
            has_selection,
            |s, _| s.execute("object.align", &json!({"horizontal": "center"}))
        ),
        cmd!(
            "printshop.align.centerVertical",
            "Align Centers Horizontally",
            ["Object", "Align and Distribute"],
            None,
            "{} line the selection's centers up on one horizontal line (the E key of the Print Shop shortcuts)",
            has_selection,
            |s, _| s.execute("object.align", &json!({"vertical": "center"}))
        ),
        cmd!(
            "printshop.centerToPage",
            "Center to Page",
            ["Object", "Align and Distribute"],
            None,
            "{} center the selection on the artboard under it, both ways, as one",
            has_selection,
            |s, _| s.execute("object.align", &json!({"horizontal": "center", "vertical": "center", "to": "artboard"}))
        ),
        cmd!(
            "printshop.distribute.left",
            "Distribute Left",
            ["Object", "Align and Distribute"],
            None,
            "{} space the selection's left edges evenly (three objects or more)",
            has_selection,
            |s, _| s.execute("object.distribute", &json!({"horizontal": "left"}))
        ),
        cmd!(
            "printshop.distribute.right",
            "Distribute Right",
            ["Object", "Align and Distribute"],
            None,
            "{} space the selection's right edges evenly",
            has_selection,
            |s, _| s.execute("object.distribute", &json!({"horizontal": "right"}))
        ),
        cmd!(
            "printshop.distribute.top",
            "Distribute Top",
            ["Object", "Align and Distribute"],
            None,
            "{} space the selection's top edges evenly",
            has_selection,
            |s, _| s.execute("object.distribute", &json!({"vertical": "top"}))
        ),
        cmd!(
            "printshop.distribute.bottom",
            "Distribute Bottom",
            ["Object", "Align and Distribute"],
            None,
            "{} space the selection's bottom edges evenly",
            has_selection,
            |s, _| s.execute("object.distribute", &json!({"vertical": "bottom"}))
        ),
        cmd!(
            "printshop.distribute.centerHorizontal",
            "Distribute Centers Horizontally",
            ["Object", "Align and Distribute"],
            None,
            "{} space the selection's centers evenly from left to right",
            has_selection,
            |s, _| s.execute("object.distribute", &json!({"horizontal": "center"}))
        ),
        cmd!(
            "printshop.distribute.centerVertical",
            "Distribute Centers Vertically",
            ["Object", "Align and Distribute"],
            None,
            "{} space the selection's centers evenly from top to bottom",
            has_selection,
            |s, _| s.execute("object.distribute", &json!({"vertical": "center"}))
        ),
        cmd!(
            "printshop.convertToCurves",
            "Convert to Curves",
            ["Object"],
            None,
            "{} turn the selected type and live shapes into plain editable paths (type → outlines, live shapes → paths, effects baked), leaving fills and strokes as they are, in one undo step → {ids}",
            has_selection,
            |s, _| s.execute("object.expand", &json!({"object": true, "fill": false, "stroke": false}))
        ),
    ]
}

/// The step of one axis of Step and Repeat, in document units (y down), for a selection `size`
/// wide (horizontal) or high (vertical).
fn axis_step(cmd: &str, p: Option<&Value>, size: f64, horizontal: bool) -> Result<f64> {
    let Some(p) = p else { return Ok(0.0) };
    if p.is_null() {
        return Ok(0.0);
    }
    if !p.is_object() {
        return Err(bad(cmd, "`horizontal` and `vertical` are objects"));
    }
    check_numbers(p, &["distance"], cmd)?;
    let distance = f64_or(p, "distance", 0.0);
    if !distance.is_finite() || distance.abs() > 1e7 {
        return Err(bad(cmd, "`distance` is out of range"));
    }
    let mode = p.get("mode").and_then(Value::as_str).unwrap_or("none");
    let direction = p.get("direction").and_then(Value::as_str);
    match mode {
        "none" => Ok(0.0),
        // Up is positive in CorelDRAW's offsets; document y runs down.
        "offset" => Ok(if horizontal { distance } else { -distance }),
        "spacing" => {
            let sign = match (horizontal, direction) {
                (true, None | Some("right")) => 1.0,
                (true, Some("left")) => -1.0,
                (false, None | Some("down")) => 1.0,
                (false, Some("up")) => -1.0,
                (_, Some(d)) => return Err(bad(cmd, format!("unknown direction `{d}`"))),
            };
            Ok(sign * (size + distance))
        }
        m => Err(bad(cmd, format!("unknown mode `{m}` (none, offset or spacing)"))),
    }
}

/// One axis of Step and Repeat: the nested object (`horizontal: {mode, distance, direction}`), or
/// the flat fields the dialog edits (`hMode`, `hDistance`, `hDirection`), nested first.
fn axis_params(p: &Value, nested: &str, prefix: &str) -> Option<Value> {
    if let Some(v) = p.get(nested) {
        return Some(v.clone());
    }
    let mut o = serde_json::Map::new();
    for (flat, key) in [("Mode", "mode"), ("Distance", "distance"), ("Direction", "direction")] {
        if let Some(v) = p.get(format!("{prefix}{flat}")) {
            o.insert(key.into(), v.clone());
        }
    }
    (!o.is_empty()).then_some(Value::Object(o))
}

fn roots_of_params(s: &Session, p: &Value) -> Result<Vec<NodeId>> {
    match p.get("ids").and_then(Value::as_array) {
        Some(a) => {
            let ids: Vec<NodeId> = a.iter().filter_map(Value::as_u64).map(NodeId).collect();
            let d = &s.doc()?.doc;
            if ids.is_empty() || ids.iter().any(|id| d.node(*id).is_none()) {
                return Err(bad("printshop", "`ids` names objects that aren't in the document"));
            }
            Ok(super::edit::outermost(d, ids))
        }
        None => selected_roots(s),
    }
}

/// Run `f` as one undo step (nested commands included), unless a group is open already.
fn one_step<T>(s: &mut Session, label: &str, f: impl FnOnce(&mut Session) -> Result<T>) -> Result<T> {
    let mine = s.active().is_some_and(|d| d.undo_group.is_none());
    if mine {
        s.begin_named_undo_group(Some(label.to_string()));
    }
    let r = f(s);
    if mine {
        // A failure undoes what the steps before it did.
        s.end_undo_group(r.is_err());
    }
    r
}

fn step_and_repeat(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "printshop.stepAndRepeat";
    let copies = p.get("copies").and_then(Value::as_u64).ok_or_else(|| bad(C, "`copies` is a whole number from 1"))?;
    if copies == 0 || copies > MAX_COPIES {
        return Err(bad(C, format!("`copies` is from 1 to {MAX_COPIES}")));
    }
    let roots = roots_of_params(s, p)?;
    let b = s.doc()?.doc.bounds_of(&roots, true).ok_or_else(|| bad(C, "the selection has no bounds"))?;
    let dx = axis_step(C, axis_params(p, "horizontal", "h").as_ref(), b.width(), true)?;
    let dy = axis_step(C, axis_params(p, "vertical", "v").as_ref(), b.height(), false)?;
    let xf = vectorcraft_geom::Affine::translate((dx, dy));
    one_step(s, "Step and Repeat", |s| {
        let mut last = roots;
        let mut all = Vec::new();
        for _ in 0..copies {
            // Each copy is made from the one before, so it lands above it.
            last = s.edit("Step and Repeat", |d, sel| super::edit::duplicate_in(d, sel, &last, xf))?;
            all.extend(last.iter().copied());
        }
        s.edit("Step and Repeat", |_, sel| {
            sel.set(all.iter().copied());
            Ok(())
        })?;
        Ok(json!({ "ids": all.iter().map(|i| i.0).collect::<Vec<_>>() }))
    })
}

/// The cut line's stroke: the spot colour `name` at 100 %, overprinting.
fn cut_stroke(color: Color, name: &str, weight: f64) -> Appearance {
    let mut stroke = StrokeLayer::new(Paint::Solid { color, swatch: Some(name.to_string()), tint: 1.0 }, weight);
    stroke.overprint = true;
    Appearance { items: vec![AppearanceItem::Stroke(stroke)], ..Appearance::default() }
}

fn cut_contour(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "printshop.cutContour";
    check_numbers(p, &["offset", "radius", "weight"], C)?;
    let offset = f64_or(p, "offset", 0.0);
    let radius = f64_or(p, "radius", 0.0).max(0.0);
    let weight = f64_or(p, "weight", CUT_WEIGHT);
    if !(offset.is_finite() && offset.abs() <= 1e5 && radius.is_finite() && radius <= 1e5 && weight.is_finite() && weight > 0.0 && weight <= 100.0) {
        return Err(bad(C, "`offset`, `radius` or `weight` is out of range"));
    }
    let name = p.get("name").and_then(Value::as_str).map(str::trim).unwrap_or(CUT_CONTOUR).to_string();
    if name.is_empty() || name.chars().count() > 200 {
        return Err(bad(C, "`name` is 1 to 200 characters"));
    }
    let shape = p.get("shape").and_then(Value::as_str).unwrap_or("outline");
    if shape != "outline" && shape != "box" {
        return Err(bad(C, format!("unknown shape `{shape}` (outline or box)")));
    }
    let roots = roots_of_params(s, p)?;
    one_step(s, "Add Cut Contour", |s| {
        // The spot colour, made once.
        let color = match s.doc()?.doc.swatch(&name).map(|w| w.paint.clone()) {
            Some(Paint::Solid { color, .. }) => color,
            Some(_) => return Err(bad(C, format!("swatch `{name}` isn't a solid colour"))),
            None => {
                let color = Color::cmyk(0.0, 1.0, 0.0, 0.0);
                let swatch = Swatch { name: name.clone(), paint: Paint::solid(color), global: true, spot: true };
                s.edit("Add Cut Contour", |d, _| {
                    d.swatches.push(swatch);
                    Ok(())
                })?;
                color
            }
        };
        let line = if shape == "box" {
            let b = s.doc()?.doc.bounds_of(&roots, true).ok_or_else(|| bad(C, "the selection has no bounds"))?;
            let r = Rect::new(b.x0 - offset, b.y0 - offset, b.x1 + offset, b.y1 + offset);
            if r.width() <= 0.0 || r.height() <= 0.0 {
                return Err(bad(C, "the inset leaves nothing to cut"));
            }
            let made = s.execute("shape.rectangle", &json!({"x": r.x0, "y": r.y0, "width": r.width(), "height": r.height(), "radius": radius}))?;
            made.get("id").and_then(Value::as_u64).map(NodeId).ok_or_else(|| bad(C, "the rectangle wasn't made"))?
        } else {
            outline_line(s, &roots, offset)?
        };
        let appearance = cut_stroke(color, &name, weight);
        s.edit("Add Cut Contour", |d, sel| {
            let n = d.node_mut(line).ok_or_else(|| bad(C, "the cut line went missing"))?;
            *n.appearance = appearance;
            n.name = Some(name.clone());
            sel.set([line]);
            Ok(())
        })?;
        Ok(json!({ "id": line.0, "name": name }))
    })
}

/// A path along the united outline of `roots`, offset by `offset`: their copies, type outlined,
/// united, then offset (the unoffset union removed).
fn outline_line(s: &mut Session, roots: &[NodeId], offset: f64) -> Result<NodeId> {
    const C: &str = "printshop.cutContour";
    let ids: Vec<u64> = roots.iter().map(|i| i.0).collect();
    let copies = s.execute("edit.duplicate", &json!({ "ids": ids }))?;
    let mut copies: Vec<u64> = copies.get("ids").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_u64).collect()).unwrap_or_default();
    // Type is cut along its glyphs.
    let has_type = {
        let d = &s.doc()?.doc;
        copies.iter().any(|id| d.node(NodeId(*id)).is_some_and(|n| contains_type(n)))
    };
    if has_type {
        s.execute("select.set", &json!({ "ids": copies }))?;
        let out = s.execute("type.createOutlines", &json!({}))?;
        if let Some(a) = out.get("ids").and_then(Value::as_array) {
            let made: Vec<u64> = a.iter().filter_map(Value::as_u64).collect();
            if !made.is_empty() {
                let d = &s.doc()?.doc;
                copies.retain(|id| d.node(NodeId(*id)).is_some());
                let new: Vec<u64> = made.into_iter().filter(|id| !copies.contains(id)).collect();
                copies.extend(new);
            }
        }
    }
    let united = s.execute("object.pathfinder.unite", &json!({ "ids": copies }))?;
    let united: Vec<u64> = united.get("ids").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_u64).collect()).unwrap_or_default();
    let base = united.first().copied().ok_or_else(|| bad(C, "the selection has no outline to cut along"))?;
    if offset == 0.0 {
        return Ok(NodeId(base));
    }
    s.execute("select.set", &json!({ "ids": [base] }))?;
    let made = s.execute("object.path.offsetPath", &json!({ "offset": offset, "joins": "round" }))?;
    let line = made
        .get("ids")
        .and_then(Value::as_array)
        .and_then(|a| a.first())
        .and_then(Value::as_u64)
        .ok_or_else(|| bad(C, "the offset left nothing to cut"))?;
    s.execute("edit.clear", &json!({ "ids": [base] }))?;
    Ok(NodeId(line))
}

fn contains_type(n: &vectorcraft_doc::Node) -> bool {
    matches!(n.kind, NodeKind::Text(_)) || n.children().is_some_and(|ch| ch.iter().any(|c| contains_type(c)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(s: &mut Session, x: f64) -> u64 {
        s.execute("shape.rectangle", &json!({"x": x, "y": 0, "width": 20, "height": 10})).unwrap()["id"].as_u64().unwrap()
    }

    fn session() -> Session {
        let mut s = Session::new();
        s.execute("file.new", &json!({"width": 400, "height": 300})).unwrap();
        s
    }

    #[test]
    fn spacing_steps_by_the_width_plus_the_gap_and_is_one_undo_step() {
        let mut s = session();
        let id = rect(&mut s, 0.0);
        s.execute("select.set", &json!({"ids": [id]})).unwrap();
        let undo = s.active().unwrap().history.undo.len();
        let out = s.execute("printshop.stepAndRepeat", &json!({"copies": 3, "horizontal": {"mode": "spacing", "distance": 5}})).unwrap();
        let ids: Vec<NodeId> = out["ids"].as_array().unwrap().iter().map(|v| NodeId(v.as_u64().unwrap())).collect();
        assert_eq!(ids.len(), 3);
        let d = &s.active().unwrap().doc;
        let xs: Vec<f64> = ids.iter().map(|i| d.bounds_of(&[*i], false).unwrap().x0).collect();
        // Spacing is measured between the visible edges: 20 wide plus the 1 pt stroke, plus 5.
        assert_eq!(xs, vec![26.0, 52.0, 78.0]);
        assert_eq!(s.active().unwrap().history.undo.len(), undo + 1);
        s.execute("edit.undo", &json!({})).unwrap();
        assert!(ids.iter().all(|i| s.active().unwrap().doc.node(*i).is_none()));
    }

    #[test]
    fn offset_up_is_negative_document_y_and_bad_counts_fail() {
        let mut s = session();
        let id = rect(&mut s, 0.0);
        s.execute("select.set", &json!({"ids": [id]})).unwrap();
        let out = s.execute("printshop.stepAndRepeat", &json!({"copies": 1, "vertical": {"mode": "offset", "distance": 7}})).unwrap();
        let c = NodeId(out["ids"][0].as_u64().unwrap());
        assert_eq!(s.active().unwrap().doc.bounds_of(&[c], false).unwrap().y0, -7.0);
        assert!(s.execute("printshop.stepAndRepeat", &json!({"copies": 0})).is_err());
        assert!(s.execute("printshop.stepAndRepeat", &json!({"copies": 5000})).is_err());
        assert!(s.execute("printshop.stepAndRepeat", &json!({"copies": 1, "horizontal": {"mode": "diagonal"}})).is_err());
    }

    #[test]
    fn a_box_cut_contour_is_a_spot_hairline_that_overprints() {
        let mut s = session();
        let id = rect(&mut s, 0.0);
        s.execute("select.set", &json!({"ids": [id]})).unwrap();
        let out = s.execute("printshop.cutContour", &json!({"shape": "box", "offset": 2})).unwrap();
        let line = NodeId(out["id"].as_u64().unwrap());
        let d = &s.active().unwrap().doc;
        let w = d.swatch(CUT_CONTOUR).unwrap();
        assert!(w.spot && w.global);
        let n = d.node(line).unwrap();
        let st = n.appearance.stroke().unwrap();
        assert!(st.overprint);
        assert_eq!(st.width, CUT_WEIGHT);
        assert!(matches!(&st.paint, Paint::Solid { swatch: Some(sw), .. } if sw == CUT_CONTOUR));
        assert!(n.appearance.fill().is_none());
        let b = d.bounds_of(&[line], false).unwrap();
        // Around the visible bounds (the 1 pt stroke takes in half a point), 2 pt out.
        assert_eq!((b.x0, b.y0, b.x1, b.y1), (-2.5, -2.5, 22.5, 12.5));
        // A second cut line reuses the swatch.
        s.execute("select.set", &json!({"ids": [id]})).unwrap();
        s.execute("printshop.cutContour", &json!({"shape": "box"})).unwrap();
        assert_eq!(s.active().unwrap().doc.swatches_iter().filter(|w| w.name == CUT_CONTOUR).count(), 1);
    }

    #[test]
    fn the_dialogs_flat_fields_step_as_the_nested_ones_do() {
        let mut s = session();
        let id = rect(&mut s, 0.0);
        s.execute("select.set", &json!({"ids": [id]})).unwrap();
        let out = s
            .execute("printshop.stepAndRepeat", &json!({"copies": 2, "hMode": "spacing", "hDistance": 5, "vMode": "offset", "vDistance": 3}))
            .unwrap();
        let d = &s.active().unwrap().doc;
        let b: Vec<(f64, f64)> = out["ids"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| d.bounds_of(&[NodeId(v.as_u64().unwrap())], false).unwrap())
            .map(|b| (b.x0, b.y0))
            .collect();
        assert_eq!(b, vec![(26.0, -3.0), (52.0, -6.0)]);
    }

    #[test]
    fn one_key_align_and_center_to_page() {
        let mut s = session();
        let a = rect(&mut s, 0.0);
        let b = s.execute("shape.rectangle", &json!({"x": 100, "y": 50, "width": 40, "height": 30})).unwrap()["id"].as_u64().unwrap();
        s.execute("select.set", &json!({"ids": [a, b]})).unwrap();
        s.execute("printshop.align.left", &json!({})).unwrap();
        let x0 = |s: &Session, id: u64| s.active().unwrap().doc.bounds_of(&[NodeId(id)], false).unwrap().x0;
        assert_eq!(x0(&s, a), x0(&s, b));
        s.execute("select.set", &json!({"ids": [b]})).unwrap();
        s.execute("printshop.centerToPage", &json!({})).unwrap();
        let bb = s.active().unwrap().doc.bounds_of(&[NodeId(b)], false).unwrap();
        assert_eq!((bb.center().x, bb.center().y), (200.0, 150.0));
        s.execute("select.set", &json!({"ids": []})).unwrap();
        assert!(s.execute("printshop.align.top", &json!({})).is_err());
    }

    #[test]
    fn convert_to_curves_turns_type_into_paths() {
        let mut s = session();
        let t = s.execute("text.create", &json!({"x": 10, "y": 40, "text": "Hi"})).unwrap()["id"].as_u64().unwrap();
        s.execute("select.set", &json!({"ids": [t]})).unwrap();
        s.execute("printshop.convertToCurves", &json!({})).unwrap();
        let d = &s.active().unwrap().doc;
        assert!(d.node(NodeId(t)).is_none() || !contains_type(d.node(NodeId(t)).unwrap()));
    }

    #[test]
    fn an_outline_cut_contour_follows_the_united_shapes_and_leaves_the_art() {
        let mut s = session();
        let a = rect(&mut s, 0.0);
        let b = rect(&mut s, 10.0);
        s.execute("select.set", &json!({"ids": [a, b]})).unwrap();
        let undo = s.active().unwrap().history.undo.len();
        let out = s.execute("printshop.cutContour", &json!({"offset": 1})).unwrap();
        let line = NodeId(out["id"].as_u64().unwrap());
        let d = &s.active().unwrap().doc;
        assert!(d.node(NodeId(a)).is_some() && d.node(NodeId(b)).is_some());
        let bb = d.bounds_of(&[line], false).unwrap();
        assert!((bb.x0 + 1.0).abs() < 0.01 && (bb.x1 - 31.0).abs() < 0.01, "{bb:?}");
        assert_eq!(s.active().unwrap().history.undo.len(), undo + 1);
    }
}
