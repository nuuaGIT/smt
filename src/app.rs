use crate::localization::{format_number, text};
use crate::map::{draw_drawing_tool_icon, DrawingToolIcon, MapView};
use crate::save_parser::parse_save_data;
use crate::storage::{AppSettings, DiffSummary, Language, RefreshResult, SaveSnapshot, Storage};
use eframe::egui;
use image::AnimationDecoder;
use rfd::FileDialog;
use std::io::Cursor;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

type ParseResult = Result<crate::save_parser::ParsedSaveData, String>;

const MIN_FIRST_STARTUP_LOADING_TIME: Duration = Duration::from_millis(3_333);
const MIN_RELOAD_LOADING_TIME: Duration = Duration::from_millis(16);
const LOADING_GIFS: [&[u8]; 8] = [
    include_bytes!("../data/loading/hop-on-satisfactory-satisfactory.gif"),
    include_bytes!("../data/loading/satisfactory-bug.gif"),
    include_bytes!("../data/loading/satisfactory-clapping.gif"),
    include_bytes!("../data/loading/satisfactory-game.gif"),
    include_bytes!("../data/loading/satisfactory-gaming.gif"),
    include_bytes!("../data/loading/satisfactory-get-real.gif"),
    include_bytes!("../data/loading/satisfactory.gif"),
    include_bytes!("../data/loading/scaveneil-dumbass.gif"),
];
static LOADING_SELECTION_COUNTER: AtomicU64 = AtomicU64::new(0);

struct LoadingFrame {
    image: egui::ColorImage,
    duration: Duration,
}

struct LoadingState {
    started_at: Instant,
    minimum_duration: Duration,
    frames: Vec<LoadingFrame>,
    textures: Vec<Option<egui::TextureHandle>>,
    assets_prepared: bool,
}

impl LoadingState {
    fn new(minimum_duration: Duration) -> Self {
        let frames = load_random_loading_gif();
        let textures = (0..frames.len()).map(|_| None).collect();
        Self {
            started_at: Instant::now(),
            minimum_duration,
            frames,
            textures,
            assets_prepared: false,
        }
    }

    fn frame_index(&self, elapsed: Duration) -> usize {
        if self.frames.len() <= 1 {
            return 0;
        }
        let total_nanos = self
            .frames
            .iter()
            .map(|frame| frame.duration.as_nanos().max(1))
            .sum::<u128>();
        let mut position = elapsed.as_nanos() % total_nanos.max(1);
        for (index, frame) in self.frames.iter().enumerate() {
            let duration = frame.duration.as_nanos().max(1);
            if position < duration {
                return index;
            }
            position -= duration;
        }
        self.frames.len() - 1
    }
}

fn load_random_loading_gif() -> Vec<LoadingFrame> {
    let time_seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| {
            duration.as_secs().rotate_left(23) ^ u64::from(duration.subsec_nanos()).rotate_left(7)
        })
        .unwrap_or_default();
    let counter_seed = LOADING_SELECTION_COUNTER
        .fetch_add(1, Ordering::Relaxed)
        .wrapping_mul(0x9E37_79B9_7F4A_7C15);
    let process_seed = u64::from(std::process::id()).rotate_left(31);
    let mixed_seed = time_seed ^ counter_seed ^ process_seed;
    let index =
        (mixed_seed ^ (mixed_seed >> 29) ^ (mixed_seed >> 47)) as usize % LOADING_GIFS.len();
    let decoder = image::codecs::gif::GifDecoder::new(Cursor::new(LOADING_GIFS[index]));
    let frames = decoder
        .ok()
        .and_then(|decoder| decoder.into_frames().collect_frames().ok())
        .unwrap_or_default();

    let decoded = frames
        .into_iter()
        .map(|frame| {
            let (numerator, denominator) = frame.delay().numer_denom_ms();
            let delay_millis = if denominator == 0 {
                100
            } else {
                ((numerator as f64 / denominator as f64).round() as u64).clamp(16, 1_000)
            };
            let image = frame.into_buffer();
            let size = [image.width() as usize, image.height() as usize];
            LoadingFrame {
                image: egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw()),
                duration: Duration::from_millis(delay_millis),
            }
        })
        .collect::<Vec<_>>();

    if decoded.is_empty() {
        vec![LoadingFrame {
            image: egui::ColorImage::from_rgba_unmultiplied([1, 1], &[0x19, 0x1F, 0x24, 0xFF]),
            duration: Duration::from_millis(100),
        }]
    } else {
        decoded
    }
}

pub struct TrackerApp {
    storage: Option<Storage>,
    map: MapView,
    status: String,
    error: Option<String>,
    parse_receiver: Option<Receiver<ParseResult>>,
    language: Language,
    debug_mode: bool,
    pause_map_when_unfocused: bool,
    flat_ui_initialized: bool,
    right_panel_open: bool,
    right_panel_width: f32,
    right_panel_generation: u64,
    right_panel_edge_blocked: bool,
    loading: Option<LoadingState>,
    confirm_delete_all_smt_notes: bool,
    auto_refresh_minutes: u32,
    last_auto_refresh: Instant,
    parse_started_at: Option<Instant>,
    analysis_duration_ms: Option<u128>,
    last_updated_at: Option<Instant>,
}

