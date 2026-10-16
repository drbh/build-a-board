//! The `.kicad_pro` project file: net classes and DRC rule severities.

use std::path::Path;

use anyhow::{Context, Result, anyhow};
use serde_json::{Map, Value, json};

use crate::design::{Board, Rules};

/// A brand-new KiCad 10 project, used when the board has none yet.
const NEW_PROJECT: &str = include_str!("new_project.kicad_pro");

/// Write the board's net classes and rule severities into the project file
/// beside `board_path`, keeping every other setting already there.
pub fn update(project_path: &Path, board: &Board) -> Result<()> {
    let text = match std::fs::read_to_string(project_path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => NEW_PROJECT.to_owned(),
        Err(err) => return Err(err).with_context(|| format!("reading {}", project_path.display())),
    };
    let mut data: Value = serde_json::from_str(&text).with_context(|| format!("parsing {}", project_path.display()))?;
    let name = project_path.file_name().and_then(|n| n.to_str()).unwrap_or_default();
    data["meta"]["filename"] = json!(name);
    // Saving a board from pcbnew records the project's root schematic sheet.
    let schematic = project_path.with_extension("kicad_sch");
    if let (true, Some(stem)) = (schematic.is_file(), project_path.file_stem().and_then(|s| s.to_str())) {
        data["schematic"]["top_level_sheets"] = json!([{
            "filename": format!("{stem}.kicad_sch"),
            "name": stem,
            "uuid": "00000000-0000-0000-0000-000000000000"
        }]);
    }

    let net_settings = object(&mut data, &["net_settings"])?;
    let classes = net_settings.entry("classes").or_insert_with(|| json!([]));
    let classes = classes.as_array_mut().ok_or_else(|| anyhow!("net_settings.classes is not a list"))?;
    let mut default = classes.iter().find(|c| c["name"] == "Default").cloned().unwrap_or_else(default_class);
    let set_rules = |entry: &mut Value, rules: &Rules| {
        entry["track_width"] = mm(rules.track_width.0);
        entry["clearance"] = mm(rules.clearance.0);
        entry["via_diameter"] = mm(rules.via_diameter.0);
        entry["via_drill"] = mm(rules.via_drill.0);
    };
    if let Some(rules) = &board.rules {
        set_rules(&mut default, rules);
    }
    classes.clear();
    classes.push(default.clone());
    for class in &board.netclasses {
        let mut entry = default.clone();
        entry["name"] = json!(class.name);
        entry["priority"] = json!(-1);
        set_rules(&mut entry, &class.rules);
        classes.push(entry);
    }
    let patterns: Vec<Value> = board
        .netclasses
        .iter()
        .flat_map(|class| class.nets.iter().map(move |net| json!({"netclass": class.name, "pattern": net})))
        .collect();
    net_settings.insert("netclass_patterns".into(), Value::Array(patterns));

    let severities = object(&mut data, &["board", "design_settings", "rule_severities"])?;
    for (rule, severity) in &board.rule_severities {
        severities.insert(rule.clone(), json!(severity));
    }
    std::fs::write(project_path, crate::json::to_pretty(&data) + "\n")
        .with_context(|| format!("writing {}", project_path.display()))
}

fn mm(nm: i64) -> Value {
    serde_json::Number::from_f64(nm as f64 / 1e6).map_or(Value::Null, Value::Number)
}

fn default_class() -> Value {
    json!({
        "bus_width": 12, "clearance": 0.2, "diff_pair_gap": 0.25, "diff_pair_via_gap": 0.25,
        "diff_pair_width": 0.2, "line_style": 0, "microvia_diameter": 0.3, "microvia_drill": 0.1,
        "name": "Default", "pcb_color": "rgba(0, 0, 0, 0.000)", "priority": 2147483647,
        "schematic_color": "rgba(0, 0, 0, 0.000)", "track_width": 0.2, "tuning_profile": "",
        "via_diameter": 0.6, "via_drill": 0.3, "wire_width": 6
    })
}

/// The object at `path`, created as needed.
fn object<'a>(root: &'a mut Value, path: &[&str]) -> Result<&'a mut Map<String, Value>> {
    let mut node = root;
    for key in path {
        node = node
            .as_object_mut()
            .ok_or_else(|| anyhow!("project setting {key} is inside a non-object"))?
            .entry(*key)
            .or_insert_with(|| Value::Object(Map::new()));
    }
    node.as_object_mut().ok_or_else(|| anyhow!("project setting {} is not an object", path.join(".")))
}
