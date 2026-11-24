//! Gridfinity plugin for SolidCraft: entries `bin` and `baseplate` build
//! ordinary, editable SolidCraft features through the host's command API.

pub mod drawer;
pub mod geometry;
pub mod panel;
pub mod plate;

use drawer::{drawer_commands, DrawerOptions, Filler, LAYOUT_GAP};
use geometry::{baseplate_commands, bin_commands, bin_inside_top, bin_size, bin_wall, label_areas, label_size, text_depth, text_width, BaseplateOptions, BinOptions, FLOOR_Z, HALF_PITCH, PITCH, UNIT_H};
use serde_json::{Map, Value};
use serde_json::json;

#[cfg(target_arch = "wasm32")]
mod bindings {
    wit_bindgen::generate!({ world: "gridfinity", path: "wit" });
}

/// Largest bin or baseplate side: 12 full cells (504 mm), and never more than 24 cells.
const MAX_SIDE: f64 = 504.0;
const MAX_CELLS: u64 = 24;
/// Custom cell sizes allowed (mm).
const CUSTOM_MIN: f64 = 15.0;
const CUSTOM_MAX: f64 = 100.0;
const MAX_UNITS: u64 = 20;
const MAX_DIVIDERS: u64 = 10;

fn get_u(p: &Map<String, Value>, key: &str, default: u64, min: u64, max: u64) -> Result<u32, String> {
    match p.get(key) {
        None | Some(Value::Null) => Ok(default as u32),
        Some(v) => match v.as_u64() {
            Some(n) if (min..=max).contains(&n) => Ok(n as u32),
            _ => Err(format!("{key} must be a whole number from {min} to {max}")),
        },
    }
}

/// The first of `keys` that is present: the dialog's name, then older/script names.
fn pick<'a>(p: &'a Map<String, Value>, keys: &[&'a str]) -> (&'a str, Option<&'a Value>) {
    for k in keys {
        if let Some(v) = p.get(*k).filter(|v| !v.is_null()) {
            return (k, Some(v));
        }
    }
    (keys[0], None)
}

fn get_b(p: &Map<String, Value>, keys: &[&str], default: bool) -> Result<bool, String> {
    let (key, v) = pick(p, keys);
    match v {
        None | Some(Value::Null) => Ok(default),
        Some(Value::Bool(b)) => Ok(*b),
        _ => Err(format!("{key} must be true or false")),
    }
}

fn get_origin(p: &Map<String, Value>) -> Result<[f64; 2], String> {
    // The desktop dialog sends position_x / position_y (it draws no list fields);
    // scripts may also use origin_x / origin_y or origin: [x, y].
    let num = |keys: &[&str]| -> Result<Option<f64>, String> {
        match pick(p, keys) {
            (_, None) => Ok(None),
            (k, Some(v)) => v.as_f64().filter(|n| n.is_finite()).map(Some).ok_or(format!("{k} must be a number (mm)")),
        }
    };
    let (ox, oy) = (num(&["position_x", "origin_x"])?, num(&["position_y", "origin_y"])?);
    if ox.is_some() || oy.is_some() {
        return Ok([ox.unwrap_or(0.0), oy.unwrap_or(0.0)]);
    }
    match p.get("origin") {
        None | Some(Value::Null) => Ok([0.0, 0.0]),
        Some(Value::Array(a)) if a.len() == 2 && a.iter().all(|v| v.as_f64().is_some()) => {
            Ok([a[0].as_f64().unwrap(), a[1].as_f64().unwrap()])
        }
        _ => Err("origin must be [x, y] in mm".into()),
    }
}

/// A length for a name: at most two decimals, no trailing zeros (89.5, 42, 39.1).
fn mm(v: f64) -> String {
    let s = format!("{:.2}", v);
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn get_name(p: &Map<String, Value>, default: &str) -> Result<String, String> {
    match pick(p, &["title", "name"]).1 {
        None | Some(Value::Null) => Ok(default.into()),
        Some(Value::String(s)) if !s.trim().is_empty() && s.len() <= 60 => Ok(s.trim().into()),
        Some(Value::String(s)) if s.trim().is_empty() => Ok(default.into()),
        _ => Err("the body name must be a short text".into()),
    }
}

const GRID_ERR: &str = "grid must be \"42 mm (full)\", \"21 mm (half)\" or \"Custom\"";

/// Cell size along X and Y. "42 mm (full)" / "21 mm (half)" / "Custom (sizes below)" as the
/// dialog offers them; scripts may say "full" / "half" / "custom". Custom takes custom_size_x
/// and custom_size_y (mm), which are ignored otherwise.
fn get_pitch(p: &Map<String, Value>) -> Result<[f64; 2], String> {
    let s = match p.get("grid") {
        None | Some(Value::Null) => return Ok([PITCH; 2]),
        Some(Value::String(s)) => s.to_ascii_lowercase(),
        _ => return Err(GRID_ERR.into()),
    };
    if s.contains("half") || s.starts_with("21") {
        return Ok([HALF_PITCH; 2]);
    }
    if s.contains("full") || s.starts_with("42") {
        return Ok([PITCH; 2]);
    }
    if !s.starts_with("custom") {
        return Err(GRID_ERR.into());
    }
    let size = |key: &str| match p.get(key) {
        None | Some(Value::Null) => Ok(PITCH),
        Some(v) => v.as_f64().filter(|n| (CUSTOM_MIN..=CUSTOM_MAX).contains(n))
            .ok_or(format!("{key} must be from {CUSTOM_MIN} to {CUSTOM_MAX} mm")),
    };
    Ok([size("custom_size_x")?, size("custom_size_y")?])
}

/// Most cells that fit in MAX_SIDE at this cell size.
fn max_cells(pitch: f64) -> u64 {
    ((MAX_SIDE / pitch + 1e-9).floor() as u64).clamp(1, MAX_CELLS)
}

/// " half", " 40x45 mm" or "" for the automatic body name.
fn grid_label(pitch: [f64; 2]) -> String {
    if pitch == [PITCH; 2] {
        String::new()
    } else if pitch == [HALF_PITCH; 2] {
        " half".into()
    } else {
        format!(" {}x{} mm", pitch[0], pitch[1])
    }
}

pub fn parse_bin(params: &Value) -> Result<BinOptions, String> {
    let empty = Map::new();
    let p = params.as_object().unwrap_or(&empty);
    let pitch = get_pitch(p)?;
    // smaller cells may be more of them for the same overall size
    let nx = get_u(p, "width", 1, 1, max_cells(pitch[0]))?;
    let ny = get_u(p, "depth", 1, 1, max_cells(pitch[1]))?;
    let units = get_u(p, "height", 3, 2, MAX_UNITS)?;
    let half = grid_label(pitch);
    let lip = get_b(p, &["stacking_lip"], true)?;
    let (solid, eco) = match p.get("inside") {
        None | Some(Value::Null) => (false, false),
        Some(Value::String(s)) if s.to_ascii_lowercase().starts_with("solid") => (true, false),
        Some(Value::String(s)) if s.to_ascii_lowercase().starts_with("hollow") => (false, false),
        Some(Value::String(s)) if s.to_ascii_lowercase().starts_with("eco") => (false, true),
        _ => return Err("inside must be \"Hollow\", \"Eco (thin walls, hollow feet)\" or \"Solid (for custom cutouts)\"".into()),
    };
    // 0: the standard wall
    let wall = get_mm(p, "inside_wall", 0.0, 0.0, MAX_WALL)?;
    if wall > 0.0 && wall < MIN_WALL {
        return Err(format!("Wall thickness must be from {MIN_WALL} to {MAX_WALL} mm, or 0 for the standard wall."));
    }
    // only a solid bin has a fill; 0 fills it to the top of the walls
    let mut fill = get_mm(p, "inside_fill_height", 0.0, 0.0, UNIT_H * MAX_UNITS as f64)?;
    if !solid {
        fill = 0.0;
    }
    // leave at least 1 mm of wall above the fill
    let max_fill = ((bin_inside_top(units, lip, wall) - 1.0) * 100.0).round() / 100.0;
    if fill > 0.0 && !(FLOOR_Z..=max_fill).contains(&fill) {
        return Err(format!("Fill height must be from {FLOOR_Z} to {max_fill} mm for a {units}-unit bin{}, or 0 to fill it to the top.",
            if lip { "" } else { " without stacking lip" }));
    }
    let mut label_text: Vec<String> = match p.get("inside_label_text") {
        None | Some(Value::Null) => vec![],
        Some(Value::String(s)) if s.trim().is_empty() => vec![],
        Some(Value::String(s)) => s.split(';').map(|t| t.trim().to_string()).collect(),
        _ => return Err("Label text must be text: one label per compartment, separated by ;".into()),
    };
    // a ; at the end adds no compartment
    while label_text.last().is_some_and(|t| t.is_empty()) {
        label_text.pop();
    }
    let auto_name = format!("Gridfinity{half} bin {nx}x{ny}x{units}");
    let (scoop, scoop_auto) = get_scoop(p)?;
    let (label, sticker, chosen) = get_label_tab(p)?;
    // text goes on a tab chosen for Printed text: one for a sticker, or none, has none
    if chosen && (!label || sticker) {
        label_text.clear();
    }
    let mut o = BinOptions {
        nx,
        ny,
        pitch,
        units,
        sized_units: units,
        magnets: get_b(p, &["holes_magnet", "magnets"], false)?,
        magnet: get_magnet(p)?,
        screws: get_b(p, &["holes_screw", "screws"], false)?,
        div_x: get_u(p, "dividers_x", 0, 0, MAX_DIVIDERS)?,
        div_y: get_u(p, "dividers_y", 0, 0, MAX_DIVIDERS)?,
        lip,
        solid,
        eco,
        fill,
        scoop,
        scoop_auto,
        label,
        sticker,
        label_w: get_mm(p, "inside_label_tab_width", geometry::LABEL_W, 5.0, geometry::LABEL_W_MAX)?,
        text_size: get_mm(p, "inside_label_text_size", 0.0, 0.0, 20.0)?,
        text_height: get_mm(p, "inside_label_text_height", 0.4, 0.1, 3.0)?,
        flush: match p.get("inside_label_text_style") {
            None | Some(Value::Null) => false,
            Some(Value::String(s)) if s.to_ascii_lowercase().starts_with("raised") => false,
            Some(Value::String(s)) if s.to_ascii_lowercase().starts_with("flush") => true,
            _ => return Err("Label text style must be \"Raised\" or \"Flush (inlaid)\"".into()),
        },
        font: match p.get("inside_label_text_font") {
            None | Some(Value::Null) => String::new(),
            Some(Value::String(s)) if s.trim().len() <= 200 => s.trim().to_string(),
            _ => return Err("Label text font must be the name of an installed font, or empty for the built-in one".into()),
        },
        label_text,
        filler: [
            get_mm(p, "filler_left", 0.0, 0.0, MAX_FILLER)?,
            get_mm(p, "filler_right", 0.0, 0.0, MAX_FILLER)?,
            get_mm(p, "filler_front", 0.0, 0.0, MAX_FILLER)?,
            get_mm(p, "filler_back", 0.0, 0.0, MAX_FILLER)?,
        ],
        wall,
        origin: get_origin(p)?,
        name: get_name(p, &auto_name)?,
    };
    // the automatic name says how big the bin really is
    if o.name == auto_name {
        let [w, d, h] = bin_size(&o);
        o.name = format!("{auto_name} ({} x {} x {} mm)", mm(w), mm(d), mm(h));
    }
    // a thick wall keeps some room inside, also in each compartment
    let inside = |cells: u32, pitch: f64, filler: f64, div: u32| {
        (pitch * cells as f64 + filler - 0.5 - 2.0 * bin_wall(eco, wall)) / (div + 1) as f64
    };
    let room = inside(nx, pitch[0], o.filler[0] + o.filler[1], o.div_x).min(inside(ny, pitch[1], o.filler[2] + o.filler[3], o.div_y));
    if wall > 0.0 && room < 5.0 {
        return Err(format!("A {wall} mm wall leaves too little room inside: use a thinner wall, a bigger bin, or fewer dividers."));
    }
    if eco && o.screws {
        return Err("An eco bin has hollow feet, with no room for screw holes: turn Screw holes off, or choose Inside \"Hollow\".".into());
    }
    if o.label_text.iter().any(|t| !t.is_empty()) {
        check_label_text(&mut o)?;
    }
    Ok(o)
}

/// Label text is slow in the desktop app: SolidCraft checks every letter as a body of its own,
/// about half a second each, and a compartment with a label tab costs about as much. A run may
/// take this many of the two together (30 seconds); sunk (flush) letters count twice.
const LABEL_BUDGET: usize = 50;

/// Why a label tab has no room for text: the tab is narrow, or the stacking lip covers its back.
/// Says how wide the tab must be, how tall the bin must be, or that turning the lip off is enough.
fn too_low(o: &BinOptions) -> String {
    let fits = |units: u32, lip: bool, label_w: f64| {
        let q = BinOptions { units, sized_units: units, lip, label_w, ..o.clone() };
        label_areas(&q).iter().zip(&o.label_text).all(|(a, t)| a.3 / text_depth(t) >= 2.0)
    };
    let wider = (o.label_w.floor() as u32 + 1..=geometry::LABEL_W_MAX as u32).find(|&w| fits(o.units, o.lip, w as f64));
    let taller = (o.units + 1..=o.units + 10).find(|&k| fits(k, o.lip, o.label_w));
    let lip_off = o.lip && fits(o.units, false, o.label_w);
    let mut fix = Vec::new();
    if let Some(w) = wider {
        fix.push(format!("make Label tab width {w} mm"));
    }
    if let Some(k) = taller {
        fix.push(format!("make the bin {k} units tall"));
    }
    if lip_off {
        fix.push("turn Stacking lip off".to_string());
    }
    format!("A bin {} units tall has too small a label tab for text{}: {}.", o.units,
        if o.lip { " under its stacking lip" } else { "" },
        if fix.is_empty() { "leave Label text empty".to_string() } else { fix.join(", or ") })
}

/// Label text puts a label tab on the bin, and every label must fit on its tab.
fn check_label_text(o: &mut BinOptions) -> Result<(), String> {
    if o.solid {
        return Err("Label text needs a hollow bin: solid bins have no label tabs.".into());
    }
    o.label = true;
    let areas = label_areas(o);
    if o.label_text.len() > areas.len() {
        let n = areas.len();
        return Err(format!("{} labels, but the bin has {n} compartment{}: give one label per compartment, separated by ;, \
            back row first, left to right (Dividers across X and Y make more compartments).",
            o.label_text.len(), if n == 1 { "" } else { "s" }));
    }
    let glyphs: usize = o.label_text.iter().map(|t| t.chars().filter(|c| !c.is_whitespace()).count()).sum();
    // sunk text is cut out of the bin as well: each letter costs about twice as much
    let per = if o.flush { 2 } else { 1 };
    let room = LABEL_BUDGET.saturating_sub(areas.len()) / per;
    if glyphs > room {
        return Err(format!("{glyphs} letters and digits of label text take longer than SolidCraft allows a plugin \
            (30 seconds): with {} compartments, use at most {room}. Shorter labels, or fewer compartments, will do.", areas.len()));
    }
    for (t, &(_, _, w, d)) in o.label_text.iter().zip(&areas) {
        if t.is_empty() {
            continue;
        }
        let h = label_size(o, t, w, d);
        if d / text_depth(t) < 2.0 {
            return Err(too_low(o));
        }
        if h < 2.0 {
            return Err(format!("\"{t}\" does not fit on its label tab ({w:.0} mm wide): make it shorter or the bin wider."));
        }
        if text_width(t, h) > w + 0.01 || h * text_depth(t) > d + 0.01 {
            return Err(format!("\"{t}\" at {h} mm does not fit on its label tab ({w:.0} x {d:.0} mm): make the text smaller, \
                or set Label text size to 0 to fit it automatically."));
        }
    }
    Ok(())
}

/// A baseplate's magnet and screw holes: both need a solid bottom.
fn get_plate_holes(p: &Map<String, Value>, open: bool) -> Result<(bool, bool), String> {
    let magnets = get_b(p, &["holes_magnet", "magnets"], false)?;
    let screws = get_b(p, &["holes_screw", "screws"], false)?;
    if open && magnets {
        return Err("magnet holes need a solid bottom: choose Bottom \"Solid\" or turn Magnet holes off".into());
    }
    if open && screws {
        return Err("screw holes need a solid bottom: choose Bottom \"Solid\" or turn Screw holes off".into());
    }
    Ok((magnets, screws))
}

/// The magnet holes' diameter and depth: 6.5 x 2.4 mm for 6 x 2 mm magnets unless set, for
/// magnets that come a little bigger or smaller. At most 8 mm across, so a bin's foot keeps a
/// wall beside the hole.
fn get_magnet(p: &Map<String, Value>) -> Result<[f64; 2], String> {
    Ok([get_mm(p, "magnet_diameter", geometry::MAGNET[0], 3.5, 8.0)?, get_mm(p, "magnet_depth", geometry::MAGNET[1], 1.0, 4.0)?])
}

/// The label tab: None, For sticker (no text; the back wall is made flush with the stacking lip)
/// or Printed text. true and false (as before 1.5) are a tab with or without text, and none.
/// Returns whether there is a tab, whether it is for a sticker, and whether one was chosen by name.
fn get_label_tab(p: &Map<String, Value>) -> Result<(bool, bool, bool), String> {
    match p.get("inside_label_tab") {
        None | Some(Value::Null) => Ok((false, false, false)),
        Some(Value::Bool(b)) => Ok((*b, false, false)),
        Some(Value::String(s)) => match s.to_ascii_lowercase() {
            s if s.starts_with("none") => Ok((false, false, true)),
            s if s.starts_with("for sticker") || s.starts_with("sticker") => Ok((true, true, true)),
            s if s.starts_with("printed") => Ok((true, false, true)),
            _ => Err(LABEL_TAB_ERR.into()),
        },
        _ => Err(LABEL_TAB_ERR.into()),
    }
}

const LABEL_TAB_ERR: &str = "Label tab must be \"None\", \"For sticker\" or \"Printed text\"";

/// The scoop: None, Auto, or Custom with `inside_scoop_radius`. A number (as before 1.5) is a
/// custom radius, 0 none. Returns the radius and whether it is Auto.
fn get_scoop(p: &Map<String, Value>) -> Result<(f64, bool), String> {
    let radius = || get_mm(p, "inside_scoop_radius", 10.0, 0.0, 40.0);
    match p.get("inside_scoop") {
        None | Some(Value::Null) => Ok((0.0, false)),
        Some(Value::Number(_)) => Ok((get_mm(p, "inside_scoop", 0.0, 0.0, 40.0)?, false)),
        Some(Value::String(s)) => match s.trim().to_ascii_lowercase().as_str() {
            "none" | "" => Ok((0.0, false)),
            "auto" => Ok((0.0, true)),
            "custom" => Ok((radius()?, false)),
            _ => Err("Scoop must be \"None\", \"Auto\" or \"Custom\"".into()),
        },
        _ => Err("Scoop must be \"None\", \"Auto\" or \"Custom\"".into()),
    }
}

pub fn parse_baseplate(params: &Value) -> Result<BaseplateOptions, String> {
    let empty = Map::new();
    let p = params.as_object().unwrap_or(&empty);
    let pitch = get_pitch(p)?;
    let nx = get_u(p, "width", 3, 1, max_cells(pitch[0]))?;
    let ny = get_u(p, "depth", 3, 1, max_cells(pitch[1]))?;
    let open = match p.get("bottom") {
        None | Some(Value::Null) => false,
        Some(Value::String(s)) if s.to_ascii_lowercase().starts_with("open") => true,
        Some(Value::String(s)) if s.to_ascii_lowercase().starts_with("solid") => false,
        _ => return Err("bottom must be \"Solid\" or \"Open (frame only)\"".into()),
    };
    let (magnets, screws) = get_plate_holes(p, open)?;
    let half = grid_label(pitch);
    let frame = if open { " frame" } else { "" };
    Ok(BaseplateOptions {
        nx,
        ny,
        pitch,
        open,
        magnets,
        magnet: get_magnet(p)?,
        screws,
        filler: [
            get_mm(p, "filler_left", 0.0, 0.0, MAX_FILLER)?,
            get_mm(p, "filler_right", 0.0, 0.0, MAX_FILLER)?,
            get_mm(p, "filler_front", 0.0, 0.0, MAX_FILLER)?,
            get_mm(p, "filler_back", 0.0, 0.0, MAX_FILLER)?,
        ],
        half_cells: get_leftover(p)?,
        origin: get_origin(p)?,
        name: get_name(p, &format!("Gridfinity{half} baseplate{frame} {nx}x{ny}"))?,
        leave_out: get_leave_out(p, nx, ny)?,
    })
}

/// `leave_out`: cells as "column-row", from 1 at the front left, separated by commas or spaces.
fn get_leave_out(p: &Map<String, Value>, nx: u32, ny: u32) -> Result<Vec<[u32; 2]>, String> {
    let text = match p.get("leave_out") {
        None | Some(Value::Null) => return Ok(Vec::new()),
        Some(Value::String(s)) => s.clone(),
        Some(_) => return Err("leave_out must be text such as \"1-1, 3-2\"".into()),
    };
    let mut cells: Vec<[u32; 2]> = Vec::new();
    for t in text.split(|c: char| c == ',' || c == ';' || c.is_whitespace()).filter(|t| !t.is_empty()) {
        let cell = t.split_once(['-', 'x', 'X', '/']).and_then(|(c, r)| Some([c.trim().parse::<u32>().ok()?, r.trim().parse::<u32>().ok()?]))
            .ok_or(format!("Leave out cells: \"{t}\" is not a cell; write column-row, such as 1-1 or 3-2"))?;
        if cell[0] < 1 || cell[0] > nx || cell[1] < 1 || cell[1] > ny {
            return Err(format!("Leave out cells: {t} is outside the plate, which is {nx} x {ny} cells"));
        }
        if !cells.contains(&cell) {
            cells.push(cell);
        }
    }
    if cells.len() as u64 == nx as u64 * ny as u64 {
        return Err("Leave out cells: that leaves no cells".into());
    }
    cells.sort();
    Ok(cells)
}

/// Most compartments a bin with scoops or label tabs can have in one run: each divider
/// crosses every row's scoop and tab, and 49 already take 25 s.
const MAX_SCOOP_COMPARTMENTS: u32 = 36;

/// Widest filler on one side of a baseplate (mm).
const MAX_FILLER: f64 = 200.0;
/// Bin walls: one line of a 0.4 mm nozzle, up to a thick wall.
const MIN_WALL: f64 = 0.4;
const MAX_WALL: f64 = 5.0;

/// Largest drawer side (mm).
const MAX_DRAWER: f64 = 1000.0;

/// SolidCraft gives a plugin run 30 s, and the time goes mostly on joining the cells, about
/// 0.15 s a cell, and more with magnet holes, on a 4-core desktop (10 x 10 cells: 16 s, 6 x 6
/// with magnet holes: 15 s, 8 x 8 with them: over 30 s). These keep a run within the limit.
const MAX_CELLS_RUN: u32 = 100;
const MAX_CELLS_RUN_MAGNETS: u32 = 42;
/// Screw holes take longer again (their countersinks most): with `solidcraft-cli plugin test`
/// on 0.46.2, 6 x 7 cells with magnet holes take 10.2 s, 5 x 5 with screw holes 10.2 s and 4 x 5
/// with both 9.6 s.
const MAX_CELLS_RUN_SCREWS: u32 = 25;
const MAX_CELLS_RUN_BOTH: u32 = 20;
/// Bins take longer a cell, in the desktop app 0.43 under xvfb: 8 x 8 cells 19 s, 9 x 9 26 s,
/// 10 x 10 over 30 s. Eco bins' hollow feet take about 0.8 s a cell to join (4 x 4: 13 s, 5 x 5:
/// 19 s, 6 x 6: 29 s).
const MAX_BIN_CELLS_RUN: u32 = 72;
const ECO_WEIGHT: f64 = 1.8;
/// Dividers count as this many cells each, and where they cross more (a 5 x 5 bin takes 7 s,
/// 11 s with 10 dividers one way, 29 s with 10 each way; a 2 x 2 one with 10 each way 22 s).
const DIVIDER_WEIGHT: f64 = 0.5;
const CROSSING_WEIGHT: f64 = 0.6;
/// A compartment with a scoop or label tab counts as this many cells (7 x 7 cells with 3 x 3
/// compartments: 15 s, 18 s with scoops and label tabs).
const SCOOP_WEIGHT: f64 = 1.0;
/// Half cells in the filler add to a run's time, in a drawer more (in the desktop app under
/// xvfb: a 4 x 4 plate with magnet holes and a ring of 20 half cells takes 14 s, 5 x 5 with 24
/// of them 17 s, 6 x 6 without magnet holes and 28 of them 19 s; drawers of 400 x 340 and
/// 440 x 360, 26 s and 29 s with 8 and 10, 2 to 3 s more than without): about this many fit in
/// a run with nothing else.
const MAX_HALF_CELLS_RUN: u32 = 70;
const MAX_HALF_CELLS_RUN_DRAWER: u32 = 50;

/// A drawer is a long job (SolidCraft 0.48+): it may run ten minutes instead of 30 s. It plans
/// for eight, so a slower computer still finishes: 16 times what fits in 30 s.
const JOB_RUNS: u32 = 16;

/// `holes`: magnet holes, screw holes. `job`: the run is a long job.
fn check_time(cells: u32, (half, max_half): (u32, u32), holes: (bool, bool), job: bool, what: &str) -> Result<(), String> {
    let (k, limit) = if job { (JOB_RUNS, "ten minutes") } else { (1, "30 seconds") };
    let (max, with) = match holes {
        (true, true) => (MAX_CELLS_RUN_BOTH, " with magnet and screw holes"),
        (true, false) => (MAX_CELLS_RUN_MAGNETS, " with magnet holes"),
        (false, true) => (MAX_CELLS_RUN_SCREWS, " with screw holes"),
        (false, false) => (MAX_CELLS_RUN, ""),
    };
    let (max, max_half) = (max * k, max_half * k);
    if half == 0 && cells > max {
        return Err(format!(
            "{cells} cells is too many for one run: SolidCraft gives a plugin {limit}, which is about {max} cells{with}. {what}"
        ));
    }
    if half > 0 && cells as f64 / max as f64 + half as f64 / max_half as f64 > 1.0 {
        return Err(format!(
            "{cells} cells{with} and {half} half cells is too much for one run: SolidCraft gives a plugin {limit}, and half \
             cells take longer than cells. Set Leftover space to Solid, or: {what}"
        ));
    }
    Ok(())
}

fn get_mm(p: &Map<String, Value>, key: &str, default: f64, min: f64, max: f64) -> Result<f64, String> {
    match p.get(key) {
        None | Some(Value::Null) => Ok(default),
        Some(v) => v.as_f64().filter(|n| (min..=max).contains(n)).ok_or(format!("{key} must be from {min} to {max} mm")),
    }
}

fn get_filler(p: &Map<String, Value>, key: &str, low: &str, high: &str) -> Result<Filler, String> {
    match p.get(key) {
        None | Some(Value::Null) => Ok(Filler::Even),
        Some(Value::String(s)) => {
            let s = s.to_ascii_lowercase();
            if s.starts_with("even") || s.starts_with("both") {
                Ok(Filler::Even)
            } else if s.starts_with(low) {
                Ok(Filler::Low)
            } else if s.starts_with(high) {
                Ok(Filler::High)
            } else {
                Err(format!("{key} must be \"Even (both sides)\", \"{low}\" or \"{high}\""))
            }
        }
        _ => Err(format!("{key} must be a choice from the list")),
    }
}

/// The space beside the cells: half cells where they fit (the default), or all solid.
fn get_leftover(p: &Map<String, Value>) -> Result<bool, String> {
    match p.get("leftover") {
        None | Some(Value::Null) => Ok(true),
        Some(Value::String(s)) if s.to_ascii_lowercase().starts_with("half") => Ok(true),
        Some(Value::String(s)) if s.to_ascii_lowercase().starts_with("solid") => Ok(false),
        _ => Err("leftover must be \"Half cells\" or \"Solid\"".into()),
    }
}

/// Room left around a plate made to fill a picked drawer floor, in all (mm): half on each side.
pub const FLOOR_CLEARANCE: f64 = 0.5;

/// The drawer floor the user picked (SolidCraft 0.44 and later), if any.
pub fn picked_floor(params: &Value) -> Option<&Value> {
    params.get("floor").and_then(|f| f.get(0))
}

/// The drawer parameters for a plate that fills a floor of this size, with its minimum corner
/// here: the size less the clearance, centred on the floor.
pub fn fill_floor(params: &Value, size: [f64; 2], corner: [f64; 2]) -> Value {
    let mut p = params.clone();
    let r = |v: f64| (v * 100.0).round() / 100.0;
    p["drawer_width"] = json!(r(size[0] - FLOOR_CLEARANCE));
    p["drawer_depth"] = json!(r(size[1] - FLOOR_CLEARANCE));
    p["position_x"] = json!(r(corner[0] + FLOOR_CLEARANCE / 2.0));
    p["position_y"] = json!(r(corner[1] + FLOOR_CLEARANCE / 2.0));
    p
}

/// How high a face must reach above the floor to count as a wall: higher than a baseplate, so a
/// plate already on the floor doesn't stop the next one.
pub const WALL_MIN_HEIGHT: f64 = 10.0;

/// An upright flat face square to x (axis 0) or y (axis 1): where it is along that axis, which way
/// it faces (+1 or -1), how far it runs along the other axis, and its lowest and highest z.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WallFace {
    pub axis: usize,
    pub facing: f64,
    pub at: f64,
    pub span: [f64; 2],
    pub z: [f64; 2],
}

