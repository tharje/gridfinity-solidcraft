//! The Gridfinity panel (SolidCraft 0.48+). Parts: a table of the parts the plugin made in the
//! design, from the settings it keeps there, and the settings of the row clicked. Quick: buttons
//! that make a common part with one click.

use serde_json::{json, Value};

/// One row of the parts table.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    /// The body's name, which is also the row's id.
    pub id: String,
    /// The name, shorter: without "Gridfinity " and the size in mm.
    pub name: String,
    pub kind: String,
    pub size: String,
    pub height: String,
    pub label: String,
}

fn num(p: &Value, key: &str) -> Option<f64> {
    p.get(key).and_then(|v| v.as_f64())
}

fn fmt(v: f64) -> String {
    if (v - v.round()).abs() < 1e-9 { format!("{}", v.round() as i64) } else { format!("{v}") }
}

/// "Gridfinity bin 2x1x6 (83.5 x 41.5 x 46.1 mm) (2)" as "bin 2x1x6 (2)".
pub fn short_name(name: &str) -> String {
    let mut n = name.strip_prefix("Gridfinity ").unwrap_or(name).to_string();
    if let Some(i) = n.find(" (") {
        if let Some(j) = n[i..].find(" mm)") {
            n.replace_range(i..i + j + 4, "");
        }
    }
    n
}

/// The rows for the stored items (`storage.get`'s `document.items`), sorted by name.
pub fn rows(items: &Value) -> Vec<Row> {
    let mut out: Vec<Row> = items.as_object().into_iter().flatten().filter(|(_, v)| v.is_object()).map(|(name, item)| {
        let p = &item["params"];
        let entry = item["entry"].as_str().unwrap_or("");
        let cells = || format!("{} x {}", fmt(num(p, "width").unwrap_or(1.0)), fmt(num(p, "depth").unwrap_or(1.0)));
        let (kind, size, height) = match entry {
            "bin" => {
                let eco = p["inside"].as_str().is_some_and(|s| s.starts_with("Eco"));
                let solid = p["inside"].as_str().is_some_and(|s| s.starts_with("Solid"));
                let kind = if eco { "Eco bin" } else if solid { "Solid bin" } else { "Bin" };
                (kind.to_string(), cells(), format!("{} u", fmt(num(p, "height").unwrap_or(3.0))))
            }
            "baseplate" => ("Base plate".to_string(), cells(), String::new()),
            "drawer" => {
                let piece = num(p, "piece").unwrap_or(0.0);
                let kind = if piece > 0.0 { format!("Drawer plate, piece {}", fmt(piece)) } else { "Drawer plate".to_string() };
                (kind, format!("{} x {} mm", fmt(num(p, "drawer_width").unwrap_or(300.0)), fmt(num(p, "drawer_depth").unwrap_or(300.0))), String::new())
            }
            other => (other.to_string(), String::new(), String::new()),
        };
        let label = p["inside_label_text"].as_str().unwrap_or("").split(';').map(str::trim).filter(|s| !s.is_empty())
            .collect::<Vec<_>>().join(", ");
        Row { id: name.clone(), name: short_name(name), kind, size, height, label }
    }).collect();
    out.sort_by(|a, b| a.id.to_lowercase().cmp(&b.id.to_lowercase()));
    out
}

fn yes(v: &Value) -> String {
    if v.as_bool().unwrap_or(false) { "Yes".into() } else { "No".into() }
}

fn text(v: &Value, default: &str) -> String {
    v.as_str().filter(|s| !s.is_empty()).unwrap_or(default).to_string()
}

/// The fields of the settings form below the table: key and label.
pub const DETAILS: [(&str, &str); 10] = [
    ("part", "Part"), ("size", "Size"), ("height", "Height"), ("inside", "Inside"), ("compartments", "Compartments"),
    ("scoop", "Scoop"), ("label_tab", "Label tab"), ("label", "Label text"), ("holes", "Holes"), ("lip", "Stacking lip"),
];

/// A stored part's settings for the form, by the keys of `DETAILS`; empty where they don't apply.
pub fn details(row: &Row, item: &Value) -> Vec<(&'static str, String)> {
    let p = &item["params"];
    let holes = match (p["holes_magnet"].as_bool().unwrap_or(false), p["holes_screw"].as_bool().unwrap_or(false)) {
        (true, true) => "Magnets and screws",
        (true, false) => "Magnets",
        (false, true) => "Screws",
        (false, false) => "None",
    };
    let bin = item["entry"] == "bin";
    let only_bin = |v: String| if bin { v } else { String::new() };
    let compartments = format!("{} x {}", num(p, "dividers_x").unwrap_or(0.0) as i64 + 1, num(p, "dividers_y").unwrap_or(0.0) as i64 + 1);
    vec![
        ("part", row.kind.clone()),
        ("size", row.size.clone()),
        ("height", row.height.clone()),
        ("inside", only_bin(text(&p["inside"], "Hollow"))),
        ("compartments", only_bin(compartments)),
        ("scoop", only_bin(text(&p["inside_scoop"], "None"))),
        ("label_tab", only_bin(text(&p["inside_label_tab"], "None"))),
        ("label", only_bin(row.label.clone())),
        ("holes", holes.to_string()),
        ("lip", only_bin(if p.get("stacking_lip").is_none() { "Yes".into() } else { yes(&p["stacking_lip"]) })),
    ]
}

