use crate::level::parse_full_save_lean;
use crate::object::ClassTables;
use crate::store::{ActorSpecific, ArrayValue, Header, PropertyValue, SaveStore, StructValue};
use anyhow::{anyhow, Context, Result};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct ParsedExtractor {
    pub instance_name: String,
    pub kind: String,
    pub world_x: f32,
    pub world_y: f32,
    pub world_z: f32,
    pub power_shards: u8,
    pub current_overclock: f32,
    pub max_overclock: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct ParsedRailPoint {
    pub location: [f32; 3],
    pub arrive_tangent: [f32; 3],
    pub leave_tangent: [f32; 3],
}

#[derive(Debug, Clone)]
pub struct ParsedRailSegment {
    pub points: Vec<ParsedRailPoint>,
}

#[derive(Debug, Clone)]
pub struct ParsedFoundation {
    pub corners: [[f32; 3]; 4],
}

#[derive(Debug, Clone)]
pub struct ParsedBeltSegment {
    pub points: Vec<ParsedRailPoint>,
}

#[derive(Debug, Clone, Default)]
pub struct ParsedSaveData {
    pub extractors: Vec<ParsedExtractor>,
    pub rails: Vec<ParsedRailSegment>,
    pub foundations: Vec<ParsedFoundation>,
    pub belts: Vec<ParsedBeltSegment>,
    pub play_duration_in_seconds: u32,
}

pub fn parse_save_data(path: &Path) -> Result<ParsedSaveData> {
    let file_data = fs::read(path)
        .with_context(|| format!("Save-Datei konnte nicht gelesen werden: {}", path.display()))?;
    let tables = ClassTables {
        conveyor_belts: Vec::new(),
    };
    let store = parse_full_save_lean(&file_data, &tables, None)
        .map_err(|error| anyhow!("Save-Datei konnte nicht geparst werden: {}", error.msg))?;

    let inventory_indices = inventory_potential_indices(&store);
    let mut extractors = Vec::new();
    for (level_index, level) in store.levels.iter().enumerate() {
        for (object_index, header) in level.headers.iter().enumerate() {
            let Header::Actor(actor) = header else {
                continue;
            };
            let kind = store.s(actor.type_path);
            if !is_resource_extractor(&kind) {
                continue;
            }
            let object = store
                .parse_object_at(level_index, object_index)
                .map_err(|error| {
                    anyhow!(
                        "Extractor-Objekt konnte nicht geparst werden: {}",
                        error.msg
                    )
                })?;
            let current_overclock = property_float(&store, &object, "mCurrentPotential")
                .unwrap_or(1.0)
                .max(1.0);
            let power_shards = property_object_ref(&store, &object, "mInventoryPotential")
                .and_then(|object_ref| inventory_indices.get(&store.s(object_ref.path_name)))
                .and_then(|&(inventory_level, inventory_index)| {
                    store
                        .parse_object_at(inventory_level, inventory_index)
                        .ok()
                        .and_then(|inventory| power_shard_count(&store, &inventory))
                })
                .unwrap_or_else(|| power_shards_from_overclock(current_overclock));
            let max_overclock = max_overclock_for_shards(power_shards);
            extractors.push(ParsedExtractor {
                instance_name: store.s(actor.instance_name),
                kind,
                world_x: actor.position[0],
                world_y: actor.position[1],
                world_z: actor.position[2],
                power_shards,
                current_overclock: current_overclock.min(max_overclock),
                max_overclock,
            });
        }
    }
    let rails = parse_rails(&store)?;
    let (foundations, belts) = parse_buildable_layers(&store);
    Ok(ParsedSaveData {
        extractors,
        rails,
        foundations,
        belts,
        play_duration_in_seconds: store.info.play_duration_in_seconds,
    })
}

fn parse_rails(store: &SaveStore) -> Result<Vec<ParsedRailSegment>> {
    let mut rails = Vec::new();
    for (level_index, level) in store.levels.iter().enumerate() {
        for (object_index, header) in level.headers.iter().enumerate() {
            let Header::Actor(actor) = header else {
                continue;
            };
            let kind = store.s(actor.type_path);
            if !is_rail_track(&kind) {
                continue;
            }
            // Rails are an optional visualization layer. If an older or
            // modded save has a track actor whose object payload is not
            // understood yet, keep the extractor/resource parse usable and
            // simply omit that rail from the map.
            let Ok(object) = store.parse_object_at(level_index, object_index) else {
                continue;
            };
            let Some(property) = object
                .properties
                .props
                .iter()
                .find(|property| store.s(property.name) == "mSplineData")
            else {
                continue;
            };
            let PropertyValue::Array(ArrayValue::Structs(spline_points)) = &property.value else {
                continue;
            };

            let points = spline_points
                .iter()
                .filter_map(|point| parse_rail_point(store, point, actor))
                .collect::<Vec<_>>();
            if points.len() >= 2 {
                rails.push(ParsedRailSegment { points });
            }
        }
    }
    Ok(rails)
}

fn parse_rail_point(
    store: &SaveStore,
    properties: &crate::store::PropList,
    actor: &crate::store::ActorHeader,
) -> Option<ParsedRailPoint> {
    let location = transform_rail_vector(
        property_vector(store, properties, "Location")?,
        actor.position,
        actor.rotation,
    );
    let arrive_tangent = rotate_rail_vector(
        property_vector(store, properties, "ArriveTangent").unwrap_or([0.0; 3]),
        actor.rotation,
    );
    let leave_tangent = rotate_rail_vector(
        property_vector(store, properties, "LeaveTangent").unwrap_or([0.0; 3]),
        actor.rotation,
    );
    if location
        .iter()
        .chain(arrive_tangent.iter())
        .chain(leave_tangent.iter())
        .any(|value| !value.is_finite())
    {
        return None;
    }
    Some(ParsedRailPoint {
        location,
        arrive_tangent,
        leave_tangent,
    })
}

fn property_vector(
    store: &SaveStore,
    properties: &crate::store::PropList,
    name: &str,
) -> Option<[f32; 3]> {
    let property = properties
        .props
        .iter()
        .find(|property| store.s(property.name) == name)?;
    let values = match &property.value {
        PropertyValue::Struct(StructValue::Vector(values)) => *values,
        PropertyValue::Array(ArrayValue::Vector(values)) => *values.first()?,
        _ => return None,
    };
    let values = [values[0] as f32, values[1] as f32, values[2] as f32];
    values
        .iter()
        .all(|value| value.is_finite())
        .then_some(values)
}

fn transform_rail_vector(local: [f32; 3], position: [f32; 3], rotation: [f32; 4]) -> [f32; 3] {
    let rotated = rotate_rail_vector(local, rotation);
    [
        rotated[0] + position[0],
        rotated[1] + position[1],
        rotated[2] + position[2],
    ]
}

fn rotate_rail_vector(vector: [f32; 3], rotation: [f32; 4]) -> [f32; 3] {
    let [mut x, mut y, mut z, mut w] = rotation;
    let length = (x * x + y * y + z * z + w * w).sqrt();
    if length > f32::EPSILON && length.is_finite() {
        x /= length;
        y /= length;
        z /= length;
        w /= length;
    } else {
        return vector;
    }
    let [vx, vy, vz] = vector;
    let tx = 2.0 * (y * vz - z * vy);
    let ty = 2.0 * (z * vx - x * vz);
    let tz = 2.0 * (x * vy - y * vx);
    [
        vx + w * tx + (y * tz - z * ty),
        vy + w * ty + (z * tx - x * tz),
        vz + w * tz + (x * ty - y * tx),
    ]
}

fn is_rail_track(type_path: &str) -> bool {
    type_path.contains("Build_RailroadTrack")
}

fn parse_buildable_layers(store: &SaveStore) -> (Vec<ParsedFoundation>, Vec<ParsedBeltSegment>) {
    let mut foundations = Vec::new();
    let mut belts = Vec::new();
    let mut seen_belts = HashSet::new();

    for (level_index, level) in store.levels.iter().enumerate() {
        for (object_index, header) in level.headers.iter().enumerate() {
            let Header::Actor(actor) = header else {
                continue;
            };
            let type_path = store.s(actor.type_path);
            let is_lightweight_subsystem =
                type_path == "/Script/FactoryGame.FGLightweightBuildableSubsystem";
            let is_conveyor_chain = type_path.contains("FGConveyorChainActor");
            if !is_lightweight_subsystem && !is_conveyor_chain {
                continue;
            }
            let Ok(object) = store.parse_object_at(level_index, object_index) else {
                continue;
            };

            match object.actor_specific {
                ActorSpecific::Lightweight { items, .. } => {
                    for group in items {
                        let build_path = store.s(group.type_path);
                        if !is_foundation_buildable(&build_path) {
                            continue;
                        }
                        let (half_width, half_height) = foundation_half_size(&build_path);
                        for instance in group.instances {
                            if let Some(corners) = foundation_corners(
                                instance.position,
                                instance.rotation,
                                half_width,
                                half_height,
                            ) {
                                foundations.push(ParsedFoundation { corners });
                            }
                        }
                    }
                }
                ActorSpecific::ConveyorChain {
                    belts: chain_belts, ..
                } => {
                    for chain_belt in chain_belts {
                        let belt_id = store.s(chain_belt.belt.path_name);
                        if !seen_belts.insert(belt_id) {
                            continue;
                        }
                        let points = chain_belt
                            .elements
                            .iter()
                            .filter_map(|element| {
                                let location = finite_vec3(element[0])?;
                                let arrive_tangent = finite_vec3(element[1])?;
                                let leave_tangent = finite_vec3(element[2])?;
                                Some(ParsedRailPoint {
                                    location,
                                    arrive_tangent,
                                    leave_tangent,
                                })
                            })
                            .collect::<Vec<_>>();
                        if points.len() >= 2 {
                            belts.push(ParsedBeltSegment { points });
                        }
                    }
                }
                _ => {}
            }
        }
    }

    (foundations, belts)
}

fn is_foundation_buildable(type_path: &str) -> bool {
    type_path.contains("/Buildable/Building/Foundation/")
}

fn foundation_half_size(type_path: &str) -> (f32, f32) {
    let build_name = type_path.split('.').next().unwrap_or(type_path);
    let dimensions = build_name
        .split('_')
        .rev()
        .find_map(|part| {
            let (width, height) = part.split_once('x')?;
            Some((width.parse::<f32>().ok()?, height.parse::<f32>().ok()?))
        })
        .unwrap_or((1.0, 1.0));
    (dimensions.0 * 500.0, dimensions.1 * 500.0)
}

fn foundation_corners(
    position: [f64; 3],
    rotation: [f64; 4],
    half_width: f32,
    half_height: f32,
) -> Option<[[f32; 3]; 4]> {
    let center = [position[0] as f32, position[1] as f32, position[2] as f32];
    if center.iter().any(|value| !value.is_finite())
        || rotation.iter().any(|value| !value.is_finite())
    {
        return None;
    }
    let local_corners = [
        [-half_width, -half_height, 0.0],
        [half_width, -half_height, 0.0],
        [half_width, half_height, 0.0],
        [-half_width, half_height, 0.0],
    ];
    let corners = local_corners.map(|corner| {
        let rotated = rotate_f64_vector(corner, rotation);
        [
            center[0] + rotated[0],
            center[1] + rotated[1],
            center[2] + rotated[2],
        ]
    });
    corners
        .iter()
        .all(|corner| corner.iter().all(|value| value.is_finite()))
        .then_some(corners)
}

fn finite_vec3(values: [f64; 3]) -> Option<[f32; 3]> {
    let values = [values[0] as f32, values[1] as f32, values[2] as f32];
    values
        .iter()
        .all(|value| value.is_finite())
        .then_some(values)
}

fn rotate_f64_vector(vector: [f32; 3], rotation: [f64; 4]) -> [f32; 3] {
    let [mut x, mut y, mut z, mut w] = rotation;
    let length = (x * x + y * y + z * z + w * w).sqrt();
    if length > f64::EPSILON && length.is_finite() {
        x /= length;
        y /= length;
        z /= length;
        w /= length;
    } else {
        return vector;
    }
    let [vx, vy, vz] = vector.map(f64::from);
    let tx = 2.0 * (y * vz - z * vy);
    let ty = 2.0 * (z * vx - x * vz);
    let tz = 2.0 * (x * vy - y * vx);
    [
        (vx + w * tx + (y * tz - z * ty)) as f32,
        (vy + w * ty + (z * tx - x * tz)) as f32,
        (vz + w * tz + (x * ty - y * tx)) as f32,
    ]
}

fn inventory_potential_indices(store: &SaveStore) -> HashMap<String, (usize, usize)> {
    let mut indices = HashMap::new();
    for (level_index, level) in store.levels.iter().enumerate() {
        for (object_index, header) in level.headers.iter().enumerate() {
            let instance_name = store.s(header.instance_name());
            if instance_name.contains("InventoryPotential") {
                indices.insert(instance_name, (level_index, object_index));
            }
        }
    }
    indices
}

fn property_float(
    store: &SaveStore,
    object: &crate::store::Object,
    property_name: &str,
) -> Option<f32> {
    object
        .properties
        .props
        .iter()
        .find(|property| store.s(property.name) == property_name)
        .and_then(|property| match &property.value {
            PropertyValue::Float(value) => Some(*value),
            PropertyValue::Double(value) => Some(*value as f32),
            _ => None,
        })
}

fn property_object_ref<'a>(
    store: &SaveStore,
    object: &'a crate::store::Object,
    property_name: &str,
) -> Option<&'a crate::store::ObjectRef> {
    object
        .properties
        .props
        .iter()
        .find(|property| store.s(property.name) == property_name)
        .and_then(|property| match &property.value {
            PropertyValue::Object(object_ref) => Some(object_ref),
            _ => None,
        })
}

