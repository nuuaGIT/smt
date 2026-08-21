use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum ExtractionMethod {
    #[default]
    Miner,
    OilExtractor,
    ResourceWellExtractor,
    ResourceWellCore,
    ManualDeposit,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceNode {
    pub id: String,
    pub resource: String,
    pub purity: String,
    pub world_x: f32,
    pub world_y: f32,
    pub world_z: f32,
    pub capacity_per_minute: f32,
    pub used_per_minute: f32,
    #[serde(default)]
    pub usage_overridden: bool,
    pub note: String,
    #[serde(default)]
    pub extractor_instance: Option<String>,
    #[serde(default)]
    pub extractor_kind: Option<String>,
    #[serde(default)]
    pub extraction_method: ExtractionMethod,
    #[serde(default)]
    pub power_shards: u8,
    #[serde(default = "default_overclock")]
    pub current_overclock: f32,
    #[serde(default = "default_overclock")]
    pub max_overclock: f32,
}

impl ResourceNode {
    pub fn remaining_per_minute(&self) -> f32 {
        (self.capacity_per_minute - self.used_per_minute).max(0.0)
    }

    pub fn utilization(&self) -> f32 {
        if self.capacity_per_minute <= 0.0 {
            0.0
        } else {
            (self.used_per_minute / self.capacity_per_minute).clamp(0.0, 1.0)
        }
    }

    pub fn clamp_usage_to_capacity(&mut self) {
        // Capacity is populated asynchronously from the savegame. Preserve a
        // pending allocation while it is still unknown, then sanitize all
        // finite and non-finite editor input once the capacity is available.
        if self.capacity_per_minute > 0.0 && self.capacity_per_minute.is_finite() {
            self.used_per_minute = if self.used_per_minute.is_finite() {
                self.used_per_minute.max(0.0).min(self.capacity_per_minute)
            } else {
                self.capacity_per_minute
            };
        }
    }
}

pub fn demo_nodes() -> Vec<ResourceNode> {
    vec![
        ResourceNode {
            id: "demo-iron-001".into(),
            resource: "Iron Ore".into(),
            purity: "Normal".into(),
            world_x: -220_000.0,
            world_y: 140_000.0,
            world_z: 0.0,
            capacity_per_minute: 600.0,
            used_per_minute: 500.0,
            usage_overridden: false,
            note: "Demo-Datensatz".into(),
            extractor_instance: None,
            extractor_kind: None,
            extraction_method: ExtractionMethod::Miner,
            power_shards: 0,
            current_overclock: 1.0,
            max_overclock: 1.0,
        },
        ResourceNode {
            id: "demo-iron-002".into(),
            resource: "Iron Ore".into(),
            purity: "Pure".into(),
            world_x: -95_000.0,
            world_y: 190_000.0,
            world_z: 0.0,
            capacity_per_minute: 780.0,
            used_per_minute: 0.0,
            usage_overridden: false,
            note: "Demo-Datensatz".into(),
            extractor_instance: None,
            extractor_kind: None,
            extraction_method: ExtractionMethod::Miner,
            power_shards: 0,
            current_overclock: 1.0,
            max_overclock: 1.0,
        },
        ResourceNode {
            id: "demo-coal-001".into(),
            resource: "Coal".into(),
            purity: "Normal".into(),
            world_x: 80_000.0,
            world_y: 85_000.0,
            world_z: 0.0,
            capacity_per_minute: 600.0,
            used_per_minute: 300.0,
            usage_overridden: false,
            note: "Demo-Datensatz".into(),
            extractor_instance: None,
            extractor_kind: None,
            extraction_method: ExtractionMethod::Miner,
            power_shards: 0,
            current_overclock: 1.0,
            max_overclock: 1.0,
        },
        ResourceNode {
            id: "demo-copper-001".into(),
            resource: "Copper Ore".into(),
            purity: "Impure".into(),
            world_x: 165_000.0,
            world_y: -125_000.0,
            world_z: 0.0,
            capacity_per_minute: 300.0,
            used_per_minute: 0.0,
            usage_overridden: false,
            note: "Demo-Datensatz".into(),
            extractor_instance: None,
            extractor_kind: None,
            extraction_method: ExtractionMethod::Miner,
            power_shards: 0,
            current_overclock: 1.0,
            max_overclock: 1.0,
        },
        ResourceNode {
            id: "demo-oil-001".into(),
            resource: "Crude Oil".into(),
            purity: "Pure".into(),
            world_x: 250_000.0,
            world_y: -230_000.0,
            world_z: 0.0,
            capacity_per_minute: 300.0,
            used_per_minute: 250.0,
            usage_overridden: false,
            note: "Demo-Datensatz".into(),
            extractor_instance: None,
            extractor_kind: None,
            extraction_method: ExtractionMethod::Miner,
            power_shards: 0,
            current_overclock: 1.0,
            max_overclock: 1.0,
        },
    ]
}

pub fn load_nodes() -> (Vec<ResourceNode>, String) {
    let static_path = Path::new("data/world_resource_nodes.json");
    if let Ok(contents) = fs::read_to_string(static_path) {
        if let Ok(records) = serde_json::from_str::<Vec<StaticResourceNode>>(&contents) {
            let nodes = records
                .into_iter()
                .filter(|node| node.node_type.to_lowercase() != "geyser")
                .map(ResourceNode::from_static)
                .collect();
            return (
                nodes,
                format!("Echte Welt-Daten: {}", static_path.display()),
            );
        }
    }

    let demo_path = Path::new("data/resource_nodes.json");
    match fs::read_to_string(demo_path) {
        Ok(contents) => match serde_json::from_str::<Vec<ResourceNode>>(&contents) {
            Ok(nodes) => (nodes, format!("Demo-Daten: {}", demo_path.display())),
            Err(error) => (demo_nodes(), format!("Demo-Daten: JSON-Fehler ({error})")),
        },
        Err(error) => (demo_nodes(), format!("Demo-Daten: Datei fehlt ({error})")),
    }
}

#[derive(Debug, Deserialize)]
struct StaticResourceNode {
    id: String,
    resource: String,
    purity: String,
    #[serde(rename = "displayName")]
    display_name: Option<String>,
    #[serde(rename = "nodeType")]
    node_type: String,
    x: f32,
    y: f32,
    z: f32,
}

impl ResourceNode {
    fn from_static(node: StaticResourceNode) -> Self {
        let resource = node
            .display_name
            .unwrap_or_else(|| display_name_from_descriptor(&node.resource));
        let extraction_method = extraction_method(&resource, &node.node_type);
        Self {
            id: node.id,
            resource,
            purity: capitalize(&node.purity),
            world_x: node.x,
            world_y: node.y,
            world_z: node.z,
            capacity_per_minute: 0.0,
            used_per_minute: 0.0,
            usage_overridden: false,
            note: format!("Weltobjekt: {}", node.node_type),
            extractor_instance: None,
            extractor_kind: None,
            extraction_method,
            power_shards: 0,
            current_overclock: 1.0,
            max_overclock: 1.0,
        }
    }
}

fn default_overclock() -> f32 {
    1.0
}

fn extraction_method(resource: &str, node_type: &str) -> ExtractionMethod {
    match node_type {
        "frackingSatellite" => ExtractionMethod::ResourceWellExtractor,
        "frackingCore" => ExtractionMethod::ResourceWellCore,
        "deposit" => ExtractionMethod::ManualDeposit,
        _ if resource == "Crude Oil" => ExtractionMethod::OilExtractor,
        _ => ExtractionMethod::Miner,
    }
}

fn capitalize(value: &str) -> String {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn display_name_from_descriptor(value: &str) -> String {
    let value = value.strip_prefix("Desc_").unwrap_or(value);
    let value = value.strip_suffix("_C").unwrap_or(value);
    value.replace("Ore", " Ore")
}

#[cfg(test)]
mod tests {
    use super::{demo_nodes, load_nodes, ExtractionMethod};

    #[test]
    fn loads_the_local_world_node_table() {
        let (nodes, source) = load_nodes();
        assert!(nodes.len() > 590, "expected real world data, got {source}");
        assert!(nodes.iter().all(|node| !node.note.contains("geyser")));
        assert!(nodes
            .iter()
            .any(|node| node.extraction_method == ExtractionMethod::OilExtractor));
        assert!(nodes
            .iter()
            .any(|node| { node.extraction_method == ExtractionMethod::ResourceWellExtractor }));
    }

    #[test]
    fn usage_input_is_clamped_even_for_non_finite_values() {
        let mut node = demo_nodes().remove(0);
        node.used_per_minute = f32::INFINITY;

        node.clamp_usage_to_capacity();

        assert_eq!(node.used_per_minute, node.capacity_per_minute);
        assert_eq!(node.utilization(), 1.0);
    }
}