/// The Settings tab: key, label, kind, choices and default. They apply to the Quick parts.
pub const SETTINGS: [(&str, &str, &str, &[&str], &str); 4] = [
    ("magnets", "Magnet holes", "toggle", &[], "false"),
    ("screws", "Screw holes", "toggle", &[], "false"),
    ("lip", "Stacking lip on bins", "toggle", &[], "true"),
    ("inside", "Bins", "choice", &["Hollow", "Eco (thin walls, hollow feet)"], "Hollow"),
];

/// The settings stored in the design (`document.settings`), with defaults for the rest.
pub fn settings(stored: &Value) -> Value {
    let mut out = json!({});
    for (key, _, kind, _, default) in SETTINGS {
        let v = stored.get(key).cloned().filter(|v| if kind == "toggle" { v.is_boolean() } else { v.is_string() });
        out[key] = v.unwrap_or_else(|| if kind == "toggle" { json!(default == "true") } else { json!(default) });
    }
    out
}

/// A Quick part's settings with the Settings tab's: holes on every part, the lip and the
/// inside on bins (an eco preset stays eco).
pub fn with_settings(what: &str, preset: &Value, s: &Value) -> Value {
    let mut p = preset.clone();
    p["holes_magnet"] = s["magnets"].clone();
    p["holes_screw"] = s["screws"].clone();
    if what == "bin" {
        p["stacking_lip"] = s["lip"].clone();
        if p.get("inside").is_none() {
            p["inside"] = s["inside"].clone();
        }
    }
    p
}

/// The Quick tab's buttons: entry, label, the plugin entry it runs and its settings.
pub fn presets() -> Vec<(&'static str, &'static str, &'static str, Value)> {
    vec![
        ("quick-bin-1x1", "Bin 1 x 1 x 6", "bin", json!({"width": 1, "depth": 1, "height": 6})),
        ("quick-bin-scoop", "Bin 2 x 1 x 6, scoop and sticker tab", "bin",
         json!({"width": 2, "depth": 1, "height": 6, "inside_scoop": "Auto", "inside_label_tab": "For sticker"})),
        ("quick-bin-4", "Bin 2 x 2 x 6, four compartments", "bin", json!({"width": 2, "depth": 2, "height": 6, "dividers_x": 1, "dividers_y": 1})),
        ("quick-bin-eco", "Eco bin 3 x 1 x 3", "bin", json!({"width": 3, "depth": 1, "height": 3, "inside": "Eco (thin walls, hollow feet)"})),
        ("quick-plate", "Base plate 4 x 4, magnet holes", "baseplate", json!({"width": 4, "depth": 4, "holes_magnet": true})),
    ]
}

/// The entries the panel runs, and their effect. `panel` also has a ribbon button.
pub fn entries() -> Vec<(&'static str, &'static str)> {
    let mut v = vec![("panel", "read"), ("panel-refresh", "read"), ("panel-select", "read")];
    v.extend(presets().into_iter().map(|p| (p.0, "edit")));
    v.extend([("layout-remove", "edit"), ("layout-restore", "edit"), ("settings-change", "edit")]);
    v
}

/// The node the Layout tab starts at: after the Quick buttons.
const LAYOUT: usize = 8 + 5;