fn power_shard_count(store: &SaveStore, object: &crate::store::Object) -> Option<u8> {
    let property = object
        .properties
        .props
        .iter()
        .find(|property| store.s(property.name) == "mInventoryStacks")?;
    let PropertyValue::Array(ArrayValue::Structs(items)) = &property.value else {
        return None;
    };

    let mut count = 0_u32;
    for item in items {
        let mut is_power_shard = false;
        let mut item_count = 0_u32;
        for property in &item.props {
            match store.s(property.name).as_str() {
                "Item" => {
                    if let PropertyValue::Struct(StructValue::InventoryItem { item_name, .. }) =
                        &property.value
                    {
                        is_power_shard = store.s(*item_name).contains("CrystalShard");
                    }
                }
                "NumItems" => {
                    item_count = match &property.value {
                        PropertyValue::Int(value) => (*value).max(0) as u32,
                        PropertyValue::Int64(value) => (*value).max(0) as u32,
                        PropertyValue::UInt32(value) => *value,
                        _ => 0,
                    };
                }
                _ => {}
            }
        }
        if is_power_shard {
            count = count.saturating_add(item_count);
        }
    }
    Some(count.min(3) as u8)
}

fn power_shards_from_overclock(overclock: f32) -> u8 {
    (((overclock - 1.0) / 0.5).round() as i32).clamp(0, 3) as u8
}