impl TrackerApp {
    pub fn load() -> Self {
        let mut app = match Storage::load() {
            Ok(storage) => Self {
                storage: Some(storage),
                map: MapView::default(),
                status: text(Language::English, "ready").to_owned(),
                error: None,
                parse_receiver: None,
                language: Language::English,
                debug_mode: false,
                pause_map_when_unfocused: true,
                flat_ui_initialized: false,
                right_panel_open: false,
                right_panel_width: 320.0,
                right_panel_generation: 0,
                right_panel_edge_blocked: false,
                loading: None,
                confirm_delete_all_smt_notes: false,
                auto_refresh_minutes: 8,
                last_auto_refresh: Instant::now(),
                parse_started_at: None,
                analysis_duration_ms: None,
                last_updated_at: None,
            },
            Err(error) => Self {
                storage: None,
                map: MapView::default(),
                status: text(Language::English, "initialization_failed").to_owned(),
                error: Some(error.to_string()),
                parse_receiver: None,
                language: Language::English,
                debug_mode: false,
                pause_map_when_unfocused: true,
                flat_ui_initialized: false,
                right_panel_open: false,
                right_panel_width: 320.0,
                right_panel_generation: 0,
                right_panel_edge_blocked: false,
                loading: None,
                confirm_delete_all_smt_notes: false,
                auto_refresh_minutes: 8,
                last_auto_refresh: Instant::now(),
                parse_started_at: None,
                analysis_duration_ms: None,
                last_updated_at: None,
            },
        };

        let startup_save = app.storage.as_ref().and_then(|storage| {
            storage
                .state
                .source_path
                .as_ref()
                .filter(|source_path| source_path.exists())
                .and_then(|_| {
                    storage
                        .active_save_path()
                        .exists()
                        .then(|| storage.active_save_path().to_path_buf())
                })
        });
        if let Some(path) = startup_save {
            app.start_parse(path);
        }
        let (
            allocations,
            settings,
            miner_tier,
            resource_order,
            rectangles,
            circles,
            arrows,
            rulers,
            drawing_history,
            texts,
            strokes,
        ) = app
            .storage
            .as_ref()
            .map(|storage| {
                (
                    storage.active_node_allocations(),
                    storage.state.settings.clone(),
                    storage.active_miner_tier(),
                    storage.active_resource_order(),
                    storage.active_rectangles(),
                    storage.active_circles(),
                    storage.active_arrows(),
                    storage.active_rulers(),
                    storage.active_drawing_history(),
                    storage.active_texts(),
                    storage.active_strokes(),
                )
            })
            .unwrap_or_default();
        app.map.apply_allocations(&allocations);
        app.map.set_rectangles(rectangles);
        app.map.set_circles(circles);
        app.map.set_arrows(arrows);
        app.map.set_rulers(rulers);
        app.map.set_drawing_history(drawing_history);
        app.map.set_texts(texts);
        app.map.set_strokes(strokes);
        app.language = settings.language;
        app.map.set_language(app.language);
        app.map.set_unclaimed_miner_tier(miner_tier);
        app.map.set_resource_order(resource_order);
        app.map.set_show_node_names(settings.show_node_names);
        app.map.set_filter_settings(
            settings.resource_filter,
            settings.purity_filter,
            settings.only_claimed,
            settings.only_partial,
            settings.only_planned,
        );
        app.map.set_selected_resources(settings.selected_resources);
        app.map.set_node_scale(settings.node_scale);
        app.map.set_show_grid(settings.show_grid);
        app.map.set_show_annotations(settings.show_annotations);
        app.map.set_show_rails(settings.show_rails);
        app.map.set_show_foundations(settings.show_foundations);
        app.map.set_show_belts(settings.show_belts);
        app.map.set_use_svg_map(settings.use_svg_map);
        app.debug_mode = settings.debug_mode;
        app.pause_map_when_unfocused = settings.pause_map_when_unfocused;
        app.right_panel_width = settings.right_panel_width.clamp(8.0, 720.0);
        app.auto_refresh_minutes = settings.auto_refresh_minutes.clamp(1, 120);
        app.loading = Some(LoadingState::new(MIN_FIRST_STARTUP_LOADING_TIME));
        app
    }

    fn show_startup_loading(&mut self, context: &egui::Context) -> bool {
        let Some(loading) = self.loading.as_ref() else {
            return false;
        };
        let elapsed = loading.started_at.elapsed();
        if elapsed >= loading.minimum_duration && self.parse_receiver.is_none() {
            self.loading = None;
            return false;
        }

        if self
            .loading
            .as_ref()
            .is_some_and(|loading| !loading.assets_prepared)
        {
            self.map.prepare_assets(context);
            if let Some(loading) = self.loading.as_mut() {
                loading.assets_prepared = true;
            }
        }

        let (texture, image_size) = {
            let loading = self.loading.as_mut().expect("loading state exists");
            let frame_index = loading.frame_index(elapsed);
            if loading.textures[frame_index].is_none() {
                let texture = context.load_texture(
                    format!("startup-loading-frame-{frame_index}"),
                    loading.frames[frame_index].image.clone(),
                    egui::TextureOptions::LINEAR,
                );
                loading.textures[frame_index] = Some(texture);
            }
            (
                loading.textures[frame_index]
                    .as_ref()
                    .expect("loading texture was created")
                    .clone(),
                loading.frames[frame_index].image.size,
            )
        };

        let screen = context.screen_rect();
        let painter = context.layer_painter(egui::LayerId::new(
            egui::Order::Background,
            egui::Id::new("startup-loading-screen"),
        ));
        painter.rect_filled(screen, 0.0, egui::Color32::from_rgb(0x19, 0x1F, 0x24));

        let image_size = egui::vec2(image_size[0] as f32, image_size[1] as f32);
        let max_size = egui::vec2(screen.width() * 0.62, screen.height() * 0.68);
        let scale = (max_size.x / image_size.x)
            .min(max_size.y / image_size.y)
            .min(1.0);
        let image_rect = egui::Rect::from_center_size(
            screen.center() - egui::vec2(0.0, 12.0),
            image_size * scale,
        );
        painter.image(
            texture.id(),
            image_rect,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            egui::Color32::WHITE,
        );
        painter.text(
            screen.center_bottom() - egui::vec2(0.0, 28.0),
            egui::Align2::CENTER_BOTTOM,
            text(self.language, "loading"),
            egui::FontId::proportional(16.0),
            egui::Color32::WHITE,
        );
        context.request_repaint_after(Duration::from_millis(16));
        true
    }

    fn start_loading_screen(&mut self) {
        self.loading = Some(LoadingState::new(MIN_RELOAD_LOADING_TIME));
    }

    fn persist_settings(&mut self) {
        let (resource_filter, purity_filter, only_claimed, only_partial, only_planned) =
            self.map.filter_settings();
        let settings = AppSettings {
            show_node_names: self.map.show_node_names(),
            language: self.language,
            debug_mode: self.debug_mode,
            pause_map_when_unfocused: self.pause_map_when_unfocused,
            resource_filter,
            selected_resources: self.map.selected_resources(),
            purity_filter,
            only_claimed,
            node_scale: self.map.node_scale(),
            show_grid: self.map.show_grid(),
            show_annotations: self.map.show_annotations(),
            only_partial,
            only_planned,
            show_rails: self.map.show_rails(),
            show_foundations: self.map.show_foundations(),
            show_belts: self.map.show_belts(),
            use_svg_map: self.map.use_svg_map(),
            right_panel_width: self.right_panel_width.max(40.0),
            auto_refresh_minutes: self.auto_refresh_minutes.clamp(1, 120),
        };
        if let Some(storage) = self.storage.as_mut() {
            if let Err(error) = storage.save_settings(settings) {
                self.error = Some(error.to_string());
            }
        }
    }