/// "Up to next": the floor rectangle (minimum and maximum corner) cut back to the nearest wall
/// seen from its middle, in each of the four directions, whichever body the wall is in. A wall
/// is a face looking back towards the middle that stands on (or reaches below) the floor and rises
/// at least WALL_MIN_HEIGHT above it. With no wall that way, the floor's own edge stays.
pub fn up_to_walls(mut lo: [f64; 2], mut hi: [f64; 2], floor_z: f64, walls: &[WallFace]) -> ([f64; 2], [f64; 2]) {
    let mid = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0];
    for w in walls {
        let (a, o) = (w.axis, 1 - w.axis);
        if w.z[0] > floor_z + 1.0 || w.z[1] < floor_z + WALL_MIN_HEIGHT || w.span[0] > mid[o] || w.span[1] < mid[o] {
            continue;
        }
        if w.facing < 0.0 && w.at > mid[a] && w.at < hi[a] {
            hi[a] = w.at;
        } else if w.facing > 0.0 && w.at < mid[a] && w.at > lo[a] {
            lo[a] = w.at;
        }
    }
    (lo, hi)
}

pub fn parse_drawer(params: &Value) -> Result<DrawerOptions, String> {
    let empty = Map::new();
    let p = params.as_object().unwrap_or(&empty);
    let pitch = get_pitch(p)?;
    let drawer = [get_mm(p, "drawer_width", 300.0, 15.0, MAX_DRAWER)?, get_mm(p, "drawer_depth", 300.0, 15.0, MAX_DRAWER)?];
    let bed = [get_mm(p, "printer_x", 256.0, 50.0, MAX_DRAWER)?, get_mm(p, "printer_y", 256.0, 50.0, MAX_DRAWER)?];
    let open = match p.get("bottom") {
        None | Some(Value::Null) => false,
        Some(Value::String(s)) if s.to_ascii_lowercase().starts_with("open") => true,
        Some(Value::String(s)) if s.to_ascii_lowercase().starts_with("solid") => false,
        _ => return Err("bottom must be \"Solid\" or \"Open (frame only)\"".into()),
    };
    let (magnets, screws) = get_plate_holes(p, open)?;
    let piece = get_u(p, "piece", 0, 0, 999)?;
    let half = grid_label(pitch);
    Ok(DrawerOptions {
        drawer,
        bed,
        pitch,
        filler: [get_filler(p, "filler_x", "left", "right")?, get_filler(p, "filler_y", "front", "back")?],
        half_cells: get_leftover(p)?,
        open,
        magnets,
        magnet: get_magnet(p)?,
        screws,
        origin: get_origin(p)?,
        // in a picked drawer (or one made again from its outline) the pieces lie together, as
        // they will in the drawer
        gap: if picked_floor(params).is_some() || p.get("floor_outline").is_some_and(|v| v.is_array()) { 0.0 } else { LAYOUT_GAP },
        name: get_name(p, &format!("Gridfinity{half} drawer {}x{}", drawer[0], drawer[1]))?,
        piece,
        leave_out: get_leave_out(p, u32::MAX, u32::MAX)?,
        outline: p.get("floor_outline").and_then(|v| v.as_array()).into_iter().flatten().filter_map(|s| {
            let pt = |q: &Value| Some([q.get(0)?.as_f64()?, q.get(1)?.as_f64()?]);
            Some([pt(s.get(0)?)?, pt(s.get(1)?)?])
        }).collect(),
    })
}

/// Turns the plan's commands into the ones SolidCraft runs, using what earlier ones reported:
/// `@pattern` becomes `sketch.pattern.rectangular` over the curves drawn since the sketch began
/// (or the last pattern), `@created` in a body list becomes the bodies the last command made,
/// and body and sketch names become the ones SolidCraft actually gave (a second bin with the
/// same name gets another, and must not change the first).
#[derive(Default)]
pub struct Runner {
    drawn: Vec<Value>,
    created: Vec<Value>,
    /// The copies of the last pattern marked `"@aside": true`, as `@aside`: patterns that must
    /// not change what `@created` and `@last` are.
    aside: Vec<Value>,
    bodies: std::collections::HashMap<String, String>,
    sketches: std::collections::HashMap<String, Value>,
    /// Text parameter names the plan uses, and the names they get in the design.
    params: std::collections::HashMap<String, String>,
    /// Every sketch and feature the run made, in order, for the folder they go in.
    pub features: Vec<Value>,
}

