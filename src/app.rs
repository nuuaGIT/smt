use crate::localization::text;
use crate::map::MapView;
use crate::save_parser::parse_save_data;
use crate::storage::{AppSettings, DiffSummary, Language, RefreshResult, SaveSnapshot, Storage};
use eframe::egui;
use rfd::FileDialog;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;

type ParseResult = Result<crate::save_parser::ParsedSaveData, String>;

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
            drawing_history,
            texts,
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
                    storage.active_drawing_history(),
                    storage.active_texts(),
                )
            })
            .unwrap_or_default();
        app.map.apply_allocations(&allocations);
        app.map.set_rectangles(rectangles);
        app.map.set_circles(circles);
        app.map.set_arrows(arrows);
        app.map.set_drawing_history(drawing_history);
        app.map.set_texts(texts);
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
        );
        app.map.set_node_scale(settings.node_scale);
        app.map.set_show_grid(settings.show_grid);
        app.map.set_show_rails(settings.show_rails);
        app.map.set_show_foundations(settings.show_foundations);
        app.map.set_show_belts(settings.show_belts);
        app.debug_mode = settings.debug_mode;
        app.pause_map_when_unfocused = settings.pause_map_when_unfocused;
        app.right_panel_width = settings.right_panel_width.clamp(8.0, 720.0);
        app
    }

    fn persist_settings(&mut self) {
        let (resource_filter, purity_filter, only_claimed, only_partial) =
            self.map.filter_settings();
        let settings = AppSettings {
            show_node_names: self.map.show_node_names(),
            language: self.language,
            debug_mode: self.debug_mode,
            pause_map_when_unfocused: self.pause_map_when_unfocused,
            resource_filter,
            purity_filter,
            only_claimed,
            node_scale: self.map.node_scale(),
            show_grid: self.map.show_grid(),
            only_partial,
            show_rails: self.map.show_rails(),
            show_foundations: self.map.show_foundations(),
            show_belts: self.map.show_belts(),
            right_panel_width: self.right_panel_width.max(40.0),
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
        self.map.set_drawing_history(Vec::new());
        self.map.set_unclaimed_miner_tier(3);
        self.map.set_play_duration_in_seconds(0);
        self.right_panel_open = false;
        self.right_panel_edge_blocked = true;
        self.right_panel_generation = self.right_panel_generation.wrapping_add(1);
        self.status = text(self.language, "no_save").to_owned();
        self.error = None;
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
                    drawing_history,
                    texts,
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
                            storage.active_drawing_history(),
                            storage.active_texts(),
                        )
                    })
                    .unwrap_or_default();
                self.map.replace_allocations(&allocations);
                self.map.set_unclaimed_miner_tier(miner_tier);
                self.map.set_resource_order(resource_order);
                self.map.set_rectangles(rectangles);
                self.map.set_circles(circles);
                self.map.set_arrows(arrows);
                self.map.set_drawing_history(drawing_history);
                self.map.set_texts(texts);
                self.map.set_play_duration_in_seconds(0);
                self.start_parse(path);
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn refresh_save(&mut self) {
        let result = match self.storage.as_mut() {
            Some(storage) => storage.refresh(),
            None => return,
        };

        match result {
            Ok(RefreshResult::Unchanged) => {
                self.error = None;
                self.status = text(self.language, "no_changes").to_owned();
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

    fn start_parse(&mut self, path: PathBuf) {
        let (sender, receiver) = mpsc::channel();
        self.parse_receiver = Some(receiver);
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
                let (matched, total) = self.map.apply_extractors(&save_data.extractors);
                self.map.replace_map_layers(
                    save_data.rails,
                    save_data.foundations,
                    save_data.belts,
                );
                self.map
                    .set_play_duration_in_seconds(save_data.play_duration_in_seconds);
                self.status = format!(
                    "{}: {matched}/{total} extractors · {} {} · {} {} · {} {}",
                    text(self.language, "save_analyzed"),
                    format_meters(self.map.rail_length_meters()),
                    text(self.language, "rails"),
                    self.map.foundation_count(),
                    text(self.language, "foundation_count"),
                    format_meters(self.map.belt_length_meters()),
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

        egui::TopBottomPanel::top("toolbar").show(context, |ui| {
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

                let mut settings_changed = false;
                ui.menu_button(text(self.language, "settings"), |ui| {
                    self.map.filters(ui);
                    ui.separator();

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

                    let mut show_names = self.map.show_node_names();
                    if ui
                        .checkbox(&mut show_names, text(self.language, "node_names"))
                        .changed()
                    {
                        self.map.set_show_node_names(show_names);
                        settings_changed = true;
                    }
                    if ui
                        .checkbox(&mut self.debug_mode, text(self.language, "debug_mode"))
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
                    ui.collapsing("Testing stuff", |ui| {
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
                    let mut node_scale = self.map.node_scale();
                    let node_scale_label = format!(
                        "{} {:.0}%",
                        text(self.language, "node_size"),
                        node_scale * 100.0
                    );
                    if ui
                        .add(egui::Slider::new(&mut node_scale, 0.5..=1.5).text(node_scale_label))
                        .changed()
                    {
                        self.map.set_node_scale(node_scale);
                        settings_changed = true;
                    }
                    ui.separator();
                    ui.small(text(self.language, "background_throttle"));
                });
                if self.map.take_filter_settings_changed() {
                    settings_changed = true;
                }
                if settings_changed {
                    self.persist_settings();
                }
                ui.label(&self.status);
                ui.separator();
                ui.small(format!(
                    "{}: {}",
                    text(self.language, "playtime"),
                    format_playtime(self.map.play_duration_in_seconds())
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
                            ui.label(format!(
                                "{} {}",
                                self.map.nodes.len(),
                                text(self.language, "resource_nodes")
                            ));
                            ui.label(format!(
                                "{} {}",
                                self.map.claimed_node_count(),
                                text(self.language, "claimed_nodes")
                            ));
                            ui.separator();
                            ui.label(format!(
                                "{}: {}",
                                text(self.language, "belts_laid"),
                                format_meters(self.map.belt_length_meters())
                            ));
                            ui.label(format!(
                                "{}: {}",
                                text(self.language, "rails_laid"),
                                format_meters(self.map.rail_length_meters())
                            ));
                            ui.label(format!(
                                "{}: {}",
                                text(self.language, "playtime"),
                                format_playtime(self.map.play_duration_in_seconds())
                            ));
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

        egui::CentralPanel::default().show(context, |ui| {
            let has_source_save = self
                .storage
                .as_ref()
                .is_some_and(|storage| storage.state.source_path.is_some());
            ui.horizontal(|ui| {
                ui.heading(text(self.language, "app_title"));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let drawing_label = if self.map.rectangle_tool_active() {
                        text(self.language, "cancel_rectangle")
                    } else {
                        text(self.language, "draw_rectangle")
                    };
                    if ui
                        .add_enabled(
                            has_source_save,
                            egui::Button::new(drawing_label).min_size(egui::vec2(150.0, 0.0)),
                        )
                        .clicked()
                    {
                        self.map.toggle_rectangle_tool();
                        context.request_repaint();
                    }
                    let circle_label = if self.map.circle_tool_active() {
                        text(self.language, "cancel_circle")
                    } else {
                        text(self.language, "draw_circle")
                    };
                    if ui
                        .add_enabled(
                            has_source_save,
                            egui::Button::new(circle_label).min_size(egui::vec2(130.0, 0.0)),
                        )
                        .clicked()
                    {
                        self.map.toggle_circle_tool();
                        context.request_repaint();
                    }
                    let arrow_label = if self.map.arrow_tool_active() {
                        text(self.language, "cancel_arrow")
                    } else {
                        text(self.language, "draw_arrow")
                    };
                    if ui
                        .add_enabled(
                            has_source_save,
                            egui::Button::new(arrow_label).min_size(egui::vec2(120.0, 0.0)),
                        )
                        .clicked()
                    {
                        self.map.toggle_arrow_tool();
                        context.request_repaint();
                    }
                    let text_label = if self.map.text_tool_active() {
                        text(self.language, "cancel_text")
                    } else {
                        text(self.language, "draw_text")
                    };
                    if ui
                        .add_enabled(
                            has_source_save,
                            egui::Button::new(text_label).min_size(egui::vec2(120.0, 0.0)),
                        )
                        .clicked()
                    {
                        self.map.toggle_text_tool();
                        context.request_repaint();
                    }
                });
            });
            ui.add_space(16.0);

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

            ui.add_space(12.0);
            egui::CollapsingHeader::new(text(self.language, "save_status"))
                .default_open(false)
                .show(ui, |ui| {
                    let Some(storage) = self.storage.as_ref() else {
                        return;
                    };
                    if let Some(snapshot) = &storage.state.current_snapshot {
                        snapshot_view(ui, snapshot);
                    } else {
                        ui.label(text(self.language, "no_saved_save"));
                    }
                    if let Some(diff) = &storage.state.last_diff {
                        ui.separator();
                        diff_view(ui, diff);
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
                            "FPS: {:.1} · Frame: {:.2} ms",
                            1.0 / stable_dt,
                            stable_dt * 1000.0
                        ));
                        ui.monospace(format!(
                            "CPU letzter Frame: {}",
                            cpu_usage
                                .map(|value| format!("{value:.1}%"))
                                .unwrap_or_else(|| "n/a".to_owned())
                        ));
                        ui.monospace(format!(
                            "Fenster: fokussiert={} minimiert={}",
                            viewport.focused.unwrap_or(false),
                            viewport.minimized.unwrap_or(false)
                        ));
                        ui.monospace(format!(
                            "Nodes: {} total · {} claimed · {} sichtbar",
                            self.map.nodes.len(),
                            self.map.claimed_node_count(),
                            self.map.visible_node_count()
                        ));
                        ui.monospace(format!(
                            "Schienen: {} · {} Segmente",
                            format_meters(self.map.rail_length_meters()),
                            self.map.rail_count()
                        ));
                        ui.monospace(format!(
                            "Layer: {} Foundations · {} Belts · {}",
                            self.map.foundation_count(),
                            self.map.belt_count(),
                            format_meters(self.map.belt_length_meters())
                        ));
                        ui.monospace(format!(
                            "Zoom: {:.2}x · Node-Größe: {:.0}%",
                            self.map.zoom_level(),
                            self.map.node_scale() * 100.0
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
                                "Viewport: {:.0} x {:.0}",
                                rect.width(),
                                rect.height()
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

fn snapshot_view(ui: &mut egui::Ui, snapshot: &SaveSnapshot) {
    ui.label(format!("Datei: {}", snapshot.file_name));
    ui.label(format!("Größe: {} Bytes", snapshot.byte_length));
    ui.label("SHA-256:");
    ui.add(egui::Label::new(egui::RichText::new(&snapshot.sha256).monospace()).wrap());
}

fn diff_view(ui: &mut egui::Ui, diff: &DiffSummary) {
    ui.label(format!("Geänderte Bytes: {}", diff.changed_bytes));
    ui.label(format!("Geänderte Bereiche: {}", diff.changed_ranges));
    if let Some(offset) = diff.first_changed_offset {
        ui.label(format!("Erster Unterschied bei Byte {}", offset));
    }
}

fn format_diff_status(language: Language, diff: &DiffSummary) -> String {
    format!(
        "{}: {} bytes in {} ranges changed",
        text(language, "save_updated"),
        diff.changed_bytes,
        diff.changed_ranges
    )
}

fn format_meters(value: f32) -> String {
    let meters = value.max(0.0).round() as u64;
    let digits = meters.to_string();
    let mut grouped = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, character) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            grouped.push('.');
        }
        grouped.push(character);
    }
    format!("{grouped} m")
}

fn format_playtime(seconds: u32) -> String {
    let hours = seconds / 3_600;
    let minutes = (seconds % 3_600) / 60;
    format!("{hours}h {minutes}min")
}