    fn clear_active_save(&mut self) -> Result<(), String> {
        let Some(storage) = self.storage.as_mut() else {
            return Err("Die Anwendung ist nicht initialisiert".to_owned());
        };
        storage
            .clear_active_save()
            .map_err(|error| error.to_string())?;
        self.map
            .replace_allocations(&std::collections::BTreeMap::new());
        self.map.set_resource_order(Vec::new());
        self.map.set_rectangles(Vec::new());
        self.map.set_circles(Vec::new());
        self.map.set_arrows(Vec::new());
        self.map.set_rulers(Vec::new());
        self.map.set_drawing_history(Vec::new());
        self.map.set_strokes(Vec::new());
        self.map.set_unclaimed_miner_tier(3);
        self.map.set_play_duration_in_seconds(0);
        self.right_panel_open = false;
        self.right_panel_edge_blocked = true;
        self.right_panel_generation = self.right_panel_generation.wrapping_add(1);
        self.status = text(self.language, "no_save").to_owned();
        self.error = None;
        self.parse_started_at = None;
        self.analysis_duration_ms = None;
        self.last_updated_at = None;
        self.last_auto_refresh = Instant::now();
        Ok(())
    }

    fn delete_all_smt_notes(&mut self) -> Result<(), String> {
        let Some(storage) = self.storage.as_mut() else {
            return Err("Die Anwendung ist nicht initialisiert".to_owned());
        };
        storage
            .delete_all_smt_notes()
            .map_err(|error| error.to_string())?;

        self.parse_receiver = None;
        self.map
            .replace_allocations(&std::collections::BTreeMap::new());
        self.map.set_resource_order(Vec::new());
        self.map.set_rectangles(Vec::new());
        self.map.set_circles(Vec::new());
        self.map.set_arrows(Vec::new());
        self.map.set_rulers(Vec::new());
        self.map.set_texts(Vec::new());
        self.map.set_strokes(Vec::new());
        self.map.set_drawing_history(Vec::new());
        self.map
            .replace_map_layers(Vec::new(), Vec::new(), Vec::new());
        self.map.set_unclaimed_miner_tier(3);
        self.map.set_play_duration_in_seconds(0);
        self.right_panel_open = false;
        self.right_panel_edge_blocked = true;
        self.right_panel_generation = self.right_panel_generation.wrapping_add(1);
        self.status = text(self.language, "no_save").to_owned();
        self.error = None;
        self.parse_started_at = None;
        self.analysis_duration_ms = None;
        self.last_updated_at = None;
        self.last_auto_refresh = Instant::now();
        Ok(())
    }