pub fn max_overclock_for_shards(power_shards: u8) -> f32 {
    1.0 + 0.5 * f32::from(power_shards.min(3))
}

fn is_resource_extractor(type_path: &str) -> bool {
    !type_path.contains("Equip_")
        && (type_path.contains("ResourceMiner")
            || type_path.contains("Build_Miner")
            || type_path.contains("ResourceExtractor")
            || type_path.contains("OilPump")
            || type_path.contains("FrackingExtractor")
            || type_path.contains("FrackingSmasher"))
}

#[cfg(test)]
mod tests {
    use super::{is_resource_extractor, max_overclock_for_shards, parse_save_data};
    use std::path::Path;

    #[test]
    fn recognizes_miner_actor_types() {
        assert!(is_resource_extractor(
            "/Game/Build_MinerMk3.Build_MinerMk3_C"
        ));
        assert!(is_resource_extractor(
            "/Game/FactoryGame/Buildable/Factory/OilPump/Build_OilPump.Build_OilPump_C"
        ));
        assert!(is_resource_extractor(
            "/Game/FactoryGame/Buildable/Factory/FrackingExtractor/Build_FrackingExtractor.Build_FrackingExtractor_C"
        ));
        assert!(is_resource_extractor(
            "/Game/FactoryGame/Buildable/Factory/FrackingSmasher/Build_FrackingSmasher.Build_FrackingSmasher_C"
        ));
        assert!(!is_resource_extractor(
            "/Game/Equip_ResourceMiner.Equip_ResourceMiner_C"
        ));
        assert!(!is_resource_extractor(
            "/Game/Build_Constructor.Build_Constructor_C"
        ));
        assert!(!is_resource_extractor(
            "/Game/FactoryGame/Buildable/Factory/OilRefinery/Build_OilRefinery.Build_OilRefinery_C"
        ));
        assert!(!is_resource_extractor(
            "/Game/FactoryGame/Resource/BP_FrackingCore.BP_FrackingCore_C"
        ));
    }

    #[test]
    fn parses_the_attached_save_when_available() {
        let path = Path::new(
            r#"C:\Users\nuua\AppData\Local\FactoryGame\Saved\SaveGames\76561198862437134\ Nuua_autosave_0.sav"#,
        );
        if !path.exists() {
            return;
        }
        let save_data = parse_save_data(path).expect("attached save should parse");
        assert_eq!(save_data.extractors.len(), 134);
        assert!(!save_data.rails.is_empty());
        assert!(!save_data.foundations.is_empty());
        assert!(!save_data.belts.is_empty());
        assert!(save_data
            .extractors
            .iter()
            .any(|extractor| extractor.power_shards == 3));
        assert!(save_data.extractors.iter().all(|extractor| {
            (extractor.max_overclock - max_overclock_for_shards(extractor.power_shards)).abs()
                < f32::EPSILON
        }));
    }

    #[test]
    fn power_shards_define_the_expected_overclock_limits() {
        assert_eq!(max_overclock_for_shards(0), 1.0);
        assert_eq!(max_overclock_for_shards(1), 1.5);
        assert_eq!(max_overclock_for_shards(2), 2.0);
        assert_eq!(max_overclock_for_shards(3), 2.5);
    }
}
