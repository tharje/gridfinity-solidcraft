//! Prints the panel's declarations and entries, for plugin.json's `runtime.ui` and `entries`.
fn main() {
    let entries: Vec<_> = gridfinity::panel::entries().into_iter().map(|(name, effect)| serde_json::json!([name, effect])).collect();
    println!("{}", serde_json::json!({"ui": gridfinity::panel::declarations(), "entries": entries}));
}