    fn choose_save(&mut self) {
        let Some(path) = FileDialog::new()
            .add_filter("Satisfactory Savegame", &["sav"])
            .set_title(match self.language {
                Language::English => "Select Satisfactory savegame",
                Language::German => "Satisfactory-Savegame auswählen",
                Language::French => "Sélectionner une sauvegarde Satisfactory",
                Language::Spanish => "Seleccionar partida de Satisfactory",
            })
            .pick_file()
        else {
            return;
        };

        let Some(storage) = self.storage.as_mut() else {
            return;
        };

        let result = storage.install_new_save(&path);
        match result {
            Ok(snapshot) => {
                self.error = None;
                self.status = format!(
                    "{}: {}",
                    text(self.language, "save_stored"),
                    snapshot.file_name
                );
                let (
                    allocations,
                    miner_tier,
                    resource_order,
                    rectangles,
                    circles,
                    arrows,
                    rulers,
                    drawing_history,
                    texts,
                    strokes,
                ) = self
                    .storage
                    .as_ref()
                    .map(|storage| {
                        (
                            storage.active_node_allocations(),
                            storage.active_miner_tier(),
                            storage.active_resource_order(),
                            storage.active_rectangles(),
                            storage.active_circles(),
                            storage.active_arrows(),
                            storage.active_rulers(),
                            storage.active_drawing_history(),
                            storage.active_texts(),
                            storage.active_strokes(),
                        )
                    })
                    .unwrap_or_default();
                self.map.replace_allocations(&allocations);
                self.map.set_unclaimed_miner_tier(miner_tier);
                self.map.set_resource_order(resource_order);
                self.map.set_rectangles(rectangles);
                self.map.set_circles(circles);
                self.map.set_arrows(arrows);
                self.map.set_rulers(rulers);
                self.map.set_drawing_history(drawing_history);
                self.map.set_texts(texts);
                self.map.set_strokes(strokes);
                self.map.set_play_duration_in_seconds(0);
                self.last_auto_refresh = Instant::now();
                self.start_loading_screen();
                self.start_parse(path);
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn refresh_save(&mut self) {
        self.last_auto_refresh = Instant::now();
        self.start_loading_screen();
        let result = match self.storage.as_mut() {
            Some(storage) => storage.refresh(),
            None => return,
        };

        match result {
            Ok(RefreshResult::Unchanged) => {
                self.error = None;
                self.status = text(self.language, "no_changes").to_owned();
                self.last_updated_at = Some(Instant::now());
            }
            Ok(RefreshResult::Updated(diff)) => {
                self.error = None;
                self.status = format_diff_status(self.language, &diff);
                let save_path = self
                    .storage
                    .as_ref()
                    .and_then(|storage| storage.state.source_path.clone());
                if let Some(path) = save_path {
                    self.start_parse(path);
                }
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn auto_refresh_if_due(&mut self, context: &egui::Context) {
        let has_source_save = self
            .storage
            .as_ref()
            .and_then(|storage| storage.state.source_path.as_ref())
            .is_some_and(|path| path.exists());
        if !has_source_save {
            return;
        }

        let interval = Duration::from_secs(u64::from(self.auto_refresh_minutes.max(1)) * 60);
        let elapsed = self.last_auto_refresh.elapsed();
        if elapsed >= interval && self.parse_receiver.is_none() && self.loading.is_none() {
            self.refresh_save();
            return;
        }

        let until_refresh = interval.saturating_sub(elapsed);
        context.request_repaint_after(until_refresh.min(Duration::from_secs(1)));
    }

    fn header_status(&self) -> String {
        if self.parse_receiver.is_some() {
            return self.status.clone();
        }
        if let (Some(duration_ms), Some(last_updated)) =
            (self.analysis_duration_ms, self.last_updated_at)
        {
            return format!(
                "{}: {} ms · {}: {}",
                text(self.language, "analysis_duration"),
                format_number(self.language, duration_ms as f64, 0),
                text(self.language, "last_updated"),
                format_time_ago(self.language, last_updated)
            );
        }
        self.status.clone()
    }

    fn start_parse(&mut self, path: PathBuf) {
        let (sender, receiver) = mpsc::channel();
        self.parse_receiver = Some(receiver);
        self.parse_started_at = Some(Instant::now());
        self.status = text(self.language, "analyzing_save").to_owned();

        thread::spawn(move || {
            let result = parse_save_data(&path).map_err(|error| error.to_string());
            let _ = sender.send(result);
        });
    }

    fn poll_parse(&mut self) {
        let Some(receiver) = self.parse_receiver.take() else {
            return;
        };

        match receiver.try_recv() {
            Ok(Ok(save_data)) => {
                self.analysis_duration_ms = self
                    .parse_started_at
                    .take()
                    .map(|started| started.elapsed().as_millis());
                self.last_updated_at = Some(Instant::now());
                let (matched, total) = self.map.apply_extractors(&save_data.extractors);
                self.map.replace_map_layers(
                    save_data.rails,
                    save_data.foundations,
                    save_data.belts,
                );
                self.map
                    .set_play_duration_in_seconds(save_data.play_duration_in_seconds);
                self.status = format!(
                    "{}: {}/{} extractors · {} {} · {} {} · {} {}",
                    text(self.language, "save_analyzed"),
                    format_number(self.language, matched as f64, 0),
                    format_number(self.language, total as f64, 0),
                    format_meters(self.language, self.map.rail_length_meters()),
                    text(self.language, "rails"),
                    format_number(self.language, self.map.foundation_count() as f64, 0),
                    text(self.language, "foundation_count"),
                    format_meters(self.language, self.map.belt_length_meters()),
                    text(self.language, "belt_count"),
                );
                self.error = None;
            }
            Ok(Err(error)) => {
                self.status = text(self.language, "analysis_failed").to_owned();
                self.error = Some(error);
            }
            Err(TryRecvError::Empty) => {
                self.parse_receiver = Some(receiver);
            }
            Err(TryRecvError::Disconnected) => {
                self.status = text(self.language, "analysis_stopped").to_owned();
            }
        }
    }
}

impl eframe::App for TrackerApp {
    fn update(&mut self, context: &egui::Context, frame: &mut eframe::Frame) {
        if !self.flat_ui_initialized {
            apply_flat_ui_style(context);
            self.flat_ui_initialized = true;
        }

        let viewport = context.input(|input| input.viewport().clone());
        let app_in_background =
            viewport.minimized.unwrap_or(false) || !viewport.focused.unwrap_or(true);

        if app_in_background {
            self.map.close_popup_for_background();
        }

        self.poll_parse();
        self.auto_refresh_if_due(context);
        if self.parse_receiver.is_some() {
            let repaint_delay = if viewport.minimized.unwrap_or(false) {
                std::time::Duration::from_secs(2)
            } else if !viewport.focused.unwrap_or(true) {
                std::time::Duration::from_millis(500)
            } else {
                std::time::Duration::from_millis(100)
            };
            context.request_repaint_after(repaint_delay);
        }

        if self.show_startup_loading(context) {
            return;
        }

        egui::TopBottomPanel::top("toolbar")
            .frame(
                egui::Frame::side_top_panel(&context.style())
                    .inner_margin(egui::Margin::symmetric(8.0, 1.0)),
            )
            .show(context, |ui| {
                ui.horizontal(|ui| {
                    if ui.button(text(self.language, "upload_save")).clicked() {
                        self.choose_save();
                    }

                    let can_refresh = self
                        .storage
                        .as_ref()
                        .and_then(|storage| storage.state.source_path.as_ref())
                        .is_some();
                    if ui
                        .add_enabled(
                            can_refresh,
                            egui::Button::new(text(self.language, "refresh")),
                        )
                        .clicked()
                    {
                        self.refresh_save();
                    }

                    let mut settings_changed = false;
                    ui.menu_button(text(self.language, "settings"), |ui| {
                        egui::CollapsingHeader::new(text(self.language, "settings_application"))
                            .default_open(true)
                            .show(ui, |ui| {
                                let mut language = self.language;
                                egui::ComboBox::from_label(text(self.language, "language"))
                                    .selected_text(language.native_name())
                                    .show_ui(ui, |ui| {
                                        for candidate in [
                                            Language::English,
                                            Language::German,
                                            Language::French,
                                            Language::Spanish,
                                        ] {
                                            ui.selectable_value(
                                                &mut language,
                                                candidate,
                                                candidate.native_name(),
                                            );
                                        }
                                    });
                                if language != self.language {
                                    self.language = language;
                                    self.map.set_language(language);
                                    settings_changed = true;
                                }
                            });

                        egui::CollapsingHeader::new(text(self.language, "settings_map"))
                            .default_open(true)
                            .show(ui, |ui| {
                                self.map.filters(ui);
                                let mut show_names = self.map.show_node_names();
                                if ui
                                    .checkbox(&mut show_names, text(self.language, "node_names"))
                                    .changed()
                                {
                                    self.map.set_show_node_names(show_names);
                                    settings_changed = true;
                                }
                                let mut node_scale = self.map.node_scale();
                                let node_scale_label = format!(
                                    "{} {}%",
                                    text(self.language, "node_size"),
                                    format_number(self.language, (node_scale * 100.0) as f64, 0)
                                );
                                if ui
                                    .add(
                                        egui::Slider::new(&mut node_scale, 0.5..=1.5)
                                            .text(node_scale_label),
                                    )
                                    .changed()
                                {
                                    self.map.set_node_scale(node_scale);
                                    settings_changed = true;
                                }
                            });

                        egui::CollapsingHeader::new(text(self.language, "settings_display"))
                            .default_open(true)
                            .show(ui, |ui| {
                                let mut show_annotations = self.map.show_annotations();
                                if ui
                                    .checkbox(
                                        &mut show_annotations,
                                        text(self.language, "show_annotations"),
                                    )
                                    .changed()
                                {
                                    self.map.set_show_annotations(show_annotations);
                                    settings_changed = true;
                                }
                                if ui
                                    .checkbox(
                                        &mut self.debug_mode,
                                        text(self.language, "debug_mode"),
                                    )
                                    .changed()
                                {
                                    settings_changed = true;
                                }
                                if ui
                                    .checkbox(
                                        &mut self.pause_map_when_unfocused,
                                        text(self.language, "pause_background"),
                                    )
                                    .changed()
                                {
                                    settings_changed = true;
                                }
                                let mut auto_refresh_minutes = self.auto_refresh_minutes;
                                if ui
                                    .add(
                                        egui::Slider::new(&mut auto_refresh_minutes, 1..=120)
                                            .text(text(self.language, "auto_refresh")),
                                    )
                                    .changed()
                                {
                                    self.auto_refresh_minutes = auto_refresh_minutes;
                                    self.last_auto_refresh = Instant::now();
                                    settings_changed = true;
                                }
                                ui.small(text(self.language, "background_throttle"));

                                let mut use_detailed_png_map = !self.map.use_svg_map();
                                if ui
                                    .checkbox(
                                        &mut use_detailed_png_map,
                                        text(self.language, "detailed_png_map"),
                                    )
                                    .changed()
                                {
                                    self.map.set_use_svg_map(!use_detailed_png_map);
                                    self.start_loading_screen();
                                    settings_changed = true;
                                }
                            });

                        egui::CollapsingHeader::new(text(self.language, "settings_testing"))
                            .default_open(false)
                            .show(ui, |ui| {
                                let mut show_rails = self.map.show_rails();
                                if ui
                                    .checkbox(&mut show_rails, text(self.language, "rails_wip"))
                                    .changed()
                                {
                                    self.map.set_show_rails(show_rails);
                                    settings_changed = true;
                                }
                                let mut show_foundations = self.map.show_foundations();
                                if ui
                                    .checkbox(
                                        &mut show_foundations,
                                        text(self.language, "show_foundations"),
                                    )
                                    .changed()
                                {
                                    self.map.set_show_foundations(show_foundations);
                                    settings_changed = true;
                                }
                                let mut show_belts = self.map.show_belts();
                                if ui
                                    .checkbox(&mut show_belts, text(self.language, "show_belts"))
                                    .changed()
                                {
                                    self.map.set_show_belts(show_belts);
                                    settings_changed = true;
                                }
                            });

                        egui::CollapsingHeader::new(text(self.language, "more"))
                            .default_open(false)
                            .show(ui, |ui| {
                                if ui
                                    .button(text(self.language, "delete_all_smt_notes"))
                                    .clicked()
                                {
                                    self.confirm_delete_all_smt_notes = true;
                                    ui.close_menu();
                                }
                                if ui.button("CLICK ME").clicked() {
                                    ui.ctx().open_url(egui::OpenUrl::new_tab(
                                        "https://youtu.be/dQw4w9WgXcQ",
                                    ));
                                }
                            });
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let can_clear_save = self
                            .storage
                            .as_ref()
                            .is_some_and(|storage| storage.state.source_path.is_some());
                        if ui
                            .add_enabled(
                                can_clear_save,
                                egui::Button::new(text(self.language, "remove_save")),
                            )
                            .clicked()
                        {
                            if let Err(error) = self.clear_active_save() {
                                self.error = Some(error);
                            } else {
                                context.request_repaint();
                            }
                        }
                        ui.separator();
                        ui.small(self.header_status());
                    });
                    if self.map.take_filter_settings_changed() {
                        settings_changed = true;
                    }
                    if settings_changed {
                        self.persist_settings();
                    }
                });
            });

        if self.confirm_delete_all_smt_notes {
            let mut delete = false;
            let mut cancel = false;
            egui::Window::new(text(self.language, "delete_all_smt_notes_title"))
                .collapsible(false)
                .resizable(false)
                .show(context, |ui| {
                    ui.label(text(self.language, "delete_all_smt_notes_warning"));
                    ui.horizontal(|ui| {
                        if ui
                            .button(text(self.language, "delete_all_smt_notes_confirm"))
                            .clicked()
                        {
                            delete = true;
                        }
                        if ui.button(text(self.language, "cancel")).clicked() {
                            cancel = true;
                        }
                    });
                });
            if delete {
                self.confirm_delete_all_smt_notes = false;
                if let Err(error) = self.delete_all_smt_notes() {
                    self.error = Some(error);
                } else {
                    context.request_repaint();
                }
            } else if cancel {
                self.confirm_delete_all_smt_notes = false;
            }
        }

        let save_path = self
            .storage
            .as_ref()
            .and_then(|storage| storage.state.source_path.as_ref())
            .cloned();
        egui::TopBottomPanel::top("save-info-bar")
            .frame(
                egui::Frame::side_top_panel(&context.style())
                    .inner_margin(egui::Margin::symmetric(8.0, 1.0)),
            )
            .show(context, |ui| {
                ui.horizontal_wrapped(|ui| {
                    let save_name = save_path
                        .as_ref()
                        .and_then(|path| path.file_name())
                        .and_then(|name| name.to_str())
                        .unwrap_or("—");
                    ui.label(
                        egui::RichText::new(format!(
                            "{}: {save_name}",
                            text(self.language, "save_file")
                        ))
                        .color(egui::Color32::WHITE)
                        .strong(),
                    );
                    ui.separator();
                    ui.label(format!(
                        "{} {} · {} {}",
                        format_number(self.language, self.map.nodes.len() as f64, 0),
                        text(self.language, "resource_nodes"),
                        format_number(self.language, self.map.claimed_node_count() as f64, 0),
                        text(self.language, "claimed_nodes")
                    ));
                    ui.separator();
                    ui.label(format!(
                        "{}: {} · {}: {} · {}: {}",
                        text(self.language, "belts_laid"),
                        format_meters(self.language, self.map.belt_length_meters()),
                        text(self.language, "rails_laid"),
                        format_meters(self.language, self.map.rail_length_meters()),
                        text(self.language, "playtime"),
                        format_playtime(self.language, self.map.play_duration_in_seconds())
                    ));
                });
            });

        let screen_rect = context.screen_rect();
        let pointer_at_right_edge = context.input(|input| {
            input
                .pointer
                .hover_pos()
                .is_some_and(|position| position.x >= screen_rect.right() - 5.0)
        });
        if !pointer_at_right_edge {
            self.right_panel_edge_blocked = false;
        }
        if !app_in_background
            && !self.right_panel_open
            && pointer_at_right_edge
            && !self.right_panel_edge_blocked
            && self
                .storage
                .as_ref()
                .is_some_and(|storage| storage.state.source_path.is_some())
        {
            self.right_panel_open = true;
            context.request_repaint();
        }

        let mut panel_width_changed = false;
        if self.right_panel_open && !app_in_background {
            let panel_id = egui::Id::new(("right-map-panel", self.right_panel_generation));
            let panel = egui::SidePanel::right(panel_id)
                .default_width(self.right_panel_width.clamp(220.0, 720.0))
                .width_range(8.0..=720.0)
                .resizable(true)
                .show(context, |ui| {
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.heading(text(self.language, "map_panel"));
                            ui.small(text(self.language, "panel_open_hint"));
                            ui.add_space(8.0);

                            if ui.button(text(self.language, "close_panel")).clicked() {
                                self.right_panel_open = false;
                                self.right_panel_generation =
                                    self.right_panel_generation.wrapping_add(1);
                                context.request_repaint();
                            }
                            ui.separator();
                            ui.add_space(8.0);
                            let resource_order_changed = self.map.resource_summary(ui);
                            if resource_order_changed {
                                let order = self.map.resource_order();
                                if let Some(storage) = self.storage.as_mut() {
                                    if let Err(error) = storage.save_active_resource_order(order) {
                                        self.error = Some(error.to_string());
                                    }
                                }
                            }
                            ui.separator();
                            ui.label(text(self.language, "max_unclaimed"));
                            let mut miner_tier = self.map.unclaimed_miner_tier();
                            egui::ComboBox::from_id_salt("right-panel-unclaimed-miner")
                                .selected_text(format!("Miner Mk.{} · 250%", miner_tier))
                                .show_ui(ui, |ui| {
                                    for tier in 1..=3 {
                                        ui.selectable_value(
                                            &mut miner_tier,
                                            tier,
                                            format!("Miner Mk.{} · 250%", tier),
                                        );
                                    }
                                });
                            if miner_tier != self.map.unclaimed_miner_tier() {
                                self.map.set_unclaimed_miner_tier(miner_tier);
                                if let Some(storage) = self.storage.as_mut() {
                                    if let Err(error) = storage.save_active_miner_tier(miner_tier) {
                                        self.error = Some(error.to_string());
                                    }
                                }
                                context.request_repaint();
                            }
                            ui.small(text(self.language, "unclaimed_hint"));
                            ui.separator();
                            ui.small(text(self.language, "panel_resize_hint"));
                        });
                });

            let width = panel.response.rect.width();
            if width > 40.0 && (width - self.right_panel_width).abs() > 0.5 {
                self.right_panel_width = width.clamp(40.0, 720.0);
                panel_width_changed = true;
            }
            if width <= 40.0 {
                self.right_panel_open = false;
                self.right_panel_edge_blocked = true;
                self.right_panel_generation = self.right_panel_generation.wrapping_add(1);
                context.request_repaint();
            }
        }
        if panel_width_changed && !context.input(|input| input.pointer.primary_down()) {
            self.persist_settings();
        }

        egui::CentralPanel::default()
            .frame(
                egui::Frame::central_panel(&context.style())
                    .inner_margin(egui::Margin::symmetric(8.0, 1.0)),
            )
            .show(context, |ui| {
                let has_source_save = self
                    .storage
                    .as_ref()
                    .is_some_and(|storage| storage.state.source_path.is_some());
                ui.horizontal(|ui| {
                    ui.heading(text(self.language, "app_title"));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.vertical(|ui| {
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.label(
                                        egui::RichText::new(text(self.language, "drawing_toolbar"))
                                            .strong()
                                            .color(egui::Color32::WHITE),
                                    );
                                },
                            );
                            ui.add_space(2.0);
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                                if toolbar_tool(
                                    ui,
                                    has_source_save,
                                    self.map.rectangle_tool_active(),
                                    DrawingToolIcon::Rectangle,
                                    text(self.language, "tool_rectangle"),
                                ) {
                                    self.map.toggle_rectangle_tool();
                                    context.request_repaint();
                                }
                                if toolbar_tool(
                                    ui,
                                    has_source_save,
                                    self.map.circle_tool_active(),
                                    DrawingToolIcon::Circle,
                                    text(self.language, "tool_circle"),
                                ) {
                                    self.map.toggle_circle_tool();
                                    context.request_repaint();
                                }
                                if toolbar_tool(
                                    ui,
                                    has_source_save,
                                    self.map.arrow_tool_active(),
                                    DrawingToolIcon::Arrow,
                                    text(self.language, "tool_arrow"),
                                ) {
                                    self.map.toggle_arrow_tool();
                                    context.request_repaint();
                                }
                                if toolbar_tool(
                                    ui,
                                    has_source_save,
                                    self.map.ruler_tool_active(),
                                    DrawingToolIcon::Ruler,
                                    text(self.language, "tool_ruler"),
                                ) {
                                    self.map.toggle_ruler_tool();
                                    context.request_repaint();
                                }
                                if toolbar_tool(
                                    ui,
                                    has_source_save,
                                    self.map.text_tool_active(),
                                    DrawingToolIcon::Text,
                                    text(self.language, "tool_text"),
                                ) {
                                    self.map.toggle_text_tool();
                                    context.request_repaint();
                                }
                                if toolbar_tool(
                                    ui,
                                    has_source_save,
                                    self.map.eraser_tool_active(),
                                    DrawingToolIcon::Eraser,
                                    text(self.language, "tool_eraser"),
                                ) {
                                    self.map.toggle_eraser_tool();
                                    context.request_repaint();
                                }
                                if toolbar_tool(
                                    ui,
                                    has_source_save,
                                    self.map.pen_tool_active(),
                                    DrawingToolIcon::Pen,
                                    text(self.language, "tool_pen"),
                                ) {
                                    self.map.toggle_pen_tool();
                                    context.request_repaint();
                                }
                            });
                        });
                    });
                });
                ui.add_space(0.0);

                if let Some(error) = &self.error {
                    egui::Frame::group(ui.style()).show(ui, |ui| {
                        ui.colored_label(
                            egui::Color32::from_rgb(220, 80, 80),
                            text(self.language, "error"),
                        );
                        ui.label(error);
                    });
                    ui.add_space(12.0);
                }

                if self.storage.is_none() {
                    ui.label(text(self.language, "not_initialized"));
                    return;
                }

                if !has_source_save {
                    ui.centered_and_justified(|ui| {
                        ui.vertical_centered(|ui| {
                            ui.weak(text(self.language, "no_save"));
                            ui.small(text(self.language, "upload_hint"));
                        });
                    });
                } else if app_in_background && self.pause_map_when_unfocused {
                    ui.centered_and_justified(|ui| {
                        ui.vertical_centered(|ui| {
                            ui.weak(text(self.language, "map_paused"));
                            ui.small(text(self.language, "change_in_settings"));
                        });
                    });
                } else {
                    self.map.canvas(ui);
                }

                if let Some(allocations) = self.map.take_allocation_save() {
                    if let Some(storage) = self.storage.as_mut() {
                        match storage.save_node_allocations(allocations) {
                            Ok(()) => {
                                self.status = text(self.language, "node_data_saved").to_owned();
                                self.error = None;
                            }
                            Err(error) => self.error = Some(error.to_string()),
                        }
                    }
                }

                if let Some(rectangles) = self.map.take_rectangle_save() {
                    if let Some(storage) = self.storage.as_mut() {
                        if let Err(error) = storage.save_active_rectangles(rectangles) {
                            self.error = Some(error.to_string());
                        }
                    }
                }
                if let Some(circles) = self.map.take_circle_save() {
                    if let Some(storage) = self.storage.as_mut() {
                        if let Err(error) = storage.save_active_circles(circles) {
                            self.error = Some(error.to_string());
                        }
                    }
                }
                if let Some(arrows) = self.map.take_arrow_save() {
                    if let Some(storage) = self.storage.as_mut() {
                        if let Err(error) = storage.save_active_arrows(arrows) {
                            self.error = Some(error.to_string());
                        }
                    }
                }
                if let Some(rulers) = self.map.take_ruler_save() {
                    if let Some(storage) = self.storage.as_mut() {
                        if let Err(error) = storage.save_active_rulers(rulers) {
                            self.error = Some(error.to_string());
                        }
                    }
                }
                if let Some(history) = self.map.take_drawing_history_save() {
                    if let Some(storage) = self.storage.as_mut() {
                        if let Err(error) = storage.save_active_drawing_history(history) {
                            self.error = Some(error.to_string());
                        }
                    }
                }
                if let Some(texts) = self.map.take_text_save() {
                    if let Some(storage) = self.storage.as_mut() {
                        if let Err(error) = storage.save_active_texts(texts) {
                            self.error = Some(error.to_string());
                        }
                    }
                }
                if let Some(strokes) = self.map.take_stroke_save() {
                    if let Some(storage) = self.storage.as_mut() {
                        if let Err(error) = storage.save_active_strokes(strokes) {
                            self.error = Some(error.to_string());
                        }
                    }
                }

                ui.add_space(12.0);
                egui::CollapsingHeader::new(text(self.language, "save_status"))
                    .default_open(false)
                    .show(ui, |ui| {
                        let Some(storage) = self.storage.as_ref() else {
                            return;
                        };
                        if let Some(snapshot) = &storage.state.current_snapshot {
                            snapshot_view(ui, snapshot, self.language);
                        } else {
                            ui.label(text(self.language, "no_saved_save"));
                        }
                        if let Some(diff) = &storage.state.last_diff {
                            ui.separator();
                            diff_view(ui, diff, self.language);
                        }
                        ui.label(format!(
                            "{}: {}",
                            text(self.language, "local_copy"),
                            storage.active_save_path().display()
                        ));
                    });
            });

        if self.debug_mode {
            let stable_dt = context.input(|input| input.stable_dt.max(f32::EPSILON));
            let cpu_usage = frame.info().cpu_usage.map(|value| value * 100.0);
            let viewport_size = context.input(|input| input.viewport().inner_rect);
            let save_path = self
                .storage
                .as_ref()
                .and_then(|storage| storage.state.source_path.as_ref())
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "kein Savegame".to_owned());

            egui::Area::new(egui::Id::new("debug-overlay"))
                .order(egui::Order::Foreground)
                .fixed_pos(egui::pos2(8.0, 42.0))
                .show(context, |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.set_min_width(270.0);
                        ui.monospace(format!(
                            "FPS: {} · Frame: {} ms",
                            format_number(self.language, (1.0 / stable_dt) as f64, 1),
                            format_number(self.language, (stable_dt * 1000.0) as f64, 2)
                        ));
                        ui.monospace(format!(
                            "CPU letzter Frame: {}",
                            cpu_usage
                                .map(|value| {
                                    format!("{}%", format_number(self.language, value as f64, 1))
                                })
                                .unwrap_or_else(|| "n/a".to_owned())
                        ));
                        ui.monospace(format!(
                            "Fenster: fokussiert={} minimiert={}",
                            viewport.focused.unwrap_or(false),
                            viewport.minimized.unwrap_or(false)
                        ));
                        ui.monospace(format!(
                            "Nodes: {} total · {} claimed · {} sichtbar",
                            format_number(self.language, self.map.nodes.len() as f64, 0),
                            format_number(self.language, self.map.claimed_node_count() as f64, 0),
                            format_number(self.language, self.map.visible_node_count() as f64, 0)
                        ));
                        ui.monospace(format!(
                            "Schienen: {} · {} Segmente",
                            format_meters(self.language, self.map.rail_length_meters()),
                            format_number(self.language, self.map.rail_count() as f64, 0)
                        ));
                        ui.monospace(format!(
                            "Layer: {} Foundations · {} Belts · {}",
                            format_number(self.language, self.map.foundation_count() as f64, 0),
                            format_number(self.language, self.map.belt_count() as f64, 0),
                            format_meters(self.language, self.map.belt_length_meters())
                        ));
                        ui.monospace(format!(
                            "Zoom: {}x · Node-Größe: {}%",
                            format_number(self.language, self.map.zoom_level() as f64, 2),
                            format_number(self.language, (self.map.node_scale() * 100.0) as f64, 0)
                        ));
                        ui.monospace(format!(
                            "Parser: {}",
                            if self.parse_receiver.is_some() {
                                "aktiv"
                            } else {
                                "idle"
                            }
                        ));
                        if let Some(rect) = viewport_size {
                            ui.monospace(format!(
                                "Viewport: {} x {}",
                                format_number(self.language, rect.width() as f64, 0),
                                format_number(self.language, rect.height() as f64, 0)
                            ));
                        }
                        ui.small(format!("Quelle: {save_path}"));
                    });
                });
        }
    }
}

