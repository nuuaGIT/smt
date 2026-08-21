use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

const APP_QUALIFIER: &str = "com";
const APP_ORGANIZATION: &str = "personal";
const APP_APPLICATION: &str = "satisfactory-resource-tracker";
const ACTIVE_SAVE_NAME: &str = "active_save.sav";
const STATE_NAME: &str = "state.json";
const CURRENT_STATE_VERSION: u32 = 2;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Language {
    English,
    German,
    French,
    Spanish,
}

impl Default for Language {
    fn default() -> Self {
        Self::English
    }
}

impl Language {
    pub fn native_name(self) -> &'static str {
        match self {
            Self::English => "English",
            Self::German => "Deutsch",
            Self::French => "Français",
            Self::Spanish => "Español",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SaveSnapshot {
    pub file_name: String,
    pub byte_length: u64,
    pub sha256: String,
}

impl SaveSnapshot {
    pub fn from_bytes(file_name: impl Into<String>, bytes: &[u8]) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(bytes);

        Self {
            file_name: file_name.into(),
            byte_length: bytes.len() as u64,
            sha256: hex::encode(hasher.finalize()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DiffSummary {
    pub changed_bytes: u64,
    pub first_changed_offset: Option<u64>,
    pub changed_ranges: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NodeAllocation {
    pub capacity_per_minute: f32,
    pub used_per_minute: f32,
    pub note: String,
    /// `None` means a legacy allocation. Legacy positive values are treated
    /// as explicit usage; legacy zero values use the new claimed-node default.
    #[serde(default)]
    pub usage_overridden: Option<bool>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct MapRectangle {
    pub min_world_x: f32,
    pub min_world_y: f32,
    pub max_world_x: f32,
    pub max_world_y: f32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct MapCircle {
    pub center_world_x: f32,
    pub center_world_y: f32,
    pub radius_world: f32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct MapArrow {
    pub start_world_x: f32,
    pub start_world_y: f32,
    pub end_world_x: f32,
    pub end_world_y: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MapText {
    pub world_x: f32,
    pub world_y: f32,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MapStroke {
    pub points: Vec<[f32; 2]>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MapStrokeErase {
    pub original: MapStroke,
    pub replacement: Vec<MapStroke>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MapAnnotation {
    Rectangle(MapRectangle),
    Circle(MapCircle),
    Arrow(MapArrow),
    Text(MapText),
    Stroke(MapStroke),
    StrokeErase(MapStrokeErase),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    #[serde(default)]
    pub language: Language,
    #[serde(default = "default_show_node_names")]
    pub show_node_names: bool,
    #[serde(default)]
    pub debug_mode: bool,
    #[serde(default = "default_pause_map_when_unfocused")]
    pub pause_map_when_unfocused: bool,
    #[serde(default = "default_resource_filter")]
    pub resource_filter: String,
    /// `None` keeps the legacy/default meaning of all resources selected.
    /// `Some(empty)` is therefore a valid explicit selection: no resources.
    #[serde(default)]
    pub selected_resources: Option<Vec<String>>,
    #[serde(default = "default_purity_filter")]
    pub purity_filter: String,
    #[serde(default)]
    pub only_claimed: bool,
    #[serde(default = "default_node_scale")]
    pub node_scale: f32,
    #[serde(default)]
    pub show_grid: bool,
    #[serde(default = "default_show_annotations")]
    pub show_annotations: bool,
    #[serde(default)]
    pub only_partial: bool,
    #[serde(default)]
    pub show_rails: bool,
    #[serde(default)]
    pub show_foundations: bool,
    #[serde(default)]
    pub show_belts: bool,
    #[serde(default = "default_right_panel_width")]
    pub right_panel_width: f32,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            language: Language::English,
            show_node_names: false,
            debug_mode: false,
            pause_map_when_unfocused: true,
            resource_filter: "Alle Ressourcen".to_owned(),
            selected_resources: None,
            purity_filter: "Alle Reinheiten".to_owned(),
            only_claimed: false,
            node_scale: 1.0,
            show_grid: false,
            show_annotations: true,
            only_partial: false,
            show_rails: false,
            show_foundations: false,
            show_belts: false,
            right_panel_width: 320.0,
        }
    }
}

fn default_show_node_names() -> bool {
    false
}

fn default_pause_map_when_unfocused() -> bool {
    true
}

fn default_right_panel_width() -> f32 {
    320.0
}

fn default_resource_filter() -> String {
    "Alle Ressourcen".to_owned()
}

fn default_purity_filter() -> String {
    "Alle Reinheiten".to_owned()
}

fn default_node_scale() -> f32 {
    1.0
}

fn default_show_annotations() -> bool {
    true
}

impl DiffSummary {
    pub fn between(old: &[u8], new: &[u8]) -> Self {
        let mut changed_bytes = 0;
        let mut changed_ranges = 0;
        let mut in_range = false;
        let max_len = old.len().max(new.len());
        let mut first_changed_offset = None;

        for index in 0..max_len {
            let different = old.get(index) != new.get(index);
            if different {
                changed_bytes += 1;
                first_changed_offset.get_or_insert(index as u64);
                if !in_range {
                    changed_ranges += 1;
                    in_range = true;
                }
            } else {
                in_range = false;
            }
        }

        Self {
            changed_bytes,
            first_changed_offset,
            changed_ranges,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistentState {
    #[serde(default = "current_state_version")]
    pub schema_version: u32,
    pub source_path: Option<PathBuf>,
    pub current_snapshot: Option<SaveSnapshot>,
    pub previous_snapshot: Option<SaveSnapshot>,
    pub last_diff: Option<DiffSummary>,
    #[serde(default)]
    pub node_allocations: BTreeMap<String, NodeAllocation>,
    #[serde(default)]
    pub allocations_by_save: BTreeMap<String, BTreeMap<String, NodeAllocation>>,
    #[serde(default)]
    pub miner_tier_by_save: BTreeMap<String, u8>,
    #[serde(default)]
    pub resource_order_by_save: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub rectangles_by_save: BTreeMap<String, Vec<MapRectangle>>,
    #[serde(default)]
    pub circles_by_save: BTreeMap<String, Vec<MapCircle>>,
    #[serde(default)]
    pub arrows_by_save: BTreeMap<String, Vec<MapArrow>>,
    #[serde(default)]
    pub texts_by_save: BTreeMap<String, Vec<MapText>>,
    #[serde(default)]
    pub strokes_by_save: BTreeMap<String, Vec<MapStroke>>,
    #[serde(default)]
    pub drawing_history_by_save: BTreeMap<String, Vec<MapAnnotation>>,
    #[serde(default)]
    pub settings: AppSettings,
    /// Preserve fields introduced by newer builds so opening and saving a
    /// state file with an older build does not silently discard them.
    #[serde(flatten)]
    pub future_fields: BTreeMap<String, serde_json::Value>,
}

impl Default for PersistentState {
    fn default() -> Self {
        Self {
            schema_version: CURRENT_STATE_VERSION,
            source_path: None,
            current_snapshot: None,
            previous_snapshot: None,
            last_diff: None,
            node_allocations: BTreeMap::new(),
            allocations_by_save: BTreeMap::new(),
            miner_tier_by_save: BTreeMap::new(),
            resource_order_by_save: BTreeMap::new(),
            rectangles_by_save: BTreeMap::new(),
            circles_by_save: BTreeMap::new(),
            arrows_by_save: BTreeMap::new(),
            texts_by_save: BTreeMap::new(),
            strokes_by_save: BTreeMap::new(),
            drawing_history_by_save: BTreeMap::new(),
            settings: AppSettings::default(),
            future_fields: BTreeMap::new(),
        }
    }
}

#[derive(Debug)]
pub struct Storage {
    active_save_path: PathBuf,
    state_path: PathBuf,
    pub state: PersistentState,
}

impl Storage {
    pub fn load() -> Result<Self> {
        let project_dirs = ProjectDirs::from(APP_QUALIFIER, APP_ORGANIZATION, APP_APPLICATION)
            .context("Konnte keinen lokalen App-Datenordner bestimmen")?;

        let data_dir = project_dirs.data_local_dir().to_path_buf();
        fs::create_dir_all(&data_dir)
            .with_context(|| format!("Konnte Datenordner nicht anlegen: {}", data_dir.display()))?;

        let state_path = data_dir.join(STATE_NAME);
        let (state, state_was_migrated) = if state_path.exists() {
            let state_json = fs::read_to_string(&state_path).with_context(|| {
                format!("Konnte Statusdatei nicht lesen: {}", state_path.display())
            })?;
            load_state_compatibly(&state_json)
                .with_context(|| format!("Statusdatei ist ungültig: {}", state_path.display()))?
        } else {
            (PersistentState::default(), false)
        };

        let mut storage = Self {
            active_save_path: data_dir.join(ACTIVE_SAVE_NAME),
            state_path,
            state,
        };

        if !storage.state.node_allocations.is_empty() {
            if let Some(source_path) = storage.state.source_path.clone() {
                let profile_key = allocation_profile_key(&source_path);
                storage.state.allocations_by_save.insert(
                    profile_key,
                    std::mem::take(&mut storage.state.node_allocations),
                );
                storage.persist()?;
            }
        }

        if state_was_migrated {
            storage.state.schema_version = CURRENT_STATE_VERSION;
            storage.persist()?;
        }

        storage.reload_source_on_startup()?;

        Ok(storage)
    }

    pub fn active_save_path(&self) -> &Path {
        &self.active_save_path
    }

    pub fn active_bytes(&self) -> Result<Vec<u8>> {
        fs::read(&self.active_save_path).with_context(|| {
            format!(
                "Konnte gespeichertes Savegame nicht lesen: {}",
                self.active_save_path.display()
            )
        })
    }

    pub fn install_new_save(&mut self, source_path: &Path) -> Result<SaveSnapshot> {
        let bytes = read_save_file(source_path)?;
        let file_name = source_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("save.sav")
            .to_owned();
        let snapshot = SaveSnapshot::from_bytes(file_name, &bytes);

        fs::copy(source_path, &self.active_save_path).with_context(|| {
            format!(
                "Konnte Savegame nicht in den App-Datenordner kopieren: {}",
                self.active_save_path.display()
            )
        })?;

        self.state.previous_snapshot = self.state.current_snapshot.clone();
        self.state.current_snapshot = Some(snapshot.clone());
        self.state.last_diff = None;
        self.state.source_path = Some(source_path.to_path_buf());
        self.persist()?;

        Ok(snapshot)
    }

    pub fn clear_active_save(&mut self) -> Result<()> {
        if self.active_save_path.exists() {
            fs::remove_file(&self.active_save_path).with_context(|| {
                format!(
                    "Konnte die lokale SMT-Savegame-Kopie nicht entfernen: {}",
                    self.active_save_path.display()
                )
            })?;
        }
        self.state.source_path = None;
        self.state.current_snapshot = None;
        self.state.previous_snapshot = None;
        self.state.last_diff = None;
        self.state.node_allocations.clear();
        self.persist()
    }

    pub fn delete_all_smt_notes(&mut self) -> Result<()> {
        if self.active_save_path.exists() {
            fs::remove_file(&self.active_save_path).with_context(|| {
                format!(
                    "Konnte die lokale SMT-Savegame-Kopie nicht entfernen: {}",
                    self.active_save_path.display()
                )
            })?;
        }

        self.state.source_path = None;
        self.state.current_snapshot = None;
        self.state.previous_snapshot = None;
        self.state.last_diff = None;
        self.state.node_allocations.clear();
        self.state.allocations_by_save.clear();
        self.state.miner_tier_by_save.clear();
        self.state.resource_order_by_save.clear();
        self.state.rectangles_by_save.clear();
        self.state.circles_by_save.clear();
        self.state.arrows_by_save.clear();
        self.state.texts_by_save.clear();
        self.state.strokes_by_save.clear();
        self.state.drawing_history_by_save.clear();
        self.persist()
    }

    pub fn refresh(&mut self) -> Result<RefreshResult> {
        let source_path = self
            .state
            .source_path
            .clone()
            .context("Es ist noch kein Quell-Savegame ausgewählt")?;

        let new_bytes = match read_save_file(&source_path) {
            Ok(bytes) => bytes,
            Err(error) => {
                self.clear_unavailable_source()?;
                return Err(error).with_context(|| {
                    format!(
                        "Die gespeicherte Quelle ist nicht verfügbar: {}",
                        source_path.display()
                    )
                });
            }
        };
        let file_name = source_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("save.sav")
            .to_owned();
        let new_snapshot = SaveSnapshot::from_bytes(file_name, &new_bytes);
        let old_bytes = self.active_bytes()?;

        if self.state.current_snapshot.as_ref() == Some(&new_snapshot) {
            return Ok(RefreshResult::Unchanged);
        }

        let diff = DiffSummary::between(&old_bytes, &new_bytes);
        fs::copy(&source_path, &self.active_save_path).with_context(|| {
            format!(
                "Konnte aktualisiertes Savegame nicht speichern: {}",
                self.active_save_path.display()
            )
        })?;

        self.state.previous_snapshot = self.state.current_snapshot.clone();
        self.state.current_snapshot = Some(new_snapshot);
        self.state.last_diff = Some(diff.clone());
        self.persist()?;

        Ok(RefreshResult::Updated(diff))
    }

    fn reload_source_on_startup(&mut self) -> Result<()> {
        let Some(source_path) = self.state.source_path.clone() else {
            return Ok(());
        };
        let Ok(new_bytes) = read_save_file(&source_path) else {
            self.clear_unavailable_source()?;
            return Ok(());
        };

        let file_name = source_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("save.sav")
            .to_owned();
        let new_snapshot = SaveSnapshot::from_bytes(file_name, &new_bytes);
        let needs_copy = self.state.current_snapshot.as_ref() != Some(&new_snapshot)
            || !self.active_save_path.exists();
        if !needs_copy {
            return Ok(());
        }

        let old_bytes = fs::read(&self.active_save_path).unwrap_or_default();
        let diff = if old_bytes.is_empty() {
            DiffSummary::default()
        } else {
            DiffSummary::between(&old_bytes, &new_bytes)
        };
        fs::copy(&source_path, &self.active_save_path).with_context(|| {
            format!(
                "Konnte aktuelles Savegame nicht aktualisieren: {}",
                self.active_save_path.display()
            )
        })?;
        self.state.previous_snapshot = self.state.current_snapshot.clone();
        self.state.current_snapshot = Some(new_snapshot);
        self.state.last_diff = Some(diff);
        self.persist()?;
        Ok(())
    }

    fn clear_unavailable_source(&mut self) -> Result<()> {
        self.clear_active_save()
    }

    pub fn save_node_allocations(
        &mut self,
        allocations: BTreeMap<String, NodeAllocation>,
    ) -> Result<()> {
        let profile_key = self.active_profile_key();
        self.state
            .allocations_by_save
            .insert(profile_key, allocations);
        self.persist()
    }

    pub fn active_node_allocations(&self) -> BTreeMap<String, NodeAllocation> {
        self.state
            .allocations_by_save
            .get(&self.active_profile_key())
            .cloned()
            .unwrap_or_default()
    }

    pub fn active_miner_tier(&self) -> u8 {
        self.state
            .miner_tier_by_save
            .get(&self.active_profile_key())
            .copied()
            .unwrap_or(3)
            .clamp(1, 3)
    }

    pub fn save_active_miner_tier(&mut self, tier: u8) -> Result<()> {
        let profile_key = self.active_profile_key();
        self.state
            .miner_tier_by_save
            .insert(profile_key, tier.clamp(1, 3));
        self.persist()
    }

    pub fn active_resource_order(&self) -> Vec<String> {
        self.state
            .resource_order_by_save
            .get(&self.active_profile_key())
            .cloned()
            .unwrap_or_default()
    }

    pub fn save_active_resource_order(&mut self, order: Vec<String>) -> Result<()> {
        let profile_key = self.active_profile_key();
        self.state.resource_order_by_save.insert(profile_key, order);
        self.persist()
    }

    pub fn active_rectangles(&self) -> Vec<MapRectangle> {
        self.state
            .rectangles_by_save
            .get(&self.active_profile_key())
            .cloned()
            .unwrap_or_default()
    }

    pub fn save_active_rectangles(&mut self, rectangles: Vec<MapRectangle>) -> Result<()> {
        let profile_key = self.active_profile_key();
        self.state
            .rectangles_by_save
            .insert(profile_key, rectangles);
        self.persist()
    }

    pub fn active_circles(&self) -> Vec<MapCircle> {
        self.state
            .circles_by_save
            .get(&self.active_profile_key())
            .cloned()
            .unwrap_or_default()
    }

    pub fn save_active_circles(&mut self, circles: Vec<MapCircle>) -> Result<()> {
        let profile_key = self.active_profile_key();
        self.state.circles_by_save.insert(profile_key, circles);
        self.persist()
    }

    pub fn active_arrows(&self) -> Vec<MapArrow> {
        self.state
            .arrows_by_save
            .get(&self.active_profile_key())
            .cloned()
            .unwrap_or_default()
    }

    pub fn save_active_arrows(&mut self, arrows: Vec<MapArrow>) -> Result<()> {
        let profile_key = self.active_profile_key();
        self.state.arrows_by_save.insert(profile_key, arrows);
        self.persist()
    }

    pub fn active_texts(&self) -> Vec<MapText> {
        self.state
            .texts_by_save
            .get(&self.active_profile_key())
            .cloned()
            .unwrap_or_default()
    }

    pub fn save_active_texts(&mut self, texts: Vec<MapText>) -> Result<()> {
        let profile_key = self.active_profile_key();
        self.state.texts_by_save.insert(profile_key, texts);
        self.persist()
    }

    pub fn active_strokes(&self) -> Vec<MapStroke> {
        self.state
            .strokes_by_save
            .get(&self.active_profile_key())
            .cloned()
            .unwrap_or_default()
    }

    pub fn save_active_strokes(&mut self, strokes: Vec<MapStroke>) -> Result<()> {
        let profile_key = self.active_profile_key();
        self.state.strokes_by_save.insert(profile_key, strokes);
        self.persist()
    }

    pub fn active_drawing_history(&self) -> Vec<MapAnnotation> {
        self.state
            .drawing_history_by_save
            .get(&self.active_profile_key())
            .cloned()
            .unwrap_or_default()
    }

    pub fn save_active_drawing_history(&mut self, history: Vec<MapAnnotation>) -> Result<()> {
        let profile_key = self.active_profile_key();
        self.state
            .drawing_history_by_save
            .insert(profile_key, history);
        self.persist()
    }

    pub fn save_settings(&mut self, settings: AppSettings) -> Result<()> {
        self.state.settings = settings;
        self.persist()
    }

    fn active_profile_key(&self) -> String {
        self.state
            .source_path
            .as_deref()
            .map(allocation_profile_key)
            .unwrap_or_else(|| "__no_save__".to_owned())
    }

    fn persist(&self) -> Result<()> {
        let json = serde_json::to_string_pretty(&self.state)
            .context("Konnte App-Status nicht serialisieren")?;
        fs::write(&self.state_path, json).with_context(|| {
            format!(
                "Konnte Status nicht speichern: {}",
                self.state_path.display()
            )
        })?;
        Ok(())
    }
}

#[derive(Debug)]
pub enum RefreshResult {
    Unchanged,
    Updated(DiffSummary),
}

fn read_save_file(path: &Path) -> Result<Vec<u8>> {
    let extension = path.extension().and_then(|extension| extension.to_str());
    anyhow::ensure!(
        extension.is_some_and(|value| value.eq_ignore_ascii_case("sav")),
        "Bitte eine Satisfactory-Savegame-Datei mit der Endung .sav auswählen"
    );

    let bytes = fs::read(path)
        .with_context(|| format!("Konnte Savegame nicht lesen: {}", path.display()))?;
    anyhow::ensure!(!bytes.is_empty(), "Die ausgewählte Savegame-Datei ist leer");
    Ok(bytes)
}

fn allocation_profile_key(path: &Path) -> String {
    path.to_string_lossy().to_ascii_lowercase()
}

fn current_state_version() -> u32 {
    CURRENT_STATE_VERSION
}

/// Load the state through a small compatibility boundary instead of relying on
/// serde's exact representation forever. Old files are unversioned and are
/// treated as version 1. Unknown fields are intentionally ignored so newer
/// state files can still be opened by an older build when their known fields
/// are unchanged.
fn load_state_compatibly(json: &str) -> Result<(PersistentState, bool)> {
    let mut value: serde_json::Value =
        serde_json::from_str(json).context("Statusdatei enthält kein gültiges JSON")?;
    let object = value
        .as_object_mut()
        .context("Statusdatei muss ein JSON-Objekt sein")?;

    let version = object
        .get("schema_version")
        .or_else(|| object.get("version"))
        .and_then(serde_json::Value::as_u64)
        .map(|value| value.min(u32::MAX as u64) as u32)
        .unwrap_or(1);
    let had_schema_version = object.contains_key("schema_version");

    // Version 1 was the unversioned format. Version 2 only formalizes the
    // schema and adds defaults, so serde can safely fill all new fields.
    if !had_schema_version {
        object.remove("version");
    }
    object.insert(
        "schema_version".to_owned(),
        serde_json::Value::from(CURRENT_STATE_VERSION),
    );

    let state: PersistentState = serde_json::from_value(value)
        .context("bekannte Statusfelder haben ein ungültiges Format")?;
    Ok((
        state,
        version != CURRENT_STATE_VERSION || !had_schema_version,
    ))
}

#[cfg(test)]
mod tests {
    use super::{load_state_compatibly, AppSettings, DiffSummary, Language, PersistentState};

    #[test]
    fn new_app_defaults_keep_node_names_hidden() {
        assert!(!AppSettings::default().show_node_names);
        assert_eq!(AppSettings::default().right_panel_width, 320.0);
        assert_eq!(AppSettings::default().language, Language::English);
    }

    #[test]
    fn diff_counts_changed_bytes_and_ranges() {
        let old = [0_u8, 1, 2, 3, 4, 5, 6];
        let new = [0_u8, 9, 8, 3, 4, 7, 6, 10];

        let diff = DiffSummary::between(&old, &new);

        assert_eq!(diff.changed_bytes, 4);
        assert_eq!(diff.changed_ranges, 3);
        assert_eq!(diff.first_changed_offset, Some(1));
    }

    #[test]
    fn identical_files_have_empty_diff() {
        let data = [1_u8, 2, 3];
        let diff = DiffSummary::between(&data, &data);

        assert_eq!(diff.changed_bytes, 0);
        assert_eq!(diff.changed_ranges, 0);
        assert_eq!(diff.first_changed_offset, None);
    }

    #[test]
    fn old_settings_without_node_scale_still_load() {
        let state: PersistentState = serde_json::from_str(
            r#"{
                "settings": {
                    "show_node_names": true,
                    "debug_mode": false,
                    "resource_filter": "Alle Ressourcen",
                    "purity_filter": "Alle Reinheiten",
                    "only_claimed": false
                }
            }"#,
        )
        .expect("old settings format should remain readable");

        assert_eq!(state.settings.node_scale, 1.0);
        assert!(!state.settings.show_grid);
        assert!(!state.settings.only_partial);
        assert_eq!(state.settings.language, Language::English);
    }

    #[test]
    fn unversioned_state_is_upgraded_to_current_schema() {
        let (state, migrated) = load_state_compatibly(
            r#"{
                "source_path": "C:\\Satisfactory\\save.sav",
                "node_allocations": {}
            }"#,
        )
        .expect("unversioned state should remain readable");

        assert!(migrated);
        assert_eq!(state.schema_version, super::CURRENT_STATE_VERSION);
    }

    #[test]
    fn newer_unknown_fields_do_not_break_loading() {
        let (state, migrated) = load_state_compatibly(
            r#"{
                "schema_version": 999,
                "future_feature": {"enabled": true},
                "settings": {"node_scale": 1.25}
            }"#,
        )
        .expect("unknown future fields should be ignored");

        assert!(migrated);
        assert_eq!(state.schema_version, super::CURRENT_STATE_VERSION);
        assert_eq!(state.settings.node_scale, 1.25);
        assert_eq!(
            state.future_fields["future_feature"]["enabled"],
            serde_json::Value::Bool(true)
        );
    }
}
