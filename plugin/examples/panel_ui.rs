//! Prints the panel's declarations and Quick entries, for plugin.json's `runtime.ui` and `entries`.
fn main() {
    let quick: Vec<_> = gridfinity::panel::presets().into_iter().map(|(entry, label, _, _)| serde_json::json!([entry, label])).collect();
    println!("{}", serde_json::json!({"ui": gridfinity::panel::declarations(), "quick": quick}));
}