fn apply_flat_ui_style(context: &egui::Context) {
    let mut style = (*context.style()).clone();
    let accent = egui::Color32::from_rgb(0x4B, 0xB4, 0xCE);
    let normal_text = egui::Color32::from_rgb(0xD2, 0xD6, 0xD9);

    // Keep the flat, old-school desktop look for containers while using a
    // restrained cyan accent for interactive states and selections.
    style.visuals.window_rounding = egui::Rounding::ZERO;
    style.visuals.menu_rounding = egui::Rounding::ZERO;
    style.visuals.window_shadow = egui::Shadow::NONE;
    style.visuals.popup_shadow = egui::Shadow::NONE;
    style.visuals.hyperlink_color = accent;
    style.visuals.selection.bg_fill = accent.gamma_multiply(0.28);
    style.visuals.selection.stroke = egui::Stroke::new(1.0_f32, accent);
    style.visuals.widgets.hovered.weak_bg_fill = accent.gamma_multiply(0.14);
    style.visuals.widgets.hovered.bg_stroke.color = accent;
    style.visuals.widgets.active.weak_bg_fill = accent.gamma_multiply(0.24);
    style.visuals.widgets.active.bg_stroke.color = accent;
    style.visuals.widgets.open.weak_bg_fill = accent.gamma_multiply(0.18);
    style.visuals.widgets.open.bg_stroke.color = accent;
    style.visuals.widgets.noninteractive.fg_stroke.color = normal_text;
    style.visuals.widgets.inactive.fg_stroke.color = normal_text;
    style.visuals.widgets.hovered.fg_stroke.color = egui::Color32::WHITE;
    style.visuals.widgets.active.fg_stroke.color = egui::Color32::WHITE;
    style.visuals.widgets.open.fg_stroke.color = egui::Color32::WHITE;
    for widget in [
        &mut style.visuals.widgets.noninteractive,
        &mut style.visuals.widgets.inactive,
        &mut style.visuals.widgets.hovered,
        &mut style.visuals.widgets.active,
        &mut style.visuals.widgets.open,
    ] {
        widget.rounding = egui::Rounding::ZERO;
    }

    context.set_style(style);
}