/// The bin's height as a parameter, `Height_units` (the runner makes its name unique), that
/// every height in the bin follows. The bin is planned twice more, one and two units taller:
/// each number that grows by the same step both times is a straight line over the height and
/// becomes an expression over the parameter; numbers that don't change, or don't change
/// evenly (a label tab that stops growing at 12 mm), stay as they are.
fn parametric_height(o: &BinOptions, base: Vec<Value>) -> Vec<Value> {
    let taller = |k: u32| bin_commands(&BinOptions { units: o.units + k, ..o.clone() });
    let (one, two) = (Value::Array(taller(1)), Value::Array(taller(2)));
    let mut base = Value::Array(base);
    if !same_shape(&base, &one) || !same_shape(&base, &two) {
        return match base { Value::Array(v) => v, _ => unreachable!() };
    }
    let u = o.units as f64;
    follow(&mut base, &one, &two, &|v0, step| {
        let a = geometry::r4(v0 - step * u);
        let step = geometry::r4(step);
        let sign = if a < 0.0 { "-" } else { "+" };
        format!("{{Height_units}} * {step} mm {sign} {} mm", a.abs())
    });
    let mut out = vec![json!({"command": "parameters.add", "params": {"name": "Height_units", "expression": o.units.to_string(),
        "unit": "", "comment": format!("{}: height in 7 mm units", o.name)}})];
    if let Value::Array(v) = base {
        out.extend(v);
    }
    out
}

/// The same commands with the same keys and lists, differing at most in numbers.
fn same_shape(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(_), Value::Number(_)) => true,
        (Value::Array(x), Value::Array(y)) => x.len() == y.len() && x.iter().zip(y).all(|(a, b)| same_shape(a, b)),
        (Value::Object(x), Value::Object(y)) => x.len() == y.len() && x.iter().all(|(k, a)| y.get(k).is_some_and(|b| same_shape(a, b))),
        _ => a == b,
    }
}

/// Replace each number in `v` that steps evenly through `one` and `two` with `expr(value, step)`.
fn follow(v: &mut Value, one: &Value, two: &Value, expr: &dyn Fn(f64, f64) -> String) {
    match v {
        Value::Number(n) => {
            let (v0, v1, v2) = (n.as_f64().unwrap(), one.as_f64().unwrap(), two.as_f64().unwrap());
            if (v1 - v0).abs() > 1e-6 && ((v2 - v1) - (v1 - v0)).abs() < 1e-3 {
                *v = json!(expr(v0, v1 - v0));
            }
        }
        Value::Array(x) => {
            for (i, e) in x.iter_mut().enumerate() {
                follow(e, &one[i], &two[i], expr);
            }
        }
        Value::Object(x) => {
            for (k, e) in x.iter_mut() {
                follow(e, &one[k], &two[k], expr);
            }
        }
        _ => {}
    }
}

/// A name for a new body called `name` that no body in the design has, nor its parts (the
/// bodies named `<name> Label`, ...): `name` itself if it is free, else `name (2)`, `name (3)`, ...
pub fn free_name(name: &str, existing: &[String]) -> Option<String> {
    let taken = |n: &str| existing.iter().any(|e| e.eq_ignore_ascii_case(n)
        || e.to_lowercase().starts_with(&format!("{} ", n.to_lowercase())));
    if !taken(name) {
        return None;
    }
    (2..).map(|k| format!("{name} ({k})")).find(|n| !taken(n))
}

/// The body a drawer run that builds one piece makes (`<name> 2-1`), or None for any other run.
pub fn piece_body(entry: &str, params: &Value) -> Option<String> {
    let o = parse_drawer(params).ok().filter(|o| entry == "drawer" && o.piece > 0)?;
    let pl = drawer::plan(&o).ok()?;
    drawer::pieces(&o, &pl).get(o.piece as usize - 1).map(|p| p.name.clone())
}

/// The text parameters a plan adds.
pub fn planned_params(cmds: &[Value]) -> Vec<String> {
    cmds.iter().filter(|c| c["command"] == "parameters.add")
        .filter_map(|c| c["params"]["name"].as_str().map(String::from)).collect()
}

impl Runner {
    /// Give the plan's parameters names no parameter in the design has yet: Label_1, Label_2,
    /// ... for the first bin, Bin2_Label_1, ... for the next.
    pub fn name_params(&mut self, planned: &[String], existing: &[String]) {
        let taken = |n: &String| existing.iter().any(|e| e.eq_ignore_ascii_case(n));
        let prefix = (1..).map(|k: u32| if k == 1 { String::new() } else { format!("Bin{k}_") })
            .find(|pre| !planned.iter().any(|p| taken(&format!("{pre}{p}")))).unwrap();
        self.params = planned.iter().map(|p| (p.clone(), format!("{prefix}{p}"))).collect();
    }

    /// The plan's parameter names and the names they got in the design.
    pub fn param_names(&self) -> &std::collections::HashMap<String, String> {
        &self.params
    }

    fn fill_names(&self, v: &mut Value) {
        match v {
            Value::String(s) if s.contains('{') => {
                for (from, to) in &self.params {
                    *s = s.replace(&format!("{{{from}}}"), to);
                }
            }
            Value::Array(x) => x.iter_mut().for_each(|e| self.fill_names(e)),
            _ => {}
        }
    }

    pub fn prepare(&self, c: &Value) -> (String, Value) {
        let name = c["command"].as_str().unwrap_or_default();
        if name == "@pattern" {
            return ("sketch.pattern.rectangular".into(), pattern_args(&c["params"], &self.drawn));
        }
        let mut p = c["params"].clone();
        if let Some(o) = p.as_object_mut() {
            o.remove("@aside");
        }
        let body = |v: &str| self.bodies.get(v).cloned().unwrap_or_else(|| v.to_string());
        for key in ["targets", "bodies", "tools"] {
            if let Some(Value::Array(list)) = p.get(key) {
                let mut out = Vec::new();
                for v in list {
                    match v.as_str() {
                        Some("@created") => out.extend(self.created.iter().cloned()),
                        Some("@aside") => out.extend(self.aside.iter().cloned()),
                        Some("@last") => out.extend(self.created.last().cloned()),
                        Some(s) => out.push(json!(body(s))),
                        None => out.push(v.clone()),
                    }
                }
                p[key] = json!(out);
            }
        }
        for key in ["target", "body"] {
            match p.get(key).and_then(|v| v.as_str()) {
                Some("@last") => p[key] = self.created.last().cloned().unwrap_or(Value::Null),
                Some(s) => p[key] = json!(body(s)),
                None => {}
            }
        }
        // `{Height_units}` in an expression: the parameter's name in the design
        if !self.params.is_empty() {
            for (key, v) in p.as_object_mut().into_iter().flatten() {
                if !(name == "parameters.add" && (key == "expression" || key == "comment")) {
                    self.fill_names(v);
                }
            }
        }
        for key in ["name", "text_expression"] {
            if name == "parameters.add" || name == "sketch.text" {
                if let Some(v) = p.get(key).and_then(|v| v.as_str()).and_then(|v| self.params.get(v)) {
                    p[key] = json!(v);
                }
            }
        }
        if let Some(s) = p.get("sketch").and_then(|v| v.as_str()) {
            if let Some(id) = self.sketches.get(s) {
                p["sketch"] = id.clone();
            }
        }
        (name.to_string(), p)
    }

    pub fn record(&mut self, c: &Value, reply: &Value) {
        let name = c["command"].as_str().unwrap_or_default();
        let reply = reply.get("result").filter(|r| r.is_object()).unwrap_or(reply);
        let made = if name == "sketch.create" { reply.get("sketch") } else { reply.get("feature") };
        if let Some(id) = made.filter(|id| id.is_number()) {
            self.features.push(id.clone());
        }
        match name {
            "@pattern" => self.drawn.clear(),
            "sketch.create" => {
                self.drawn.clear();
                if let (Some(n), Some(id)) = (c["params"]["name"].as_str(), reply.get("sketch")) {
                    self.sketches.insert(n.to_string(), id.clone());
                }
            }
            "sketch.finish" => {}
            n if n.starts_with("sketch.") => {
                if let Some(Value::Array(ids)) = reply.pointer("/created/curves") {
                    self.drawn.extend(ids.iter().cloned());
                }
            }
            _ => {}
        }
        if let Some(Value::Array(made)) = reply.get("created_bodies") {
            // `@created` / `@last`: the copies the last body pattern made
            if name == "solid.pattern.rectangular" {
                if c["params"]["@aside"] == json!(true) {
                    self.aside = made.clone();
                } else {
                    self.created = made.clone();
                }
            }
            // the body this command asked to make, by the name SolidCraft gave it
            if let (Some(asked), Some(Value::String(got))) = (c["params"]["body_name"].as_str(), made.first()) {
                self.bodies.insert(asked.to_string(), got.clone());
            }
        }
    }
}

/// What the design keeps about a part the plugin made (SolidCraft 0.45 and later): the entry and
/// the settings it was made with, the timeline folder and features that make it, and the names
/// its parameters got in the design. `edit` reads it back.
pub fn stored_item(entry: &str, params: &Value, folder: Option<&Value>, features: &[Value],
                   names: &std::collections::HashMap<String, String>) -> Value {
    let mut p = params.clone();
    if let Some(o) = p.as_object_mut() {
        o.remove("floor");
    }
    json!({"entry": entry, "params": p, "folder": folder.cloned().unwrap_or(Value::Null),
           "features": features, "parameters": names})
}

/// The stored part a picked body belongs to: the body itself, or one of its parts (the bodies
/// named `<name> Label`, ...). The longest name wins, so `Bin (2)` is not taken for `Bin`.
pub fn find_item<'a>(items: &'a Value, picked: &str) -> Option<(&'a str, &'a Value)> {
    items.as_object()?.iter()
        .filter(|(k, _)| picked == k.as_str() || picked.starts_with(&format!("{k} ")))
        .max_by_key(|(k, _)| k.len())
        .map(|(k, v)| (k.as_str(), v))
}

/// The stored settings with what was changed in Parameters since: the height (`Height_units`)
/// and the label texts (`Label_1`, ...). `names` maps the plan's names to the design's,
/// `list` is `parameters.list`'s `parameters`.
pub fn with_current_values(params: &Value, names: &Value, list: &[Value]) -> Value {
    let mut p = params.clone();
    let current = |plan_name: &str| names.get(plan_name).and_then(|n| n.as_str())
        .and_then(|n| list.iter().find(|q| q["name"].as_str() == Some(n)));
    if let Some(v) = current("Height_units").and_then(|q| q["value"].as_f64()) {
        if v.fract().abs() < 1e-9 && v >= 1.0 {
            p["height"] = json!(v as u64);
        }
    }
    let mut texts: Vec<String> = match p.get("inside_label_text").and_then(|t| t.as_str()) {
        Some(s) if !s.trim().is_empty() => s.split(';').map(|t| t.trim().to_string()).collect(),
        _ => vec![],
    };
    let mut changed = false;
    for (plan_name, _) in names.as_object().into_iter().flatten() {
        let Some(k) = plan_name.strip_prefix("Label_").and_then(|k| k.parse::<usize>().ok()).filter(|&k| k > 0) else { continue };
        let Some(q) = current(plan_name) else { continue };
        let text = q["text"].as_str().or(q["expression"].as_str()).unwrap_or_default().trim().trim_matches('"').trim().to_string();
        if texts.len() < k {
            texts.resize(k, String::new());
        }
        if texts[k - 1] != text {
            texts[k - 1] = text;
            changed = true;
        }
    }
    if changed {
        p["inside_label_text"] = json!(texts.join("; "));
    }
    p
}

/// A bin's settings with the changes asked for in `edit`: each field left at Keep keeps the
/// bin's own value.
pub fn apply_edits(params: &Value, edits: &Value) -> Value {
    let mut p = params.clone();
    if !p.is_object() {
        p = json!({});
    }
    // the form calls width and depth cells_x and cells_y, so they sort together at the top
    for (key, keys) in [("width", ["cells_x", "width"]), ("depth", ["cells_y", "depth"]), ("height", ["height", "height"])] {
        if let Some(v) = keys.iter().find_map(|k| edits.get(*k).and_then(|v| v.as_u64()).filter(|&v| v > 0)) {
            p[key] = json!(v);
        }
    }
    for key in ["dividers_x", "dividers_y"] {
        if let Some(v) = edits.get(key).and_then(|v| v.as_i64()).filter(|&v| v >= 0) {
            p[key] = json!(v);
        }
    }
    // a radius on its own means a Custom scoop of that size
    if let Some(v) = edits.get("inside_scoop_radius").and_then(|v| v.as_f64()).filter(|&v| v > 0.0) {
        p["inside_scoop"] = json!("Custom");
        p["inside_scoop_radius"] = json!(v);
    }
    match edits.get("inside_scoop") {
        Some(Value::String(s)) if s != KEEP => {
            // Custom without a new radius keeps the bin's own one, if it had one
            if s == "Custom" && p.get("inside_scoop_radius").is_none() {
                if let Some(r) = p.get("inside_scoop").and_then(|v| v.as_f64()).filter(|&r| r > 0.0) {
                    p["inside_scoop_radius"] = json!(r);
                }
            }
            p["inside_scoop"] = json!(s);
        }
        // a number, as before 1.5: -1 keeps
        Some(Value::Number(n)) if n.as_f64().is_some_and(|v| v >= 0.0) => p["inside_scoop"] = json!(n),
        _ => {}
    }
    if let Some(v) = edits.get("inside_label_tab_width").and_then(|v| v.as_f64()).filter(|&v| v > 0.0) {
        p["inside_label_tab_width"] = json!(v);
    }
    for key in ["inside", "inside_label_text_style"] {
        if let Some(v) = edits.get(key).and_then(|v| v.as_str()).filter(|&v| v != KEEP) {
            p[key] = json!(v);
        }
    }
    for key in ["magnet_diameter", "magnet_depth"] {
        if let Some(v) = edits.get(key).and_then(|v| v.as_f64()).filter(|&v| v > 0.0) {
            p[key] = json!(v);
        }
    }
    if let Some(v) = edits.get("inside_label_text_size").and_then(|v| v.as_f64()).filter(|&v| v >= 0.0) {
        p["inside_label_text_size"] = json!(v);
    }
    if let Some(v) = edits.get("inside_label_text_height").and_then(|v| v.as_f64()).filter(|&v| v > 0.0) {
        p["inside_label_text_height"] = json!(v);
    }
    match edits.get("inside_label_text_font").and_then(|v| v.as_str()).map(str::trim) {
        Some("-") => p["inside_label_text_font"] = json!(""),
        Some(f) if !f.is_empty() => p["inside_label_text_font"] = json!(f),
        _ => {}
    }
    for key in ["holes_magnet", "holes_screw", "stacking_lip", "inside_label_tab"] {
        match edits.get(key).and_then(|v| v.as_str()) {
            Some("Yes") => p[key] = json!(true),
            Some("No") => p[key] = json!(false),
            Some(s) if key == "inside_label_tab" && s != KEEP => p[key] = json!(s),
            _ => {}
        }
    }
    match edits.get("inside_label_text").and_then(|v| v.as_str()).map(str::trim) {
        Some("-") => p["inside_label_text"] = json!(""),
        Some(t) if !t.is_empty() => {
            p["inside_label_text"] = json!(t);
            // new text on a bin whose tab is for a sticker, or has none: a tab for the text
            if p["inside_label_tab"].as_str().is_some_and(|s| !s.to_ascii_lowercase().starts_with("printed")) {
                p["inside_label_tab"] = json!("Printed text");
            }
        }
        _ => {}
    }
    p
}

/// The value an `edit` field has when it keeps what the bin has.
pub const KEEP: &str = "Keep";

/// `sketch.pattern.rectangular`'s arguments for an `@pattern` step over the curves `ids`.
pub fn pattern_args(p: &Value, ids: &[Value]) -> Value {
    let (n1, n2) = (p["count"].as_u64().unwrap_or(1), p["count2"].as_u64().unwrap_or(1));
    if n1 == 1 {
        // only along Y
        json!({"entities": ids, "count": n2, "spacing": p["spacing2"], "dir": [0, 1]})
    } else if n2 == 1 {
        json!({"entities": ids, "count": n1, "spacing": p["spacing"]})
    } else {
        json!({"entities": ids, "count": n1, "spacing": p["spacing"], "count2": n2, "spacing2": p["spacing2"]})
    }
}