/// The panel's declarations, as in plugin.json's `runtime.ui`.
pub fn declarations() -> Value {
    let col = |k: &str, l: &str| json!({"key": k, "label": l, "unit": null});
    let mut nodes = vec![
        json!({"widget": {"tabs": [{"label": "Parts", "child": 1}, {"label": "Quick", "child": 6}, {"label": "Layout", "child": LAYOUT}, {"label": "Settings", "child": LAYOUT + 3}]}}),
        // the table, and beside it the settings of the row clicked
        json!({"widget": {"split": {"vertical": false, "first": 5, "second": 4}}}),
        json!({"widget": {"toolbar": [{"label": "Refresh", "icon": null, "entry": "panel-refresh"}]}}),
        json!({"widget": {"table": {"bind": "/parts", "row_id": "id",
            "columns": [col("name", "Name"), col("kind", "Part"), col("size", "Size"), col("height", "Height"), col("label", "Label")],
            "select": "single", "on_select": "panel-select", "on_edit": null}}}),
        json!({"widget": {"form": {"bind": "/details", "fields": DETAILS.iter().map(|(k, l)|
            json!({"key": k, "label": l, "kind": "text", "choices": [], "on_change": null})).collect::<Vec<_>>()}}}),
    ];
    nodes.push(json!({"widget": {"group": [2, 3]}}));
    let quick: Vec<usize> = (0..presets().len() + 1).map(|i| 7 + i).collect();
    nodes.push(json!({"widget": {"group": quick}}));
    nodes.push(json!({"widget": {"text": "One click makes the part beside what is already in the design. Change a bin afterwards with Edit bin."}}));
    for (entry, label, _, _) in presets() {
        nodes.push(json!({"widget": {"button": {"label": label, "icon": null, "entry": entry}}}));
    }
    assert_eq!(nodes.len(), LAYOUT);
    let n = nodes.len();
    nodes.push(json!({"widget": {"group": [n + 1, n + 2]}}));
    nodes.push(json!({"widget": {"text": "Make a base plate that fits a space that isn't a rectangle: click Remove a cell, then a pocket of a \
        base plate in the model, and it is made again without that cell. Restore all cells: click it, then the plate."}}));
    nodes.push(json!({"widget": {"form": {"bind": "/layout", "fields": [
        {"key": "remove", "label": "Remove a cell", "kind": "selection", "choices": [], "on_change": "layout-remove"},
        {"key": "restore", "label": "Restore all cells", "kind": "selection", "choices": [], "on_change": "layout-restore"},
        {"key": "status", "label": "Result", "kind": "text", "choices": [], "on_change": null},
    ]}}}));
    let n = nodes.len();
    nodes.push(json!({"widget": {"group": [n + 1, n + 2]}}));
    nodes.push(json!({"widget": {"text": "For the parts the Quick tab makes, kept in this design."}}));
    nodes.push(json!({"widget": {"form": {"bind": "/settings", "fields": SETTINGS.iter().map(|(k, l, kind, choices, _)|
        json!({"key": k, "label": l, "kind": kind, "choices": choices, "on_change": "settings-change"})).collect::<Vec<_>>()}}}));
    json!({"ui_version": 1, "panels": [{"id": "parts", "title": "Gridfinity", "zones": ["bottom", "right"],
        "layout": {"root": 0, "nodes": nodes}}], "workspaces": []})
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn rows_say_what_each_part_is() {
        let items = json!({
            "Gridfinity bin 2x1x6": {"entry": "bin", "params": {"width": 2, "depth": 1, "height": 6, "inside": "Eco (thin walls, hollow feet)",
                                                                 "inside_label_text": "M3; M4"}},
            "Gridfinity drawer 400x300": {"entry": "drawer", "params": {"drawer_width": 400, "drawer_depth": 300}},
            "Base": {"entry": "baseplate", "params": {"width": 4, "depth": 3}},
            "gone": null
        });
        let r = rows(&items);
        assert_eq!(r.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(), ["Base", "Gridfinity bin 2x1x6", "Gridfinity drawer 400x300"]);
        assert_eq!(r[1], Row { id: "Gridfinity bin 2x1x6".into(), name: "bin 2x1x6".into(), kind: "Eco bin".into(), size: "2 x 1".into(), height: "6 u".into(), label: "M3, M4".into() });
        assert_eq!((r[0].kind.as_str(), r[0].size.as_str()), ("Base plate", "4 x 3"));
        assert_eq!(r[2].size, "400 x 300 mm");
        assert_eq!(short_name("Gridfinity bin 2x1x6 (83.5 x 41.5 x 46.1 mm)"), "bin 2x1x6");
        assert_eq!(short_name("Base"), "Base");
        assert_eq!(short_name("Gridfinity bin 2x1x6 (83.5 x 41.5 x 46.1 mm) (2)"), "bin 2x1x6 (2)");
        let d = details(&r[1], &items["Gridfinity bin 2x1x6"]);
        assert_eq!(d.iter().find(|(k, _)| *k == "inside").unwrap().1, "Eco (thin walls, hollow feet)");
        assert_eq!(d.iter().find(|(k, _)| *k == "compartments").unwrap().1, "1 x 1");
        assert_eq!(details(&r[0], &items["Base"]).iter().find(|(k, _)| *k == "inside").unwrap().1, "");
    }

    #[test]
    fn quick_parts_take_the_settings() {
        let s = settings(&json!({"magnets": true, "inside": "Eco (thin walls, hollow feet)", "lip": "no"}));
        assert_eq!(s, json!({"magnets": true, "screws": false, "lip": true, "inside": "Eco (thin walls, hollow feet)"}));
        let (_, _, what, preset) = presets().into_iter().find(|p| p.0 == "quick-bin-1x1").unwrap();
        let p = with_settings(what, &preset, &s);
        assert_eq!((p["holes_magnet"].as_bool(), p["inside"].as_str()), (Some(true), Some("Eco (thin walls, hollow feet)")));
        let plate = with_settings("baseplate", &json!({"width": 4}), &s);
        assert!(plate.get("inside").is_none() && plate["holes_magnet"] == true);
    }

    /// plugin.json's panel and entries are the ones made here.
    #[test]
    fn manifest_has_the_panel() {
        let m: Value = serde_json::from_str(include_str!("../../package/gridfinity/plugin.json")).unwrap();
        assert_eq!(m["runtime"]["ui"], declarations());
        let entries = m["runtime"]["entries"].as_array().unwrap();
        for (entry, effect) in super::entries() {
            assert!(entries.iter().any(|e| e["name"] == entry && e["effect"] == effect), "{entry}");
        }
    }
}