fn toolbar_tool(
    ui: &mut egui::Ui,
    enabled: bool,
    active: bool,
    icon: DrawingToolIcon,
    name: &'static str,
) -> bool {
    ui.allocate_ui_with_layout(
        egui::vec2(58.0, 53.0),
        egui::Layout::top_down(egui::Align::Center),
        |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            let clicked = toolbar_button(ui, enabled, active, icon, name);
            ui.label(egui::RichText::new(name).size(9.0).color(if enabled {
                egui::Color32::from_rgb(0xD2, 0xD6, 0xD9)
            } else {
                egui::Color32::from_rgb(0x6A, 0x73, 0x78)
            }));
            clicked
        },
    )
    .inner
}

fn toolbar_button(
    ui: &mut egui::Ui,
    enabled: bool,
    active: bool,
    icon: DrawingToolIcon,
    tooltip: &'static str,
) -> bool {
    let accent = egui::Color32::from_rgb(0x4B, 0xB4, 0xCE);
    let button = egui::Button::new(egui::RichText::new(" "))
        .min_size(egui::vec2(40.0, 40.0))
        .fill(if active {
            accent.gamma_multiply(0.28)
        } else {
            egui::Color32::TRANSPARENT
        })
        .stroke(egui::Stroke::new(
            1.0_f32,
            if active {
                accent
            } else {
                egui::Color32::from_rgb(0x5F, 0x6D, 0x76)
            },
        ));
    let response = ui.add_enabled(enabled, button);
    let icon_color = if !enabled {
        egui::Color32::from_rgb(0x6A, 0x73, 0x78)
    } else if active {
        egui::Color32::WHITE
    } else {
        egui::Color32::from_rgb(0xD2, 0xD6, 0xD9)
    };
    draw_drawing_tool_icon(ui.painter(), response.rect.center(), icon, icon_color);
    let clicked = response.clicked();
    response.on_hover_text(tooltip);
    clicked
}