/// The command list for an entry, or a message saying what is wrong with the input.
pub fn plan(entry: &str, params: &Value) -> Result<(String, Vec<Value>), String> {
    match entry {
        "bin" => {
            let o = parse_bin(params)?;
            let compartments = (o.div_x + 1) * (o.div_y + 1);
            if !o.solid && (o.has_scoop() || o.label) && compartments > MAX_SCOOP_COMPARTMENTS {
                return Err(format!(
                    "{compartments} compartments with a scoop or label tab take longer than SolidCraft allows a plugin \
                     (30 seconds): use at most {MAX_SCOOP_COMPARTMENTS}, or fewer dividers."
                ));
            }
            // holes and hollow feet make the feet slower to join: screw holes (which reach into
            // the block) most
            let weight = 1.0 + if o.magnets { 0.6 } else { 0.0 } + if o.screws { 2.0 } else { 0.0 }
                + if o.eco { ECO_WEIGHT } else { 0.0 };
            let cells = o.nx * o.ny;
            let (dx, dy) = (o.div_x as f64, o.div_y as f64);
            let scoops = !o.solid && (o.has_scoop() || o.label);
            let dividers = if o.solid { 0.0 } else { DIVIDER_WEIGHT * (dx + dy) + CROSSING_WEIGHT * dx * dy }
                + if scoops { SCOOP_WEIGHT * compartments as f64 } else { 0.0 };
            if cells as f64 * weight <= MAX_BIN_CELLS_RUN as f64 && cells as f64 * weight + dividers > MAX_BIN_CELLS_RUN as f64 {
                return Err(format!("{} x {} compartments{} take too long in a bin this size: SolidCraft gives a plugin 30 seconds. \
                    Use fewer dividers{}, or make a smaller bin.", o.div_x + 1, o.div_y + 1,
                    if scoops { " with a scoop or label tab" } else { "" }, if scoops { " (or no scoop or label tab)" } else { "" }));
            }
            if cells as f64 * weight > MAX_BIN_CELLS_RUN as f64 {
                let fit = (MAX_BIN_CELLS_RUN as f64 / weight).floor();
                let with = match (o.magnets, o.screws) {
                    (true, true) => " with magnet and screw holes",
                    (true, false) => " with magnet holes",
                    (false, true) => " with screw holes",
                    _ => "",
                };
                let what = if o.eco { "an eco bin" } else { "a bin" };
                return Err(format!("{cells} cells is too many for one run: SolidCraft gives a plugin 30 seconds, \
                    which is about {fit} cells for {what}{with}. Make a smaller bin."));
            }
            Ok((o.name.clone(), parametric_height(&o, bin_commands(&o))))
        }
        "baseplate" => {
            let o = parse_baseplate(params)?;
            let half = plate::half_cell_count(&geometry::baseplate_piece(&o));
            check_time(o.nx * o.ny, (half, MAX_HALF_CELLS_RUN), (o.magnets && !o.open, o.screws && !o.open), false,
                "Use Gridfinity drawer baseplate, which splits it into pieces and can build them one at a time.")?;
            Ok((o.name.clone(), baseplate_commands(&o)))
        }
        "drawer" => {
            let o = parse_drawer(params)?;
            let (pl, c) = drawer_commands(&o)?;
            let count = pl.axes[0].pieces.len() * pl.axes[1].pieces.len();
            let built = drawer::built_cells(&o, &pl);
            let half = drawer::built_half_cells(&o, &pl);
            check_time(built, (half, MAX_HALF_CELLS_RUN_DRAWER), (o.magnets && !o.open, o.screws && !o.open), true, &if o.piece == 0 {
                format!("This drawer is {count} pieces: set Piece to 1, 2, ... {count} and run once for each.")
            } else {
                "Use a smaller printer size, so the pieces get smaller.".to_string()
            })?;
            Ok((o.name.clone(), c))
        }
        other => Err(format!("unknown entry `{other}` (use `bin`, `baseplate`, `drawer` or `edit`)")),
    }
}

#[cfg(target_arch = "wasm32")]
mod guest {
    use super::*;
    use bindings::craft::plugin::commands;
    use bindings::craft::plugin::control;
    use bindings::craft::plugin::types::{FieldFailure, PluginError};
    use bindings::craft::plugin::types::{Datum, Value as UiValue};
    use bindings::craft::plugin::ui::{self, ButtonSpec, Column, Declarations, DockZone, Field, FieldKind, FormSpec, Layout, Node,
        PanelSpec, SelectionMode, SplitSpec, Tab, TableSpec, UiEvent, Widget};
    use bindings::exports::craft::plugin::entry::Guest;
    use bindings::exports::craft::plugin::ui_provider::Guest as UiGuest;

    struct Gridfinity;

    /// Host calls made in this run: SolidCraft allows 256, progress and cancel checks included.
    static CALLS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

    fn run_cmd(c: &str, a: &str) -> Result<String, PluginError> {
        CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        commands::run(c, &a.to_string())
    }

    fn calls_left() -> usize {
        256usize.saturating_sub(CALLS.load(std::sync::atomic::Ordering::Relaxed))
    }

    /// Runs a host command and returns its result.
    fn host(c: &str, a: Value) -> Result<Value, String> {
        let r: Value = serde_json::from_str(&run_cmd(c, &a.to_string()).map_err(|e| format!("{c}: {e:?}"))?).unwrap_or(Value::Null);
        Ok(r.get("result").cloned().unwrap_or(r))
    }

    /// The picked face's outline as an axis-aligned rectangle: its size, its minimum corner and
    /// its height, and the outline itself as segments seen from above (an arc as two). The face
    /// must be flat and face up. Its outline is the edges that name it.
    #[allow(clippy::type_complexity)]
    fn floor_rect(face: &Value) -> Result<([f64; 2], [f64; 2], f64, Vec<[[f64; 2]; 2]>), String> {
        let body = face["body"].as_str().ok_or("Drawer floor: pick a face")?;
        let index = face["index"].as_u64().ok_or("Drawer floor: pick a face")?;
        let faces = host("model.faces", json!({"body": body}))?;
        let f = faces["faces"].as_array().into_iter().flatten().find(|f| f["index"].as_u64() == Some(index))
            .ok_or("Drawer floor: that face is gone")?;
        if f["plane_normal"][2].as_f64().unwrap_or(0.0) < 0.999 {
            return Err("Drawer floor: pick a flat face that faces up".into());
        }
        let name = f["name"].as_str().unwrap_or_default().to_string();
        let edges = host("model.edges", json!({"body": body}))?;
        let (mut lo, mut hi) = ([f64::MAX; 2], [f64::MIN; 2]);
        let mut z = 0.0;
        let mut outline = Vec::new();
        for e in edges["edges"].as_array().into_iter().flatten() {
            if !e["name"].as_str().unwrap_or_default().split('|').any(|n| n == name) {
                continue;
            }
            let xy = |k: &str| [e[k][0].as_f64().unwrap_or(0.0), e[k][1].as_f64().unwrap_or(0.0)];
            let pts = if e["mid"].is_array() { drawer::edge_points(xy("start"), xy("mid"), xy("end")) } else { vec![xy("start"), xy("end")] };
            outline.extend(pts.windows(2).map(|w| [w[0], w[1]]));
            for k in ["start", "end", "mid"] {
                for i in 0..2 {
                    let v = e[k][i].as_f64().unwrap_or(0.0);
                    lo[i] = lo[i].min(v);
                    hi[i] = hi[i].max(v);
                }
                z = e[k][2].as_f64().unwrap_or(0.0);
            }
        }
        if lo[0] > hi[0] {
            return Err("Drawer floor: found no edges around that face".into());
        }
        // Walls may be bodies of their own standing on a bigger floor: stop at them.
        let (lo, hi) = up_to_walls(lo, hi, z, &wall_faces(lo, hi, z));
        Ok(([hi[0] - lo[0], hi[1] - lo[1]], lo, z, outline))
    }

    /// The upright faces, square to x or y, of every body over the floor that rises above it.
    /// Best effort: if the design can't be read this way, there are just no walls.
    fn wall_faces(lo: [f64; 2], hi: [f64; 2], z: f64) -> Vec<WallFace> {
        let Ok(m) = host("inspect.measure", json!({})) else { return Vec::new() };
        let over = |b: &&Value| {
            let (bmin, bmax) = (&b["bbox"]["min"], &b["bbox"]["max"]);
            let f = |v: &Value, i: usize| v[i].as_f64().unwrap_or(f64::NAN);
            f(bmin, 0) < hi[0] && f(bmax, 0) > lo[0] && f(bmin, 1) < hi[1] && f(bmax, 1) > lo[1]
                && f(bmin, 2) <= z + 1.0 && f(bmax, 2) >= z + WALL_MIN_HEIGHT
        };
        let mut out = Vec::new();
        for b in m["bodies"].as_array().into_iter().flatten().filter(over).take(40) {
            let Some(body) = b["name"].as_str() else { continue };
            let (Ok(faces), Ok(edges)) = (host("model.faces", json!({"body": body})), host("model.edges", json!({"body": body}))) else { continue };
            // upright faces by name: (axis, facing), and their extent from the edges around them
            let mut upright: Vec<(String, usize, f64, [f64; 3], [f64; 3])> = Vec::new();
            for f in faces["faces"].as_array().into_iter().flatten() {
                let n = |i: usize| f["plane_normal"][i].as_f64().unwrap_or(0.0);
                let axis = if n(0).abs() > 0.999 { 0 } else if n(1).abs() > 0.999 { 1 } else { continue };
                upright.push((f["name"].as_str().unwrap_or_default().to_string(), axis, n(axis).signum(), [f64::MAX; 3], [f64::MIN; 3]));
            }
            for e in edges["edges"].as_array().into_iter().flatten() {
                let names: Vec<&str> = e["name"].as_str().unwrap_or_default().split('|').collect();
                for u in upright.iter_mut().filter(|u| names.contains(&u.0.as_str())) {
                    for k in ["start", "end", "mid"] {
                        for i in 0..3 {
                            let v = e[k][i].as_f64().unwrap_or(f64::NAN);
                            u.3[i] = u.3[i].min(v);
                            u.4[i] = u.4[i].max(v);
                        }
                    }
                }
            }
            for (_, axis, facing, mn, mx) in upright {
                if mn[0] <= mx[0] {
                    let o = 1 - axis;
                    out.push(WallFace { axis, facing, at: mn[axis], span: [mn[o], mx[o]], z: [mn[2], mx[2]] });
                }
            }
        }
        out
    }