fn snapshot_view(ui: &mut egui::Ui, snapshot: &SaveSnapshot, language: Language) {
    ui.label(format!("Datei: {}", snapshot.file_name));
    ui.label(format!(
        "Größe: {} Bytes",
        format_number(language, snapshot.byte_length as f64, 0)
    ));
    ui.label("SHA-256:");
    ui.add(egui::Label::new(egui::RichText::new(&snapshot.sha256).monospace()).wrap());
}

fn diff_view(ui: &mut egui::Ui, diff: &DiffSummary, language: Language) {
    ui.label(format!(
        "Geänderte Bytes: {}",
        format_number(language, diff.changed_bytes as f64, 0)
    ));
    ui.label(format!(
        "Geänderte Bereiche: {}",
        format_number(language, diff.changed_ranges as f64, 0)
    ));
    if let Some(offset) = diff.first_changed_offset {
        ui.label(format!(
            "Erster Unterschied bei Byte {}",
            format_number(language, offset as f64, 0)
        ));
    }
}

fn format_diff_status(language: Language, diff: &DiffSummary) -> String {
    format!(
        "{}: {} bytes in {} ranges changed",
        text(language, "save_updated"),
        format_number(language, diff.changed_bytes as f64, 0),
        format_number(language, diff.changed_ranges as f64, 0)
    )
}

fn format_meters(language: Language, value: f32) -> String {
    let meters = value.max(0.0).round() as u64;
    format!("{} m", format_number(language, meters as f64, 0))
}

fn format_playtime(language: Language, seconds: u32) -> String {
    let hours = seconds / 3_600;
    let minutes = (seconds % 3_600) / 60;
    format!(
        "{}h {}min",
        format_number(language, hours as f64, 0),
        format_number(language, minutes as f64, 0)
    )
}

fn format_time_ago(language: Language, updated_at: Instant) -> String {
    let seconds = updated_at.elapsed().as_secs();
    if seconds < 60 {
        let seconds_text = format_number(language, seconds as f64, 0);
        return match language {
            Language::English => format!("{seconds_text}s ago"),
            Language::German => format!("vor {seconds_text}s"),
            Language::French => format!("il y a {seconds_text}s"),
            Language::Spanish => format!("hace {seconds_text}s"),
        };
    }
    let minutes = seconds / 60;
    let minutes_text = format_number(language, minutes as f64, 0);
    if minutes < 60 {
        return match language {
            Language::English => format!("{minutes_text}m ago"),
            Language::German => format!("vor {minutes_text}min"),
            Language::French => format!("il y a {minutes_text}min"),
            Language::Spanish => format!("hace {minutes_text}min"),
        };
    }
    let hours = minutes / 60;
    let hours_text = format_number(language, hours as f64, 0);
    match language {
        Language::English => format!("{hours_text}h ago"),
        Language::German => format!("vor {hours_text}h"),
        Language::French => format!("il y a {hours_text}h"),
        Language::Spanish => format!("hace {hours_text}h"),
    }
}

#[cfg(test)]
mod tests {
    use super::LOADING_GIFS;
    use image::AnimationDecoder;
    use std::io::Cursor;

    #[test]
    fn bundled_loading_gifs_are_animated() {
        for gif in LOADING_GIFS {
            let frames = image::codecs::gif::GifDecoder::new(Cursor::new(gif))
                .expect("bundled loading GIF should decode")
                .into_frames()
                .collect_frames()
                .expect("bundled loading GIF frames should decode");
            assert!(
                frames.len() > 1,
                "loading asset must contain multiple frames"
            );
        }
    }
}