    fn invalid_in(field: &str) -> impl Fn(String) -> PluginError + '_ {
        move |reason| PluginError::InvalidInput(FieldFailure { field: field.into(), reason })
    }

    /// `edit`: make a stored bin again with some of its settings changed. The old bin's folder
    /// (its sketches and features) and parameters are deleted first, so the new one gets its name.
    fn edit(edits: &Value) -> Result<String, PluginError> {
        let invalid = invalid_in("edit");
        let picked = edits["bin"].get(0)
            .and_then(|b| b["name"].as_str().or(b["body"].as_str()).or(b.as_str()))
            .ok_or_else(|| invalid("Edit bin: pick the bin to change".into()))?.to_string();
        let stored = host("storage.get", json!({})).map_err(&invalid)?;
        let items = &stored["document"]["items"];
        let Some((name, item)) = find_item(items, &picked) else {
            return Err(invalid(format!("Edit bin: the design has no settings for \"{picked}\". Only bins made with \
                Gridfinity 1.4 or later in SolidCraft 0.45 or later can be edited; make this one again with Gridfinity bin.")));
        };
        if item["entry"].as_str() != Some("bin") {
            return Err(invalid(format!("Edit bin: \"{name}\" is a {}, not a bin.",
                item["entry"].as_str().unwrap_or("part").replace("drawer", "drawer baseplate"))));
        }
        let (name, item) = (name.to_string(), item.clone());
        let list = host("parameters.list", json!({"filter": "user"})).map_err(&invalid)?;
        let list: Vec<Value> = list["parameters"].as_array().cloned().unwrap_or_default();
        let params = apply_edits(&with_current_values(&item["params"], &item["parameters"], &list), edits);
        // check the new settings before anything is deleted
        plan("bin", &params).map_err(&invalid)?;
        rebuild(&name, &item, params)
    }

    /// Makes stored part `name` again with `params`: its folder (sketches and features) and
    /// parameters go first, so the new one gets its name.
    fn rebuild(name: &str, item: &Value, params: Value) -> Result<String, PluginError> {
        let entry = item["entry"].as_str().unwrap_or("bin");
        let invalid = invalid_in(entry);
        let deleted = match item["folder"].as_u64() {
            Some(folder) => host("timeline.delete", json!({"groups": [folder]})),
            None => host("timeline.delete", json!({"features": item["features"]})),
        }.map_err(|e| invalid(format!("could not remove the old {entry}: {e}")))?;
        for n in item["parameters"].as_object().into_iter().flatten().filter_map(|(_, n)| n.as_str()) {
            let _ = host("parameters.delete", json!({"name": n}));
        }
        let failing = deleted["failing"].as_array().map_or(0, |f| f.len());
        build(entry, params, Some(name), failing)
    }

    /// A drawing of the drawer's pieces from above, each with its number and size in the pocket
    /// in its middle, so the pieces can be laid in the drawer in order. The drawing is the last
    /// thing the run makes; without room for it in the run's host calls, there is none. The view
    /// is shaded: as lines, a drawer of 9 pieces is more than a sheet takes (100000 records).
    fn layout_sheet(o: &DrawerOptions) -> Result<(), String> {
        let pl = drawer::plan(o)?;
        let labels = drawer::labels(o, &pl);
        if calls_left() < 2 * labels.len() + 6 {
            return Err("this run has no host calls left for it".into());
        }
        let d = host("drawing.create", json!({"name": format!("{} layout", o.name), "template": "iso-a3-part",
            "first_sheet_paper": {"size": {"standard": "a3"}, "orientation": "landscape"}}))?;
        let made = (|| {
            let sheet = d["sheets"][0]["id"].clone();
            let v = host("drawing.view.base", json!({"sheet": sheet, "view": "top", "visual_style": "shaded"}))?;
            let view = v["view"].clone();
            let scale = v["scale"]["factor"].as_f64().ok_or("no scale")?;
            let e = host("drawing.view.inspect", json!({"view": view}))?["extents"].clone();
            let f = |k: &str, i: usize| e[k][i].as_f64().unwrap_or(0.0);
            let centre = [(f("min", 0) + f("max", 0)) / 2.0, (f("min", 1) + f("max", 1)) / 2.0];
            let pocket = o.pitch[0].min(o.pitch[1]) * scale;
            for l in &labels {
                let at = [(l.at[0] - centre[0]) * scale, (l.at[1] - centre[1]) * scale];
                let size = format!("{} x {}", mm(l.size[0]), mm(l.size[1]));
                let big = (pocket * 0.35).clamp(2.5, 10.0);
                let small = (pocket * 0.13).min(pocket * 0.65 / (0.6 * size.len() as f64)).clamp(1.8, 5.0);
                host("drawing.note.add", json!({"anchor": {"view": view}, "at": [geometry::r4(at[0]), geometry::r4(at[1] + pocket * 0.12)],
                    "content": l.number.to_string(), "cap_mm": geometry::r4(big), "align": "centre"}))?;
                host("drawing.note.add", json!({"anchor": {"view": view}, "at": [geometry::r4(at[0]), geometry::r4(at[1] - pocket * 0.25)],
                    "content": size, "cap_mm": geometry::r4(small), "align": "centre"}))?;
            }
            let low = v["boundary_mm"]["min"].clone();
            let high = v["boundary_mm"]["max"].clone();
            let x = (low[0].as_f64().unwrap_or(0.0) + high[0].as_f64().unwrap_or(0.0)) / 2.0;
            host("drawing.note.add", json!({"anchor": {"sheet": sheet}, "at": [geometry::r4(x), geometry::r4(low[1].as_f64().unwrap_or(20.0) - 8.0)],
                "content": format!("{}: {} pieces, numbered along X, then Y, from the front left. Sizes in mm, with the tabs.",
                    o.name, labels.len()), "cap_mm": 3.5, "align": "centre"}))?;
            Ok::<(), String>(())
        })();
        let _ = host("drawing.close", json!({}));
        made
    }

    /// Plan an entry and run it on the design; `replaces` names the stored part this one takes
    /// the place of, and `failing` the later features that failed when it was removed.
    fn build(entry: &str, params: Value, replaces: Option<&str>, failing: usize) -> Result<String, PluginError> {
        let invalid = invalid_in(entry);
        // A picked drawer floor gives the drawer's size and where the plate goes; the plate
        // is built at z 0 and lifted onto the floor at the end.
        let mut params = params;
        let mut lift = 0.0;
        if let Some(face) = picked_floor(&params).filter(|_| entry == "drawer").cloned() {
            let (size, corner, z, outline) = floor_rect(&face).map_err(&invalid)?;
            params = fill_floor(&params, size, corner);
            // a floor that isn't a rectangle: the cells off it are left out
            let o = parse_drawer(&params).map_err(&invalid)?;
            let mut cells = o.leave_out.clone();
            for c in drawer::cells_off_floor(&o, &outline, FLOOR_CLEARANCE / 2.0).map_err(&invalid)? {
                if !cells.contains(&c) {
                    cells.push(c);
                }
            }
            cells.sort();
            params["leave_out"] = json!(cells.iter().map(|[c, r]| format!("{c}-{r}")).collect::<Vec<_>>().join(", "));
            if !cells.is_empty() {
                // kept with the settings, so the plate can be made again the same without the pick
                params["floor_outline"] = json!(outline.iter().map(|[a, b]| json!([[geometry::r4(a[0]), geometry::r4(a[1])],
                    [geometry::r4(b[0]), geometry::r4(b[1])]])).collect::<Vec<_>>());
            }
            lift = z;
        }
        let (mut body, mut cmds) = plan(entry, &params).map_err(&invalid)?;
        // A body named like one already in the design would get an automatic name
        // ("Body2") that SolidCraft doesn't keep when it rebuilds, and the features made for
        // it would lose it: give it a free name of its own instead.
        let list: Value = serde_json::from_str(&run_cmd("model.body_features", &json!({}).to_string())?).unwrap_or(Value::Null);
        let list = list.get("result").unwrap_or(&list);
        let existing: Vec<String> = list["bodies"].as_array().into_iter().flatten()
            .filter_map(|b| b["body"].as_str().map(String::from)).collect();
        // A drawer built a piece at a time: the other pieces already there have its name, and
        // only the piece itself being there again means another drawer.
        let taken = match piece_body(entry, &params) {
            Some(piece) => existing.iter().any(|e| e.eq_ignore_ascii_case(&piece)),
            None => true,
        };
        if let Some(free) = free_name(&body, &existing).filter(|_| taken) {
            let mut p = params.clone();
            p["title"] = json!(free);
            (body, cmds) = plan(entry, &p).map_err(&invalid)?;
        }
        let total = cmds.len();
        let mut run = Runner::default();
        let planned = planned_params(&cmds);
        if !planned.is_empty() {
            let list: Value = serde_json::from_str(&run_cmd("parameters.list", &json!({"filter": "user"}).to_string())?).unwrap_or(Value::Null);
            let list = list.get("result").unwrap_or(&list);
            let existing: Vec<String> = list["parameters"].as_array().into_iter().flatten()
                .filter_map(|p| p["name"].as_str().map(String::from)).collect();
            run.name_params(&planned, &existing);
        }
        for (i, c) in cmds.iter().enumerate() {
            // progress and cancel checks are host calls too, and a run may make only 256
            if i % 16 == 0 {
                CALLS.fetch_add(2, std::sync::atomic::Ordering::Relaxed);
                if control::cancelled() {
                    return Err(PluginError::Cancelled);
                }
                control::progress(i as f32 / total as f32, &format!("step {} of {total}", i + 1));
            }
            let (name, args) = run.prepare(c);
            let reply = run_cmd(&name, &args.to_string())?;
            run.record(c, &serde_json::from_str(&reply).unwrap_or(Value::Null));
        }
        if lift.abs() > 1e-6 {
            let list = host("model.body_features", json!({})).map_err(&invalid)?;
            let made: Vec<&str> = list["bodies"].as_array().into_iter().flatten()
                .filter_map(|b| b["body"].as_str()).filter(|b| !existing.iter().any(|e| e == b)).collect();
            let reply = host("solid.move", json!({"bodies": made, "translate": [0.0, 0.0, lift]})).map_err(&invalid)?;
            if let Some(f) = reply.get("feature") {
                run.features.push(f.clone());
            }
        }
        // One folder in the timeline, named like the body, holds everything the run made.
        // The model is finished by now, so a folder that can't be made doesn't fail the run.
        let mut folder = None;
        if !run.features.is_empty() {
            folder = host("timeline.group", json!({"name": body, "features": run.features})).ok()
                .and_then(|r| r.get("group").cloned());
        }
        // The settings go in the design, so Edit bin can make it again with a change. Best
        // effort too: without them the part is still there, it just can't be edited.
        let mut items = json!({});
        if let Some(old) = replaces.filter(|&old| old != body) {
            items[old] = Value::Null;
        }
        items[body.as_str()] = stored_item(entry, &params, folder.as_ref(), &run.features, run.param_names());
        let _ = host("storage.patch", json!({"patch": {"items": items}}));
        let mut reply = json!({"body": body, "commands": total});
        if entry == "drawer" && params.get("layout_sheet").and_then(|v| v.as_bool()).unwrap_or(true) {
            if let Ok(o) = parse_drawer(&params).map(|o| DrawerOptions { name: body.clone(), ..o }) {
                // with every piece built: in this run, or this run builds the last one
                let count = drawer::plan(&o).map(|pl| pl.axes[0].pieces.len() * pl.axes[1].pieces.len()).unwrap_or(0);
                if o.piece == 0 || o.piece as usize == count {
                    if let Some(why) = layout_sheet(&DrawerOptions { piece: 0, ..o }).err() {
                        reply["warning"] = json!(format!("No layout sheet: {why}"));
                    }
                }
            }
        }
        if failing > 0 {
            reply["warning"] = json!(format!("{failing} later feature(s) used the old bin and now fail: see the timeline."));
        }
        Ok(reply.to_string())
    }

    impl Guest for Gridfinity {
        fn execute(entry: String, params: String) -> Result<String, PluginError> {
            let params: Value = serde_json::from_str(if params.trim().is_empty() { "{}" } else { &params })
                .map_err(|e| PluginError::InvalidInput(FieldFailure { field: "params".into(), reason: e.to_string() }))?;
            match entry.as_str() {
                "edit" => edit(&params),
                "panel" => {
                    let _ = ui::show_panel(PANEL, true);
                    panel_refresh().map(|n| json!({"parts": n}).to_string())
                }
                "panel-refresh" => panel_refresh().map(|n| json!({"parts": n}).to_string()),
                "panel-select" => panel_select(&params),
                e if e.starts_with("quick-") => quick(e),
                "layout-remove" | "layout-restore" => layout(&entry, &params),
                "settings-change" => settings_change(&params),
                _ => build(&entry, params, None, 0),
            }
        }
    }

    const PANEL: &str = "parts";

    fn text(path: String, v: &str) -> Datum {
        Datum { path, value: UiValue::Text(v.to_string()) }
    }

    fn stored_items() -> Result<Value, PluginError> {
        let stored = host("storage.get", json!({})).map_err(invalid_in("panel"))?;
        Ok(stored["document"]["items"].clone())
    }

    /// Fills the parts table from the settings stored in the design and empties the settings
    /// form; returns the row count.
    fn panel_refresh() -> Result<usize, PluginError> {
        let rows = panel::rows(&stored_items()?);
        ui::clear_data(PANEL)?;
        let mut data = Vec::new();
        for (i, r) in rows.iter().enumerate() {
            for (key, v) in [("id", &r.id), ("name", &r.name), ("kind", &r.kind), ("size", &r.size), ("height", &r.height), ("label", &r.label)] {
                data.push(text(format!("/parts/{i}/{key}"), v));
            }
        }
        for (key, _) in panel::DETAILS {
            data.push(text(format!("/details/{key}"), ""));
        }
        data.extend(settings_data(&stored_settings()?));
        ui::set_data(PANEL, &data)?;
        Ok(rows.len())
    }

    /// The parts table again, the rest of the panel as it is.
    fn panel_refresh_rows() -> Result<(), PluginError> {
        let rows = panel::rows(&stored_items()?);
        let mut data = Vec::new();
        for (i, r) in rows.iter().enumerate() {
            for (key, v) in [("id", &r.id), ("name", &r.name), ("kind", &r.kind), ("size", &r.size), ("height", &r.height), ("label", &r.label)] {
                data.push(text(format!("/parts/{i}/{key}"), v));
            }
        }
        ui::set_data(PANEL, &data)
    }

    /// A row clicked: its settings in the form below the table.
    fn panel_select(event: &Value) -> Result<String, PluginError> {
        let Some(id) = event["row"].as_str() else { return Ok("{}".into()) };
        let items = stored_items()?;
        let Some(row) = panel::rows(&items).into_iter().find(|r| r.id == id) else { return Ok("{}".into()) };
        let data: Vec<Datum> = panel::details(&row, &items[id]).into_iter()
            .map(|(key, v)| text(format!("/details/{key}"), &v)).collect();
        ui::set_data(PANEL, &data)?;
        Ok(json!({"selected": id}).to_string())
    }

    /// Layout: a pick in the model, from the Remove a cell or Restore all cells field.
    /// What it did, or why not, shows in the Result field: an error would undo that too.
    fn layout(entry: &str, event: &Value) -> Result<String, PluginError> {
        let said = match layout_change(entry, event) {
            Ok(s) | Err(s) => s,
        };
        ui::set_data(PANEL, &[text("/layout/status".into(), &said), text("/layout/remove".into(), ""), text("/layout/restore".into(), "")])?;
        let _ = panel_refresh_rows();
        Ok(json!({"result": said}).to_string())
    }

    fn layout_change(entry: &str, event: &Value) -> Result<String, String> {
        let pick: Value = event["value"]["geometry-ref"].as_str().and_then(|s| serde_json::from_str(s).ok())
            .ok_or("Nothing picked: click the field, then a pocket of a base plate")?;
        let body = pick["body"].as_str().or(pick["name"].as_str()).ok_or("Pick a face of a base plate")?;
        let items = stored_items().map_err(|e| format!("{e:?}"))?;
        let (name, item) = find_item(&items, body).ok_or(format!("{body} isn't a part Gridfinity made (1.4 or later)"))?;
        if item["entry"] != "baseplate" {
            return Err(format!("{} is a {}: only a base plate can leave cells out", panel::short_name(name),
                item["entry"].as_str().unwrap_or("part").replace("drawer", "drawer baseplate")));
        }
        let (name, item) = (name.to_string(), item.clone());
        let mut params = item["params"].clone();
        let o = parse_baseplate(&params)?;
        if entry == "layout-restore" {
            if o.leave_out.is_empty() {
                return Ok(format!("{} has all its cells", panel::short_name(&name)));
            }
            params["leave_out"] = json!("");
            rebuild(&name, &item, params).map_err(|e| format!("{e:?}"))?;
            return Ok(format!("{}: all cells back", panel::short_name(&name)));
        }
        // where the pick is: the point clicked, else the face's middle
        let at = match pick["point"].as_array() {
            Some(p) => [p[0].as_f64().unwrap_or(f64::NAN), p[1].as_f64().unwrap_or(f64::NAN)],
            None => {
                let faces = host("model.faces", json!({"body": body}))?;
                let face = faces["faces"].as_array().into_iter().flatten().find(|f| f["index"] == pick["index"])
                    .ok_or("that face is gone; pick again")?;
                [face["centroid"][0].as_f64().unwrap_or(f64::NAN), face["centroid"][1].as_f64().unwrap_or(f64::NAN)]
            }
        };
        let corner = geometry::baseplate_piece(&o).cells.corner;
        let cell = [((at[0] - corner[0]) / o.pitch[0]).floor() + 1.0, ((at[1] - corner[1]) / o.pitch[1]).floor() + 1.0];
        if !(cell[0] >= 1.0 && cell[0] <= o.nx as f64 && cell[1] >= 1.0 && cell[1] <= o.ny as f64) {
            return Err("That is the plate's filler, not a cell: pick a pocket".into());
        }
        let cell = [cell[0] as u32, cell[1] as u32];
        let mut cells = o.leave_out.clone();
        cells.push(cell);
        if cells.len() as u32 >= o.nx * o.ny {
            return Err("That is the plate's last cell".into());
        }
        cells.sort();
        params["leave_out"] = json!(cells.iter().map(|[c, r]| format!("{c}-{r}")).collect::<Vec<_>>().join(", "));
        rebuild(&name, &item, params).map_err(|e| format!("{e:?}"))?;
        Ok(format!("{}: cell {}-{} left out", panel::short_name(&name), cell[0], cell[1]))
    }

    fn stored_settings() -> Result<Value, PluginError> {
        let stored = host("storage.get", json!({})).map_err(invalid_in("panel"))?;
        Ok(panel::settings(&stored["document"]["settings"]))
    }

    fn settings_data(s: &Value) -> Vec<Datum> {
        panel::SETTINGS.iter().map(|(key, _, kind, _, _)| Datum {
            path: format!("/settings/{key}"),
            value: if *kind == "toggle" { UiValue::Flag(s[key].as_bool().unwrap_or(false)) } else { UiValue::Text(st(&s[key])) },
        }).collect()
    }

    /// A setting changed in the Settings tab: kept in the design.
    fn settings_change(event: &Value) -> Result<String, PluginError> {
        let invalid = invalid_in("settings");
        let key = event["path"].as_str().and_then(|p| p.strip_prefix("/settings/")).unwrap_or_default();
        let (_, _, kind, choices, _) = panel::SETTINGS.iter().find(|s| s.0 == key)
            .ok_or_else(|| invalid(format!("no setting {key}")))?;
        let v = &event["value"];
        let value = match *kind {
            "toggle" => v["flag"].as_bool().map(Value::Bool),
            _ => v["text"].as_str().filter(|t| choices.contains(t)).map(|t| json!(t)),
        }.ok_or_else(|| invalid(format!("{key}: {v}")))?;
        host("storage.patch", json!({"patch": {"settings": {key: value.clone()}}})).map_err(&invalid)?;
        ui::set_data(PANEL, &settings_data(&stored_settings()?))?;
        Ok(json!({key: value}).to_string())
    }

    /// A Quick button: its part, beside everything in the design (to the right, 10 mm apart).
    fn quick(entry: &str) -> Result<String, PluginError> {
        let invalid = invalid_in("quick");
        let (_, _, what, preset) = panel::presets().into_iter().find(|p| p.0 == entry)
            .ok_or_else(|| invalid(format!("unknown entry `{entry}`")))?;
        let mut params = panel::with_settings(what, &preset, &stored_settings()?);
        let m = host("inspect.measure", json!({})).map_err(&invalid)?;
        let right = m["bodies"].as_array().into_iter().flatten()
            .filter_map(|b| b["bbox"]["max"][0].as_f64()).fold(f64::NEG_INFINITY, f64::max);
        if right.is_finite() {
            params["position_x"] = json!(((right + 10.0) * 10.0).round() / 10.0);
        }
        let reply = build(what, params, None, 0)?;
        let _ = panel_refresh();
        Ok(reply)
    }

    fn st(v: &Value) -> String {
        v.as_str().unwrap_or_default().to_string()
    }

    fn opt(v: &Value) -> Option<String> {
        v.as_str().map(String::from)
    }

    /// One layout node of `panel::declarations()` as the WIT record.
    fn widget(w: &Value) -> Widget {
        let ids = |v: &Value| v.as_array().into_iter().flatten().filter_map(|i| i.as_u64()).map(|i| i as u32).collect::<Vec<_>>();
        let button = |b: &Value| ButtonSpec { label: st(&b["label"]), icon: opt(&b["icon"]), entry: st(&b["entry"]) };
        let (kind, v) = w.as_object().and_then(|o| o.iter().next()).expect("one widget a node");
        match kind.as_str() {
            "group" => Widget::Group(ids(v)),
            "tabs" => Widget::Tabs(v.as_array().into_iter().flatten()
                .map(|t| Tab { label: st(&t["label"]), child: t["child"].as_u64().unwrap_or(0) as u32 }).collect()),
            "split" => Widget::Split(SplitSpec { vertical: v["vertical"].as_bool().unwrap_or(false),
                first: v["first"].as_u64().unwrap_or(0) as u32, second: v["second"].as_u64().unwrap_or(0) as u32 }),
            "toolbar" => Widget::Toolbar(v.as_array().into_iter().flatten().map(button).collect()),
            "button" => Widget::Button(button(v)),
            "text" => Widget::Text(st(v)),
            "table" => Widget::Table(TableSpec {
                bind: st(&v["bind"]),
                row_id: st(&v["row_id"]),
                columns: v["columns"].as_array().into_iter().flatten()
                    .map(|c| Column { key: st(&c["key"]), label: st(&c["label"]), unit: opt(&c["unit"]) }).collect(),
                select: match v["select"].as_str() { Some("multi") => SelectionMode::Multi, Some("none") => SelectionMode::None, _ => SelectionMode::Single },
                on_select: opt(&v["on_select"]),
                on_edit: opt(&v["on_edit"]),
            }),
            "form" => Widget::Form(FormSpec {
                bind: st(&v["bind"]),
                fields: v["fields"].as_array().into_iter().flatten().map(|f| Field {
                    key: st(&f["key"]),
                    label: st(&f["label"]),
                    kind: match f["kind"].as_str() { Some("number") => FieldKind::Number, Some("toggle") => FieldKind::Toggle,
                                                     Some("choice") => FieldKind::Choice, Some("selection") => FieldKind::Selection,
                                                     _ => FieldKind::Text },
                    choices: f["choices"].as_array().into_iter().flatten().map(st).collect(),
                    on_change: opt(&f["on_change"]),
                }).collect(),
            }),
            other => Widget::EmptyState(format!("unknown widget {other}")),
        }
    }

    impl UiGuest for Gridfinity {
        fn describe() -> Declarations {
            let d = panel::declarations();
            Declarations {
                ui_version: d["ui_version"].as_u64().unwrap_or(1) as u32,
                panels: d["panels"].as_array().into_iter().flatten().map(|p| PanelSpec {
                    id: st(&p["id"]),
                    title: st(&p["title"]),
                    zones: p["zones"].as_array().into_iter().flatten().map(|z| match z.as_str() {
                        Some("left") => DockZone::Left, Some("bottom") => DockZone::Bottom, _ => DockZone::Right }).collect(),
                    layout: Layout {
                        root: p["layout"]["root"].as_u64().unwrap_or(0) as u32,
                        nodes: p["layout"]["nodes"].as_array().into_iter().flatten().map(|n| Node { widget: widget(&n["widget"]) }).collect(),
                    },
                }).collect(),
                workspaces: vec![],
            }
        }

        fn on_event(_event: UiEvent) -> Result<(), PluginError> {
            // the entry the event names does the work
            Ok(())
        }

        fn on_enter(_workspace: String) -> Result<(), PluginError> {
            Ok(())
        }

        fn on_exit(_workspace: String) -> Result<(), PluginError> {
            Ok(())
        }
    }

    bindings::export!(Gridfinity with_types_in bindings);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_picked_floor_is_filled_less_the_clearance_and_the_pieces_lie_together() {
        let face = json!({"type": "face", "body": "Drawer", "index": 10, "point": [0.0, 0.0, 4.0]});
        let p = fill_floor(&json!({"floor": [face]}), [314.0, 234.0], [13.0, 23.0]);
        assert_eq!((p["drawer_width"].as_f64(), p["drawer_depth"].as_f64()), (Some(313.5), Some(233.5)));
        assert_eq!((p["position_x"].as_f64(), p["position_y"].as_f64()), (Some(13.25), Some(23.25)));
        assert_eq!(parse_drawer(&p).unwrap().gap, 0.0);
        assert_eq!(parse_drawer(&json!({"floor": []})).unwrap().gap, LAYOUT_GAP);
        assert_eq!(parse_drawer(&json!({})).unwrap().gap, LAYOUT_GAP);
    }

    #[test]
    fn a_floor_under_separate_walls_stops_at_them() {
        // floor 330 x 250 at z 4; walls 5 mm (front, back) and 6 mm (left, right), 70 high
        let wall = |axis, facing, at, span: [f64; 2]| WallFace { axis, facing, at, span, z: [4.0, 74.0] };
        let walls = [
            wall(1, 1.0, 5.0, [0.0, 330.0]),      // front wall's inside, looking +y
            wall(1, -1.0, 245.0, [0.0, 330.0]),   // back wall's inside
            wall(0, 1.0, 6.0, [5.0, 245.0]),      // left
            wall(0, -1.0, 324.0, [5.0, 245.0]),   // right
            wall(0, -1.0, 0.0, [5.0, 245.0]),     // left wall's outside: behind the middle, ignored
            WallFace { axis: 0, facing: -1.0, at: 200.0, span: [0.0, 250.0], z: [4.0, 9.0] }, // a low plate: not a wall
            WallFace { axis: 0, facing: -1.0, at: 250.0, span: [0.0, 50.0], z: [4.0, 74.0] }, // off the middle line
        ];
        let (lo, hi) = up_to_walls([0.0, 0.0], [330.0, 250.0], 4.0, &walls);
        assert_eq!((lo, hi), ([6.0, 5.0], [324.0, 245.0]));
        // no walls: the floor's own edges
        assert_eq!(up_to_walls([1.0, 2.0], [3.0, 4.0], 0.0, &[]), ([1.0, 2.0], [3.0, 4.0]));
    }

    #[test]
    fn the_runner_collects_what_goes_in_the_folder() {
        let mut run = Runner::default();
        run.record(&json!({"command": "sketch.create", "params": {"name": "S"}}), &json!({"sketch": 1}));
        run.record(&json!({"command": "sketch.line", "params": {}}), &json!({"created": {"curves": ["l1"]}}));
        run.record(&json!({"command": "solid.extrude", "params": {}}), &json!({"result": {"feature": 2}}));
        run.record(&json!({"command": "parameters.add", "params": {}}), &json!({"name": "Height_units"}));
        assert_eq!(run.features, vec![json!(1), json!(2)]);
    }

    #[test]
    fn defaults_and_limits() {
        let o = parse_bin(&json!({})).unwrap();
        assert_eq!((o.nx, o.ny, o.units, o.lip), (1, 1, 3, true));
        assert!(parse_bin(&json!({"width": 0})).is_err());
        assert!(parse_bin(&json!({"height": 21})).is_err());
        assert!(parse_bin(&json!({"magnets": "yes"})).is_err());
        assert!(plan("lid", &json!({})).is_err());
        assert_eq!(parse_bin(&json!({"origin_x": 50.0, "origin_y": -10})).unwrap().origin, [50.0, -10.0]);
        assert_eq!(parse_bin(&json!({"origin": [1, 2]})).unwrap().origin, [1.0, 2.0]);
        assert!(parse_bin(&json!({"origin_x": "a"})).is_err());
        // the desktop dialog's names
        let o = parse_bin(&json!({"holes_magnet": true, "holes_screw": true, "position_x": 5, "title": "Tray"})).unwrap();
        assert!(o.magnets && o.screws);
        assert_eq!((o.origin, o.name.as_str()), ([5.0, 0.0], "Tray"));
        assert!(parse_baseplate(&json!({"holes_magnet": true})).unwrap().magnets);
        let h = parse_bin(&json!({"grid": "21 mm (half)", "width": 24})).unwrap();
        assert_eq!((h.pitch, h.nx, h.name.as_str()), ([HALF_PITCH; 2], 24, "Gridfinity half bin 24x1x3 (503.5 x 20.5 x 25.1 mm)"));
        // the automatic name has the real size: less the gap, with filler, with or without the lip
        assert_eq!(parse_bin(&json!({"width": 3, "height": 5, "grid": "Custom", "custom_size_x": 30, "custom_size_y": 30})).unwrap().name,
            "Gridfinity 30x30 mm bin 3x1x5 (89.5 x 29.5 x 39.1 mm)");
        assert_eq!(parse_bin(&json!({"filler_left": 10.25, "stacking_lip": false})).unwrap().name, "Gridfinity bin 1x1x3 (51.75 x 41.5 x 21 mm)");
        assert_eq!(parse_bin(&json!({"grid": "42 mm (full)"})).unwrap().pitch, [PITCH; 2]);
        assert!(parse_bin(&json!({"grid": "huge"})).is_err());
        assert!(parse_bin(&json!({"width": 13})).is_err());
        assert!(parse_bin(&json!({"inside": "Solid (for custom cutouts)"})).unwrap().solid);
        assert!(parse_bin(&json!({"inside": "Eco (thin walls, hollow feet)"})).unwrap().eco);
        assert!(parse_bin(&json!({"inside": "Eco", "holes_screw": true})).unwrap_err().contains("Screw holes off"));
        assert!(!parse_bin(&json!({"inside": "Hollow"})).unwrap().solid);
        assert!(parse_bin(&json!({"inside": "liquid"})).is_err());
        let b = parse_baseplate(&json!({"grid": "21 mm (half)", "bottom": "Open (frame only)", "width": 4})).unwrap();
        assert_eq!((b.pitch, b.open, b.name.as_str()), ([HALF_PITCH; 2], true, "Gridfinity half baseplate frame 4x3"));
        assert!(parse_baseplate(&json!({"bottom": "Open (frame only)", "holes_magnet": true})).is_err());
        assert!(parse_baseplate(&json!({"bottom": "Open (frame only)", "holes_screw": true})).unwrap_err().contains("Screw holes off"));
        assert!(parse_drawer(&json!({"bottom": "Open (frame only)", "holes_screw": true})).is_err());
        let b = parse_baseplate(&json!({"holes_screw": true})).unwrap();
        assert!(b.screws && !b.magnets);
        assert!(parse_baseplate(&json!({"screws": true, "magnets": true})).unwrap().screws);
        assert!(parse_drawer(&json!({"holes_screw": true})).unwrap().screws);
        let (name, cmds) = plan("baseplate", &json!({"width": 2, "depth": 2, "magnets": true})).unwrap();
        assert_eq!(name, "Gridfinity baseplate 2x2");
        assert!(!cmds.is_empty());
        // custom cell sizes
        let c = parse_bin(&json!({"grid": "Custom (sizes below)", "custom_size_x": 40, "custom_size_y": 45.5, "width": 2})).unwrap();
        assert_eq!((c.pitch, c.name.as_str()), ([40.0, 45.5], "Gridfinity 40x45.5 mm bin 2x1x3 (79.5 x 45 x 25.1 mm)"));
        assert_eq!(parse_bin(&json!({"grid": "custom"})).unwrap().pitch, [PITCH; 2]);
        // the custom sizes only count when Custom is chosen
        assert_eq!(parse_bin(&json!({"grid": "42 mm (full)", "custom_size_x": 30})).unwrap().pitch, [PITCH; 2]);
        assert!(parse_bin(&json!({"grid": "custom", "custom_size_x": 14})).is_err());
        assert!(parse_bin(&json!({"grid": "custom", "custom_size_y": 101})).is_err());
        // 504 mm at most: 20 cells of 25 mm, 5 of 100 mm
        assert!(parse_bin(&json!({"grid": "custom", "custom_size_x": 25, "width": 20})).is_ok());
        assert!(parse_bin(&json!({"grid": "custom", "custom_size_x": 25, "width": 21})).is_err());
        assert!(parse_baseplate(&json!({"grid": "custom", "custom_size_y": 100, "depth": 6})).is_err());
        let b = parse_baseplate(&json!({"grid": "custom", "custom_size_x": 50, "custom_size_y": 30})).unwrap();
        assert_eq!(b.name, "Gridfinity 50x30 mm baseplate 3x3");
    }

    #[test]
    fn runs_that_would_take_too_long_say_what_to_do() {
        assert!(plan("baseplate", &json!({"width": 10, "depth": 10})).is_ok());
        let e = plan("baseplate", &json!({"width": 12, "depth": 12})).unwrap_err();
        assert!(e.contains("144 cells") && e.contains("drawer baseplate"));
        assert!(plan("baseplate", &json!({"width": 7, "depth": 7, "holes_magnet": true})).is_err());
        assert!(plan("baseplate", &json!({"width": 5, "depth": 5, "holes_screw": true})).is_ok());
        assert!(plan("baseplate", &json!({"width": 6, "depth": 5, "holes_screw": true})).unwrap_err().contains("25 cells with screw holes"));
        assert!(plan("baseplate", &json!({"width": 5, "depth": 4, "holes_magnet": true, "holes_screw": true})).is_ok());
        assert!(plan("baseplate", &json!({"width": 7, "depth": 3, "holes_magnet": true, "holes_screw": true}))
            .unwrap_err().contains("20 cells with magnet and screw holes"));
        assert!(plan("bin", &json!({"width": 7, "depth": 6, "holes_magnet": true})).is_ok());
        assert!(plan("bin", &json!({"width": 7, "depth": 6, "holes_magnet": true, "holes_screw": true})).unwrap_err().contains("20 cells"));
        assert!(plan("bin", &json!({"width": 9, "depth": 8})).is_ok());
        assert!(plan("bin", &json!({"width": 9, "depth": 9})).unwrap_err().contains("about 72 cells for a bin."));
        // eco bins' hollow feet take longer
        assert!(plan("bin", &json!({"width": 5, "depth": 5, "inside": "Eco"})).is_ok());
        assert!(plan("bin", &json!({"width": 6, "depth": 5, "inside": "Eco"})).unwrap_err().contains("about 25 cells for an eco bin."));
        assert!(plan("bin", &json!({"width": 5, "depth": 4, "inside": "Eco", "holes_magnet": true})).is_ok());
        // dividers that cross take time too
        assert!(plan("bin", &json!({"width": 5, "depth": 5, "dividers_x": 10})).is_ok());
        assert!(plan("bin", &json!({"width": 2, "depth": 2, "dividers_x": 9, "dividers_y": 9})).is_ok());
        assert!(plan("bin", &json!({"width": 5, "depth": 5, "dividers_x": 10, "dividers_y": 10})).unwrap_err().contains("fewer dividers"));
        assert!(plan("bin", &json!({"width": 3, "depth": 3, "dividers_x": 10, "dividers_y": 10, "inside": "Eco"})).is_err());
        let scooped = json!({"width": 7, "depth": 7, "dividers_x": 2, "dividers_y": 2, "inside_scoop": 10, "inside_label_tab": true});
        assert!(plan("bin", &scooped).is_ok());
        let e = plan("bin", &json!({"width": 6, "depth": 6, "dividers_x": 5, "dividers_y": 5, "inside_scoop": 10})).unwrap_err();
        assert!(e.contains("6 x 6 compartments with a scoop"), "{e}");
        // a drawer is a long job: a big one is built in one run
        assert!(plan("drawer", &json!({"drawer_width": 600, "drawer_depth": 500, "holes_magnet": true})).is_ok());
        assert!(plan("drawer", &json!({"drawer_width": 1000, "drawer_depth": 1000})).is_ok());
        // too big even for ten minutes: it says how to build it piece by piece
        let d = json!({"drawer_width": 1000, "drawer_depth": 1000, "grid": "21 mm (half)", "holes_magnet": true});
        let e = plan("drawer", &d).unwrap_err();
        assert!(e.contains("ten minutes") && e.contains("set Piece to 1, 2"), "{e}");
        let mut one = d.clone();
        one["piece"] = json!(1);
        assert!(plan("drawer", &one).is_ok());
    }

    #[test]
    fn cells_left_out_are_cut_away() {
        let o = parse_baseplate(&json!({"width": 3, "depth": 2, "leave_out": "3-2, 1-1 1-1", "filler_left": 5})).unwrap();
        assert_eq!(o.leave_out, vec![[1, 1], [3, 2]]);
        let c = baseplate_commands(&o);
        let at = c.iter().position(|v| v["params"]["name"] == "Gridfinity baseplate 3x2 Left out").unwrap();
        // 1-1: through the filler on the left, past the plate at the front; 3-2: past the right and back
        // each has one inner corner, rounded off: 2-1's back left and 2-2's front left
        let pts = |i: usize| c[at + i]["params"]["points"].as_array().unwrap().clone();
        assert_eq!((pts(1).len(), pts(2).len()), (12, 12));
        assert_eq!((pts(1)[0].clone(), pts(1)[1].clone()), (json!([-0.5, -0.5]), json!([47.0, -0.5])));
        assert_eq!((pts(2)[0].clone(), pts(2)[8].clone(), pts(2)[10].clone()), (json!([89.0, 46.0]), json!([93.0, 42.0]), json!([131.5, 84.5])));
        let op = |sk: &str| c.iter().find(|v| v["command"] == "solid.extrude" && v["params"]["sketch"].as_str().is_some_and(|s| s.ends_with(sk)))
            .map(|v| v["params"]["operation"].clone());
        assert_eq!(op("Left out"), Some(json!("cut")));
        // the outer corners it leaves are cut round first
        assert_eq!(op("Left out corners"), Some(json!("cut")));
        assert!(parse_baseplate(&json!({"width": 2, "depth": 1, "leave_out": "3-1"})).unwrap_err().contains("outside the plate"));
        assert!(parse_baseplate(&json!({"width": 2, "depth": 1, "leave_out": "1-1, 2-1"})).unwrap_err().contains("no cells"));
        assert!(parse_baseplate(&json!({"leave_out": "a"})).unwrap_err().contains("not a cell"));
    }

    #[test]
    fn half_cells_in_the_filler() {
        let p = json!({"width": 2, "depth": 2, "filler_left": 30, "filler_front": 21});
        let (_, cmds) = plan("baseplate", &p).unwrap();
        let sketches: Vec<_> = cmds.iter().filter(|c| c["command"] == "sketch.create")
            .filter_map(|c| c["params"]["name"].as_str()).filter(|n| n.contains("Half cell")).collect();
        // three sizes: 21 x 42 beside the cells, 42 x 21 in front, 21 x 21 in the corner
        assert_eq!(sketches.len(), 9, "{sketches:?}");
        let first = |size: &str| cmds.iter().skip_while(|c| c["params"]["name"] != format!("Gridfinity baseplate 2x2 Half cell {size} 1"))
            .nth(1).unwrap()["params"]["p0"].clone();
        assert_eq!(first("21x42"), json!([30.0 - 21.0 + 0.05, 21.0 + 0.05]));
        assert_eq!(first("21x21"), json!([30.0 - 21.0 + 0.05, 0.05]));
        assert_eq!(first("42x21"), json!([30.0 + 0.05, 0.05]));
        // each used once, so none is copied
        assert!(!cmds.iter().any(|c| c["command"] == "solid.move"));
        assert_eq!(plate::half_cell_count(&geometry::baseplate_piece(&parse_baseplate(&p).unwrap())), 5);
        let mut solid = p.clone();
        solid["leftover"] = json!("Solid");
        let (_, cmds) = plan("baseplate", &solid).unwrap();
        assert!(!cmds.iter().any(|c| c["params"]["name"].as_str().is_some_and(|n| n.contains("Half cells"))));
        assert!(parse_baseplate(&json!({"leftover": "quarter"})).is_err());
        // half cells count towards the time a run takes
        let ring = |w: u32, magnets: bool| json!({"width": w, "depth": w, "holes_magnet": magnets,
            "filler_left": 21, "filler_right": 21, "filler_front": 21, "filler_back": 21});
        assert_eq!(plate::half_cell_count(&geometry::baseplate_piece(&parse_baseplate(&ring(4, true)).unwrap())), 20);
        assert!(plan("baseplate", &ring(4, true)).is_ok());
        assert!(plan("baseplate", &ring(5, true)).is_ok());
        assert!(plan("baseplate", &ring(6, true)).unwrap_err().contains("Leftover space to Solid"));
        assert!(plan("baseplate", &ring(6, false)).is_ok());
        assert!(plan("baseplate", &ring(8, false)).is_err());
        assert!(plan("drawer", &json!({"drawer_width": 400, "drawer_depth": 300})).is_ok());
        assert!(plan("drawer", &json!({"drawer_width": 400, "drawer_depth": 340})).is_ok());
        assert!(plan("drawer", &json!({"drawer_width": 450, "drawer_depth": 380})).is_ok());
        let both = json!({"drawer_width": 1000, "drawer_depth": 1000, "holes_magnet": true, "holes_screw": true});
        let e = plan("drawer", &both).unwrap_err();
        assert!(e.contains("half cells") && e.contains("Leftover space to Solid") && e.contains("set Piece to 1, 2"), "{e}");
        // a custom grid keeps its filler solid
        let mut custom = p.clone();
        custom["grid"] = json!("Custom");
        custom["custom_size_x"] = json!(40);
        let (_, cmds) = plan("baseplate", &custom).unwrap();
        assert!(!cmds.iter().any(|c| c["params"]["name"].as_str().is_some_and(|n| n.contains("Half cells"))));
    }

    #[test]
    fn baseplate_filler_keeps_the_corner_at_the_origin() {
        let b = parse_baseplate(&json!({"width": 2, "depth": 1, "filler_left": 10, "filler_back": 7.5})).unwrap();
        assert_eq!(b.filler, [10.0, 0.0, 0.0, 7.5]);
        assert!(parse_baseplate(&json!({"filler_right": -1})).is_err());
        assert!(parse_baseplate(&json!({"filler_front": 201})).is_err());
        let (_, cmds) = plan("baseplate", &json!({"width": 2, "depth": 1, "filler_left": 10, "filler_back": 7.5})).unwrap();
        let boxes: Vec<_> = cmds.iter().filter(|c| c["command"] == "solid.box").map(|c| c["params"]["corner"].clone()).collect();
        assert_eq!(boxes, vec![json!([0.0, 0.0, 0.0]), json!([0.0, 42.0, 0.0])]);
        // no filler: the same commands as before
        let (_, a) = plan("baseplate", &json!({"width": 2})).unwrap();
        let (_, z) = plan("baseplate", &json!({"width": 2, "filler_left": 0})).unwrap();
        assert_eq!(a, z);
    }

    #[test]
    fn bin_filler_reaches_over_the_baseplate_filler() {
        let o = parse_bin(&json!({"width": 2, "filler_left": 10, "filler_back": 7.5})).unwrap();
        assert_eq!(o.filler, [10.0, 0.0, 0.0, 7.5]);
        assert!(parse_bin(&json!({"filler_right": -1})).is_err());
        assert!(parse_bin(&json!({"filler_front": 201})).is_err());
        // a bin with the baseplate's filler, at the baseplate's position, sits on its grid
        let p = json!({"width": 2, "depth": 1, "filler_left": 10, "filler_front": 5, "holes_magnet": true});
        let (_, bin) = plan("bin", &p).unwrap();
        let (_, plate) = plan("baseplate", &json!({"width": 2, "depth": 1, "filler_left": 10, "filler_front": 5, "holes_magnet": true})).unwrap();
        let magnets = |c: &[Value], d: f64| -> Vec<Value> {
            let mut v: Vec<Value> = geometry::expand(c).into_iter()
                .filter(|v| v["command"] == "sketch.circle.center" && v["params"]["diameter"] == json!(d))
                .map(|v| v["params"]["center"].clone()).collect();
            v.sort_by_key(|v| v.to_string());
            v
        };
        assert_eq!(magnets(&bin, 6.5), magnets(&plate, 6.5));
        // no filler: the same commands as before
        let (_, a) = plan("bin", &json!({"width": 2})).unwrap();
        let (_, z) = plan("bin", &json!({"width": 2, "filler_left": 0})).unwrap();
        assert_eq!(a, z);
    }

    #[test]
    fn bin_wall_thickness() {
        assert_eq!(parse_bin(&json!({"inside_wall": 2.5})).unwrap().wall, 2.5);
        assert!(parse_bin(&json!({"inside_wall": 0.3})).unwrap_err().contains("Wall thickness"));
        assert!(parse_bin(&json!({"inside_wall": 5.1})).is_err());
        // too little room left, in the bin or in a compartment
        assert!(parse_bin(&json!({"grid": "Custom", "custom_size_x": 15, "inside_wall": 5})).unwrap_err().contains("too little room"));
        assert!(parse_bin(&json!({"inside_wall": 3, "dividers_x": 7})).unwrap_err().contains("too little room"));
        assert!(parse_bin(&json!({"inside_wall": 3, "dividers_x": 3})).is_ok());
        // a thicker wall lets the fill go higher under the lip
        assert!(bin_inside_top(6, true, 2.6) > bin_inside_top(6, true, 0.0));
        // 0: the same commands as before
        let (_, a) = plan("bin", &json!({"width": 2})).unwrap();
        let (_, z) = plan("bin", &json!({"width": 2, "inside_wall": 0})).unwrap();
        assert_eq!(a, z);
    }

    /// The value of a number, or of a `{Height_units} * s mm + a mm` expression at `units`.
    fn height_at(v: &Value, units: f64) -> f64 {
        if let Some(n) = v.as_f64() {
            return n;
        }
        let s = v.as_str().unwrap();
        let w: Vec<&str> = s.split_whitespace().collect();
        assert_eq!((w[0], w[1], w[3], w[6]), ("{Height_units}", "*", "mm", "mm"), "{s}");
        let a: f64 = w[5].parse().unwrap();
        units * w[2].parse::<f64>().unwrap() + if w[4] == "-" { -a } else { a }
    }

    fn eval_height(v: &Value, units: f64) -> Value {
        match v {
            Value::String(s) if s.starts_with("{Height_units}") => json!(height_at(v, units)),
            Value::Array(x) => Value::Array(x.iter().map(|e| eval_height(e, units)).collect()),
            Value::Object(x) => Value::Object(x.iter().map(|(k, e)| (k.clone(), eval_height(e, units))).collect()),
            _ => v.clone(),
        }
    }

    fn close(a: &Value, b: &Value) -> bool {
        match (a, b) {
            (Value::Number(x), Value::Number(y)) => (x.as_f64().unwrap() - y.as_f64().unwrap()).abs() < 1e-3,
            (Value::Array(x), Value::Array(y)) => x.len() == y.len() && x.iter().zip(y).all(|(a, b)| close(a, b)),
            (Value::Object(x), Value::Object(y)) => x.len() == y.len() && x.iter().all(|(k, a)| y.get(k).is_some_and(|b| close(a, b))),
            _ => a == b,
        }
    }

    fn first_diff(a: &Value, b: &Value, at: String) -> Option<String> {
        match (a, b) {
            (Value::Array(x), Value::Array(y)) if x.len() == y.len() => x.iter().zip(y).enumerate().find_map(|(i, (a, b))| first_diff(a, b, format!("{at}/{i}"))),
            (Value::Object(x), Value::Object(y)) if x.len() == y.len() => x.iter().find_map(|(k, a)| y.get(k).and_then(|b| first_diff(a, b, format!("{at}/{k}")))),
            _ if close(a, b) => None,
            _ => Some(format!("{at}: {a} vs {b}")),
        }
    }

    #[test]
    fn height_is_a_parameter() {
        // a bin planned 3 units high, with its height set to 7 in Parameters, is the 7-unit bin
        for extra in [json!({}), json!({"stacking_lip": false}), json!({"dividers_x": 2, "dividers_y": 1, "holes_magnet": true}),
                      json!({"inside": "Solid (for custom cutouts)", "inside_fill_height": 12}),
            json!({"dividers_y": 1, "inside_scoop": 10, "inside_label_text": "M3; Nuts", "stacking_lip": false}),
            json!({"inside_label_text": "M3x10", "inside_label_text_style": "Flush (inlaid)"})] {
            let mut p = json!({"width": 2, "height": 3, "title": "B"});
            let mut q = json!({"width": 2, "height": 7, "title": "B"});
            for (k, v) in extra.as_object().unwrap() {
                p[k] = v.clone();
                q[k] = v.clone();
            }
            let (_, c) = plan("bin", &p).unwrap();
            assert_eq!(c[0]["params"]["name"], "Height_units");
            assert_eq!(c[0]["params"]["expression"], "3");
            let at7 = eval_height(&json!(c[1..]), 7.0);
            // scoops and label tabs keep the size they had at 3 units
            let seven = json!(bin_commands(&BinOptions { sized_units: 3, ..parse_bin(&q).unwrap() }));
            assert!(close(&at7, &seven), "{extra}: {}", first_diff(&at7, &seven, String::new()).unwrap_or_default());
            assert!(c.iter().any(|v| v.to_string().contains("{Height_units}")));
        }
        // the runner puts the parameter's name in the design into the expressions
        let (_, c) = plan("bin", &json!({})).unwrap();
        let mut run = Runner::default();
        run.name_params(&planned_params(&c), &["Height_units".into()]);
        let ext = c.iter().find(|v| v["command"] == "solid.extrude").unwrap();
        assert!(run.prepare(ext).1["distance"].as_str().unwrap().starts_with("Bin2_Height_units * 7 mm"));
        assert_eq!(run.prepare(&c[0]).1["name"], "Bin2_Height_units");
    }

    #[test]
    fn label_text() {
        // text adds the tab; one label per compartment, back row first
        let o = parse_bin(&json!({"width": 2, "dividers_x": 1, "inside_label_text": "M3x6; M3x8"})).unwrap();
        assert!(o.label);
        assert_eq!(o.label_text, vec!["M3x6", "M3x8"]);
        let (_, c) = plan("bin", &json!({"width": 2, "dividers_x": 1, "inside_label_text": "M3x6;; "})).unwrap();
        let texts: Vec<_> = c.iter().filter(|v| v["command"] == "sketch.text").collect();
        assert_eq!(texts.len(), 1);
        // left compartment, on the back tab
        let at = texts[0]["params"]["at"].as_array().unwrap();
        assert!(at[0].as_f64().unwrap() < 42.0 && at[1].as_f64().unwrap() > 30.0);
        // the text joins a seed block, which is cut away after
        let last = c.last().unwrap();
        assert_eq!((last["command"].as_str(), last["params"]["operation"].as_str(), last["params"]["targets"][0].as_str()),
            (Some("solid.box"), Some("cut"), Some("Gridfinity bin 2x1x3 (83.5 x 41.5 x 25.1 mm) Label")));
        // flush: sunk into the tab, then cut out of the bin keeping the text
        let (_, c) = plan("bin", &json!({"inside_label_text": "M3", "inside_label_text_style": "Flush (inlaid)"})).unwrap();
        let ext = c.iter().rev().find(|v| v["command"] == "solid.extrude").unwrap();
        assert_eq!(ext["params"]["direction"].as_str(), Some("negative"));
        let last = c.last().unwrap();
        assert_eq!((last["params"]["operation"].as_str(), last["params"]["keep_tools"].as_bool()), (Some("cut"), Some(true)));
        assert!(parse_bin(&json!({"inside_label_text_style": "Sunk"})).is_err());
        let flush = json!({"width": 3, "depth": 2, "dividers_x": 2, "dividers_y": 1, "inside_label_text_style": "Flush (inlaid)",
            "inside_label_text": "M3x6 bolt; M3x8 bolt; M3x10 bolt"});
        assert!(parse_bin(&flush).unwrap_err().contains("with 6 compartments, use at most 22"));
        // labels are text parameters, named so they don't clash with ones already in the design
        let (_, c) = plan("bin", &json!({"width": 2, "dividers_x": 1, "inside_label_text": "M3; M4", "inside_label_text_font": "DejaVu Sans"})).unwrap();
        let planned = planned_params(&c);
        assert_eq!(planned, vec!["Height_units", "Label_1", "Label_2"]);
        let mut run = Runner::default();
        run.name_params(&planned, &["Label_2".into(), "Width".into()]);
        let text = c.iter().find(|v| v["command"] == "sketch.text").unwrap();
        let (_, p) = run.prepare(text);
        assert_eq!((p["text_expression"].as_str(), p["font"].as_str()), (Some("Bin2_Label_1"), Some("DejaVu Sans")));
        let add = c.iter().find(|v| v["command"] == "parameters.add" && v["params"]["name"] == "Label_1").unwrap();
        assert_eq!(run.prepare(add).1["name"], "Bin2_Label_1");
        run.name_params(&planned, &[]);
        assert_eq!(run.prepare(add).1["name"], "Label_1");
        // a second bin of the same size gets a name of its own
        let have = vec!["Gridfinity bin 1x1x3".to_string(), "Gridfinity bin 1x1x3 Label".into(), "Gridfinity bin 1x1x3 (2)".into()];
        assert_eq!(free_name("Gridfinity bin 1x1x3", &have).as_deref(), Some("Gridfinity bin 1x1x3 (3)"));
        assert_eq!(free_name("Gridfinity bin 2x1x3", &have), None);
        assert_eq!(free_name("Gridfinity bin 1x1", &have), None);
        // no text, no text commands
        let (_, c) = plan("bin", &json!({"inside_label_tab": true})).unwrap();
        assert!(!c.iter().any(|v| v["command"] == "sketch.text"));

        assert!(parse_bin(&json!({"inside_label_text": "a; b"})).unwrap_err().contains("2 labels, but the bin has 1 compartment:"));
        // a low bin's tab stands on the floor, so it has the full width for text
        let low = json!({"width": 3, "depth": 3, "height": 2, "dividers_x": 3, "inside_label_text": "M12; M13; M14",
            "inside_label_text_style": "Flush (inlaid)"});
        assert!(parse_bin(&low).is_ok());
        // a narrow tab is too small for text: say how to fix that, not "shorter"
        let mut narrow = low.clone();
        narrow["inside_label_tab_width"] = json!(5);
        assert_eq!(parse_bin(&narrow).unwrap_err(), "A bin 2 units tall has too small a label tab for text under its stacking lip: \
            make Label tab width 6 mm, or turn Stacking lip off.");
        assert!(parse_bin(&json!({"width": 3, "depth": 3, "height": 3, "dividers_x": 3, "inside_label_text": "M12; M13; M14",
            "inside_label_text_style": "Flush (inlaid)"})).is_ok());
        assert!(parse_bin(&json!({"inside": "Solid (for custom cutouts)", "inside_label_text": "x"})).unwrap_err().contains("hollow"));
        assert!(parse_bin(&json!({"inside_label_text": "A very long label for one small cell"})).unwrap_err().contains("does not fit"));
        assert!(parse_bin(&json!({"width": 2, "inside_label_text": "M3", "inside_label_text_size": 12})).unwrap_err().contains("at 12 mm"));
        let many = vec!["M3x10"; 17].join(";");
        assert!(parse_bin(&json!({"width": 4, "depth": 4, "dividers_x": 5, "dividers_y": 5, "inside_label_text": many}))
            .unwrap_err().contains("85 letters and digits of label text take longer than SolidCraft allows a plugin (30 seconds): with 36 compartments, use at most 14"));
    }

    #[test]
    fn label_tab_none_sticker_or_printed_text() {
        let t = |v: Value| { let o = parse_bin(&v).unwrap(); (o.label, o.sticker, o.label_text.len()) };
        assert_eq!(t(json!({"inside_label_tab": "None"})), (false, false, 0));
        assert_eq!(t(json!({"inside_label_tab": "For sticker"})), (true, true, 0));
        assert_eq!(t(json!({"inside_label_tab": "Printed text", "inside_label_text": "M3", "height": 6})), (true, false, 1));
        // text goes only on a tab for printed text
        assert_eq!(t(json!({"inside_label_tab": "For sticker", "inside_label_text": "M3"})), (true, true, 0));
        assert_eq!(t(json!({"inside_label_tab": "None", "inside_label_text": "M3"})), (false, false, 0));
        // as before 1.5: true and false, and text on its own adds a tab
        assert_eq!(t(json!({"inside_label_tab": true})), (true, false, 0));
        assert_eq!(t(json!({"inside_label_text": "M3", "height": 6})), (true, false, 1));
        assert!(parse_bin(&json!({"inside_label_tab": "Big"})).unwrap_err().contains("For sticker"));
        // edits: the new names, and new text turns a sticker's tab into one for the text
        let old = json!({"inside_label_tab": "For sticker"});
        assert_eq!(apply_edits(&old, &json!({"inside_label_tab": "None"}))["inside_label_tab"], "None");
        assert_eq!(apply_edits(&old, &json!({"inside_label_tab": KEEP}))["inside_label_tab"], "For sticker");
        assert_eq!(apply_edits(&old, &json!({"inside_label_text": "M3"}))["inside_label_tab"], "Printed text");
        assert_eq!(apply_edits(&json!({"inside_label_tab": true}), &json!({"inside_label_text": "M3"}))["inside_label_tab"], true);
    }

    #[test]
    fn scoop_and_label_options() {
        let o = parse_bin(&json!({"inside_scoop": 10, "inside_label_tab": true})).unwrap();
        assert_eq!((o.scoop, o.label), (10.0, true));
        let o = parse_bin(&json!({})).unwrap();
        assert_eq!((o.scoop, o.label), (0.0, false));
        assert!(parse_bin(&json!({"inside_scoop": 41})).is_err());
        // off: the same commands as without the fields
        let (_, a) = plan("bin", &json!({"width": 2, "dividers_x": 1})).unwrap();
        let (_, z) = plan("bin", &json!({"width": 2, "dividers_x": 1, "inside_scoop": 0, "inside_label_tab": false})).unwrap();
        assert_eq!(a, z);
        // on: one YZ sketch with a profile per row and per feature, joined, then trimmed
        let (_, c) = plan("bin", &json!({"width": 2, "dividers_y": 2, "inside_scoop": 8, "inside_label_tab": true})).unwrap();
        assert_eq!(c.iter().filter(|v| v["command"] == "sketch.create" && v["params"]["plane"] == "YZ").count(), 1);
        assert_eq!(c.iter().filter(|v| v["command"] == "sketch.line").count(), 6);
        // a solid bin has no inside to put them in
        let (_, s) = plan("bin", &json!({"inside": "Solid (for custom cutouts)", "inside_scoop": 8, "inside_label_tab": true})).unwrap();
        assert!(!s.iter().any(|v| v["params"]["plane"] == "YZ"));
        // too many compartments for one run
        assert!(plan("bin", &json!({"width": 4, "depth": 4, "dividers_x": 6, "dividers_y": 6, "inside_label_tab": true})).unwrap_err().contains("49 compartments"));
        assert!(plan("bin", &json!({"width": 4, "depth": 4, "dividers_x": 6, "dividers_y": 6})).is_ok());
    }

    #[test]
    fn scoop_none_auto_custom_and_old_numbers() {
        let s = |v: Value| { let o = parse_bin(&v).unwrap(); (o.scoop, o.scoop_auto) };
        assert_eq!(s(json!({"inside_scoop": "None"})), (0.0, false));
        assert_eq!(s(json!({"inside_scoop": "Auto"})), (0.0, true));
        assert_eq!(s(json!({"inside_scoop": "Custom"})), (10.0, false));
        assert_eq!(s(json!({"inside_scoop": "Custom", "inside_scoop_radius": 6})), (6.0, false));
        assert_eq!(s(json!({"inside_scoop": 7})), (7.0, false));             // scripts from before
        assert!(parse_bin(&json!({"inside_scoop": "Big"})).is_err());
        // Auto: the smallest of the height inside, depth / 2.5 and 18 mm
        let o = parse_bin(&json!({"inside_scoop": "Auto", "height": 6})).unwrap();
        let room = o.sized_top() - o.floor();
        assert_eq!(o.scoop_radius(30.0, 0.0), 12.0);                          // shallow: depth / 2.5
        assert_eq!(o.scoop_radius(80.0, 0.0), 18.0_f64.min(room));          // deep: the cap
        let low = parse_bin(&json!({"inside_scoop": "Auto", "height": 2})).unwrap();
        assert_eq!(low.scoop_radius(80.0, 0.0), low.sized_top() - low.floor()); // low: the height inside
        assert!(plan("bin", &json!({"width": 2, "dividers_y": 1, "inside_scoop": "Auto"})).unwrap().1
            .iter().any(|v| v["params"]["name"].as_str().is_some_and(|n| n.ends_with("Scoops and labels"))));
    }

    #[test]
    fn label_tab_width() {
        let o = parse_bin(&json!({"inside_label_tab": true, "height": 6})).unwrap();
        assert_eq!(o.label_width(80.0), 12.0);                                // as before
        let o = parse_bin(&json!({"inside_label_tab": true, "inside_label_tab_width": 15, "height": 6})).unwrap();
        assert_eq!(o.label_width(80.0), 15.0);
        assert_eq!(o.label_width(20.0), 10.0);                                // at most half the compartment
        // a low bin gets the width asked for too: the tab stands on the floor
        let low = parse_bin(&json!({"inside_label_tab": true, "inside_label_tab_width": 20, "height": 2})).unwrap();
        assert!(low.sized_top() - low.floor() < 20.0);
        assert_eq!(low.label_width(80.0), 20.0);
        assert!(parse_bin(&json!({"inside_label_tab_width": 31})).is_err());
        // a wider tab leaves less room for the scoop
        let o = parse_bin(&json!({"inside_scoop": "Custom", "inside_scoop_radius": 30, "height": 6})).unwrap();
        assert_eq!(o.scoop_radius(40.0, 15.0), 24.0);
    }

    #[test]
    fn edit_scoop_and_label_tab_width() {
        let old = json!({"inside_scoop": 8, "inside_label_tab": true});
        let keep = json!({"inside_scoop": KEEP, "inside_scoop_radius": 0, "inside_label_tab_width": 0});
        assert_eq!(apply_edits(&old, &keep), old);
        let auto = apply_edits(&old, &json!({"inside_scoop": "Auto"}));
        assert_eq!(auto["inside_scoop"], "Auto");
        // Custom keeps the bin's own radius
        let c = apply_edits(&old, &json!({"inside_scoop": "Custom"}));
        assert_eq!((c["inside_scoop"].as_str(), c["inside_scoop_radius"].as_f64()), (Some("Custom"), Some(8.0)));
        // a radius on its own makes it Custom
        let r = apply_edits(&json!({"inside_scoop": "Auto"}), &json!({"inside_scoop_radius": 12}));
        assert_eq!((r["inside_scoop"].as_str(), r["inside_scoop_radius"].as_f64()), (Some("Custom"), Some(12.0)));
        let w = apply_edits(&old, &json!({"inside_label_tab_width": 14}));
        assert_eq!(w["inside_label_tab_width"], 14.0);
        assert!(parse_bin(&apply_edits(&old, &json!({"inside_scoop": "None"}))).unwrap().scoop == 0.0);
    }

    #[test]
    fn fill_height() {
        let solid = "Solid (for custom cutouts)";
        // off, or on a hollow bin: the same commands as without the field
        let (_, a) = plan("bin", &json!({"inside": solid, "height": 6})).unwrap();
        let (_, z) = plan("bin", &json!({"inside": solid, "height": 6, "inside_fill_height": 0})).unwrap();
        assert_eq!(a, z);
        let (_, h) = plan("bin", &json!({"height": 6})).unwrap();
        let (_, hf) = plan("bin", &json!({"height": 6, "inside_fill_height": 20})).unwrap();
        assert_eq!(h, hf);
        // on: a cavity cut down to the fill, under the lip support
        let (_, c) = plan("bin", &json!({"inside": solid, "height": 6, "inside_fill_height": 20})).unwrap();
        let cav = c.iter().find(|v| v["command"] == "solid.extrude" && v["params"]["sketch"].as_str().unwrap().ends_with("Cavity")).unwrap();
        let top = bin_inside_top(6, true, 0.0);
        assert!((height_at(&cav["params"]["distance"], 6.0) - (top - 20.0)).abs() < 1e-6);
        assert!(c.iter().any(|v| v["params"]["sketch"].as_str().map_or(false, |s| s.ends_with("Support1"))));
        assert!(!c.iter().any(|v| v["command"] == "solid.box"));
        // out of range
        assert!(plan("bin", &json!({"inside": solid, "height": 6, "inside_fill_height": 5})).unwrap_err().contains("Fill height"));
        assert!(plan("bin", &json!({"inside": solid, "height": 6, "inside_fill_height": top})).is_err());
        assert!(plan("bin", &json!({"inside": solid, "height": 6, "inside_fill_height": top - 1.0})).is_ok());
    }

    #[test]
    fn magnet_holes_can_be_sized_for_the_magnets() {
        let circles = |c: &[Value]| -> Vec<f64> {
            geometry::expand(c).into_iter().filter(|v| v["command"] == "sketch.circle.center")
                .filter_map(|v| v["params"]["diameter"].as_f64()).collect()
        };
        let depths = |c: &[Value]| -> Vec<f64> {
            c.iter().filter(|v| v["command"] == "solid.extrude" && v["params"]["sketch"].as_str().is_some_and(|s| s.ends_with("Magnets")))
                .filter_map(|v| v["params"]["distance"].as_f64()).collect()
        };
        // unset: the standard 6.5 x 2.4 mm
        assert_eq!(parse_bin(&json!({"holes_magnet": true})).unwrap().magnet, [6.5, 2.4]);
        for entry in ["bin", "baseplate", "drawer"] {
            let mut p = json!({"holes_magnet": true, "magnet_diameter": 6.8, "magnet_depth": 2.6});
            if entry == "drawer" {
                p["drawer_width"] = json!(100);
                p["drawer_depth"] = json!(100);
            }
            let (_, c) = plan(entry, &p).unwrap();
            assert!(circles(&c).contains(&6.8) && !circles(&c).contains(&6.5), "{entry}");
            assert!(!depths(&c).is_empty() && depths(&c).iter().all(|&d| d == 2.6), "{entry}");
            assert!(plan(entry, &json!({"holes_magnet": true, "magnet_diameter": 9})).unwrap_err().contains("magnet_diameter"));
            assert!(plan(entry, &json!({"holes_magnet": true, "magnet_depth": 0.5})).unwrap_err().contains("magnet_depth"));
        }
        // a deeper hole makes the base plate's floor thicker by as much
        let h = |d: f64| parse_baseplate(&json!({"holes_magnet": true, "magnet_depth": d})).map(|o| geometry::baseplate_piece(&o).h).unwrap();
        assert!((h(3.4) - h(2.4) - 1.0).abs() < 1e-9);
        // eco bins raise the hollow feet over a deeper hole too
        assert!(plan("bin", &json!({"inside": "Eco", "holes_magnet": true, "magnet_depth": 4})).is_ok());
        // edit: 0 keeps, a size changes it
        let stored = json!({"holes_magnet": true, "magnet_diameter": 6.8});
        assert_eq!(apply_edits(&stored, &json!({"magnet_diameter": 0, "magnet_depth": 0})), stored);
        let e = apply_edits(&stored, &json!({"magnet_diameter": 6.2, "magnet_depth": 2.2}));
        assert_eq!((e["magnet_diameter"].as_f64(), e["magnet_depth"].as_f64()), (Some(6.2), Some(2.2)));
    }

    #[test]
    fn edit_keeps_what_is_left_at_keep_and_changes_the_rest() {
        let stored = json!({"width": 3, "depth": 2, "height": 6, "holes_magnet": true, "dividers_x": 2, "inside_label_text": "A; B; C"});
        let keep = json!({"bin": [{"type": "body", "name": "B"}], "width": 0, "depth": 0, "height": 0, "dividers_x": -1, "dividers_y": -1,
            "inside_scoop": -1, "inside": KEEP, "holes_magnet": KEEP, "holes_screw": KEEP, "stacking_lip": KEEP,
            "inside_label_tab": KEEP, "inside_label_text": ""});
        assert_eq!(apply_edits(&stored, &keep), stored);
        let mut keep_text = keep.clone();
        keep_text["inside_label_text_style"] = json!(KEEP);
        keep_text["inside_label_text_size"] = json!(-1);
        keep_text["inside_label_text_height"] = json!(0);
        keep_text["inside_label_text_font"] = json!("");
        assert_eq!(apply_edits(&stored, &keep_text), stored);
        let mut text = keep_text.clone();
        text["cells_x"] = json!(4);
        text["inside_label_text_style"] = json!("Flush (inlaid)");
        text["inside_label_text_size"] = json!(0);
        text["inside_label_text_height"] = json!(0.6);
        text["inside_label_text_font"] = json!("DejaVu Sans");
        let p = apply_edits(&json!({"inside_label_text_font": "Arial", "width": 3}), &text);
        assert_eq!(p, json!({"width": 4, "inside_label_text_style": "Flush (inlaid)", "inside_label_text_size": 0.0,
            "inside_label_text_height": 0.6, "inside_label_text_font": "DejaVu Sans"}));
        text["inside_label_text_font"] = json!("-");
        assert_eq!(apply_edits(&json!({"inside_label_text_font": "Arial"}), &text)["inside_label_text_font"], json!(""));
        let mut change = keep.clone();
        change["height"] = json!(4);
        change["dividers_x"] = json!(0);
        change["holes_magnet"] = json!("No");
        change["inside"] = json!("Eco (thin walls, hollow feet)");
        change["inside_label_text"] = json!("-");
        let p = apply_edits(&stored, &change);
        assert_eq!(p, json!({"width": 3, "depth": 2, "height": 4, "holes_magnet": false, "dividers_x": 0,
            "inside_label_text": "", "inside": "Eco (thin walls, hollow feet)"}));
        assert!(plan("bin", &p).is_ok());
    }

    #[test]
    fn edit_takes_the_height_and_labels_changed_in_parameters() {
        let stored = json!({"width": 3, "height": 6, "dividers_x": 2, "inside_label_text": "A; ; C"});
        let names = json!({"Height_units": "Bin2_Height_units", "Label_1": "Bin2_Label_1", "Label_3": "Bin2_Label_3"});
        let list = vec![json!({"name": "Height_units", "value": 9.0, "expression": "9"}),
            json!({"name": "Bin2_Height_units", "value": 5.0, "expression": "5"}),
            json!({"name": "Bin2_Label_1", "expression": "\"M3\"", "value": 0.0}),
            json!({"name": "Bin2_Label_3", "expression": "C", "value": 0.0})];
        let p = with_current_values(&stored, &names, &list);
        assert_eq!(p["height"], json!(5));
        assert_eq!(p["inside_label_text"], json!("M3; ; C"));
        // nothing changed in Parameters: the settings as stored
        let list = vec![json!({"name": "Bin2_Height_units", "value": 6.0}), json!({"name": "Bin2_Label_1", "expression": "A"}),
            json!({"name": "Bin2_Label_3", "expression": "C"})];
        assert_eq!(with_current_values(&stored, &names, &list), stored);
    }

    #[test]
    fn a_drawer_built_a_piece_at_a_time_keeps_its_name() {
        let d = json!({"drawer_width": 600, "drawer_depth": 500, "piece": 2});
        assert_eq!(piece_body("drawer", &d).as_deref(), Some("Gridfinity drawer 600x500 2-1"));
        assert_eq!(piece_body("drawer", &json!({"drawer_width": 600, "drawer_depth": 500})), None);
        assert_eq!(piece_body("bin", &d), None);
    }

    #[test]
    fn a_picked_body_finds_its_bin() {
        let items = json!({"Bin": {"entry": "bin"}, "Bin (2)": {"entry": "bin", "n": 2}, "Plate": {"entry": "baseplate"}});
        assert_eq!(find_item(&items, "Bin").unwrap().0, "Bin");
        assert_eq!(find_item(&items, "Bin (2)").unwrap().0, "Bin (2)");
        assert_eq!(find_item(&items, "Bin (2) Label").unwrap().0, "Bin (2)");
        assert_eq!(find_item(&items, "Bin Label").unwrap().0, "Bin");
        assert!(find_item(&items, "Binder").is_none());
        assert!(find_item(&Value::Null, "Bin").is_none());
        let names = std::collections::HashMap::from([("Height_units".to_string(), "Height_units".to_string())]);
        let s = stored_item("drawer", &json!({"floor": [{}], "drawer_width": 300}), Some(&json!(7)), &[json!(1), json!(2)], &names);
        assert_eq!(s, json!({"entry": "drawer", "params": {"drawer_width": 300}, "folder": 7, "features": [1, 2],
            "parameters": {"Height_units": "Height_units"}}));
    }
}
