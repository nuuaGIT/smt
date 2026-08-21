use crate::localization::text;
use crate::save_parser::{
    ParsedBeltSegment, ParsedExtractor, ParsedFoundation, ParsedRailPoint, ParsedRailSegment,
};
use crate::storage::{
    Language, MapAnnotation, MapArrow, MapCircle, MapRectangle, MapText, NodeAllocation,
};
use crate::world_data::{ExtractionMethod, ResourceNode};
use eframe::egui;
use std::collections::{BTreeSet, HashMap};
use std::path::Path;

const MAP_SIZE: f32 = 8192.0;
const WORLD_TO_PIXEL_SCALE: f32 = 22.887;
const WORLD_OFFSET_X: f32 = 18_282.5;
const WORLD_OFFSET_Y: f32 = 20_480.0;
const OLD_MAP_DESCALE: f32 = 20.0;
const CROP_LO: f32 = 204.8;
const SCALE_TO_HIGHRES: f32 = MAP_SIZE / 1638.4;
const GRID_WORLD_CHUNK_SIZE: f32 = 20_000.0;
const MAX_VISIBLE_GRID_LINES: usize = 128;
const RESOURCE_WELL_MAX_DISTANCE: f32 = 18_000.0;
const WORLD_UNITS_PER_METER: f32 = 100.0;
const FOUNDATION_COORDINATE_SCALE: f32 = 100.0;
const FOUNDATION_FILL_STEP: f32 = 6.0;
const FOUNDATION_STRIPE_SPACING: f32 = 18.0;
const ALL_RESOURCES_FILTER: &str = "__all_resources__";
const ALL_PURITY_FILTER: &str = "__all_purities__";

#[derive(Debug, Clone, Copy, Default)]
struct ResourceWellTotals {
    used_per_minute: f32,
    capacity_per_minute: f32,
    satellite_count: usize,
}

#[derive(Debug, Clone)]
struct FoundationCluster {
    contours: Vec<Vec<[f32; 2]>>,
    min_x: f32,
    min_y: f32,
    max_x: f32,
    max_y: f32,
}

#[derive(Debug, Clone, Default)]
struct ResourceSummary {
    total_per_minute: f32,
    remaining_per_minute: f32,
    partially_used_nodes: usize,
}

const UNCLAIMED_DEFAULT_MAX_OVERCLOCK: f32 = 2.5;

pub struct MapView {
    pub nodes: Vec<ResourceNode>,
    rails: Vec<ParsedRailSegment>,
    foundation_clusters: Vec<FoundationCluster>,
    foundation_instance_count: usize,
    belts: Vec<ParsedBeltSegment>,
    rail_length_meters: f32,
    belt_length_meters: f32,
    play_duration_in_seconds: u32,
    data_source: String,
    background: Option<egui::TextureHandle>,
    resource_icons: std::collections::BTreeMap<String, egui::TextureHandle>,
    resource_icon_colors: std::collections::BTreeMap<String, egui::Color32>,
    resource_icons_loaded: bool,
    zoom: f32,
    pan: egui::Vec2,
    selected_id: Option<String>,
    allocation_dirty: bool,
    allocation_save_requested: bool,
    show_node_names: bool,
    filter_settings_dirty: bool,
    node_scale: f32,
    show_grid: bool,
    show_rails: bool,
    show_foundations: bool,
    show_belts: bool,
    unclaimed_miner_tier: u8,
    resource_order: Vec<String>,
    resource_summary_cache: std::collections::BTreeMap<String, ResourceSummary>,
    resource_summary_dirty: bool,
    resource_filter: String,
    purity_filter: String,
    only_used: bool,
    only_partial: bool,
    language: Language,
    rectangles: Vec<MapRectangle>,
    circles: Vec<MapCircle>,
    arrows: Vec<MapArrow>,
    texts: Vec<MapText>,
    rectangle_tool_active: bool,
    circle_tool_active: bool,
    arrow_tool_active: bool,
    text_tool_active: bool,
    rectangle_start_world: Option<egui::Vec2>,
    rectangle_preview_end_world: Option<egui::Vec2>,
    rectangle_save_requested: bool,
    circle_save_requested: bool,
    arrow_save_requested: bool,
    text_save_requested: bool,
    selected_rectangle: Option<usize>,
    selected_circle: Option<usize>,
    selected_arrow: Option<usize>,
    selected_text: Option<usize>,
    rectangle_popup_position: Option<egui::Pos2>,
    moving_rectangle: Option<usize>,
    rectangle_move_offset: Option<egui::Vec2>,
    text_edit_world: Option<egui::Vec2>,
    text_edit_buffer: String,
    drawing_history: Vec<MapAnnotation>,
    drawing_history_save_requested: bool,
}

impl Default for MapView {
    fn default() -> Self {
        let (nodes, data_source) = crate::world_data::load_nodes();
        Self {
            nodes,
            rails: Vec::new(),
            foundation_clusters: Vec::new(),
            foundation_instance_count: 0,
            belts: Vec::new(),
            rail_length_meters: 0.0,
            belt_length_meters: 0.0,
            play_duration_in_seconds: 0,
            data_source,
            background: None,
            resource_icons: std::collections::BTreeMap::new(),
            resource_icon_colors: std::collections::BTreeMap::new(),
            resource_icons_loaded: false,
            zoom: 1.0,
            pan: egui::Vec2::ZERO,
            selected_id: None,
            allocation_dirty: false,
            allocation_save_requested: false,
            show_node_names: false,
            filter_settings_dirty: false,
            node_scale: 1.0,
            show_grid: false,
            show_rails: false,
            show_foundations: false,
            show_belts: false,
            unclaimed_miner_tier: 3,
            resource_order: Vec::new(),
            resource_summary_cache: std::collections::BTreeMap::new(),
            resource_summary_dirty: true,
            resource_filter: ALL_RESOURCES_FILTER.into(),
            purity_filter: ALL_PURITY_FILTER.into(),
            only_used: false,
            only_partial: false,
            language: Language::English,
            rectangles: Vec::new(),
            circles: Vec::new(),
            arrows: Vec::new(),
            texts: Vec::new(),
            rectangle_tool_active: false,
            circle_tool_active: false,
            arrow_tool_active: false,
            text_tool_active: false,
            rectangle_start_world: None,
            rectangle_preview_end_world: None,
            rectangle_save_requested: false,
            circle_save_requested: false,
            arrow_save_requested: false,
            text_save_requested: false,
            selected_rectangle: None,
            selected_circle: None,
            selected_arrow: None,
            selected_text: None,
            rectangle_popup_position: None,
            moving_rectangle: None,
            rectangle_move_offset: None,
            text_edit_world: None,
            text_edit_buffer: String::new(),
            drawing_history: Vec::new(),
            drawing_history_save_requested: false,
        }
    }
}

impl MapView {
    pub fn set_language(&mut self, language: Language) {
        self.language = language;
    }

    pub fn rectangle_tool_active(&self) -> bool {
        self.rectangle_tool_active
    }

    pub fn circle_tool_active(&self) -> bool {
        self.circle_tool_active
    }

    pub fn arrow_tool_active(&self) -> bool {
        self.arrow_tool_active
    }

    pub fn text_tool_active(&self) -> bool {
        self.text_tool_active
    }

    pub fn toggle_rectangle_tool(&mut self) {
        if self.rectangle_tool_active {
            self.cancel_rectangle_tool();
        } else {
            self.cancel_drawing_tools();
            self.selected_rectangle = None;
            self.rectangle_popup_position = None;
            self.moving_rectangle = None;
            self.rectangle_move_offset = None;
            self.rectangle_tool_active = true;
            self.rectangle_start_world = None;
            self.rectangle_preview_end_world = None;
        }
    }

    pub fn toggle_circle_tool(&mut self) {
        if self.circle_tool_active {
            self.cancel_rectangle_tool();
        } else {
            self.cancel_drawing_tools();
            self.circle_tool_active = true;
            self.rectangle_start_world = None;
            self.rectangle_preview_end_world = None;
        }
    }

    pub fn toggle_arrow_tool(&mut self) {
        if self.arrow_tool_active {
            self.cancel_rectangle_tool();
        } else {
            self.cancel_drawing_tools();
            self.arrow_tool_active = true;
            self.rectangle_start_world = None;
            self.rectangle_preview_end_world = None;
        }
    }

    pub fn toggle_text_tool(&mut self) {
        if self.text_tool_active {
            self.cancel_rectangle_tool();
        } else {
            self.cancel_drawing_tools();
            self.text_tool_active = true;
            self.selected_rectangle = None;
            self.selected_circle = None;
            self.selected_arrow = None;
            self.selected_text = None;
        }
    }

    fn commit_text_edit(&mut self) {
        let Some(position) = self.text_edit_world.take() else {
            return;
        };
        let value = self.text_edit_buffer.trim().to_owned();
        self.text_edit_buffer.clear();
        if value.is_empty() {
            return;
        }
        let annotation = MapText {
            world_x: position.x,
            world_y: position.y,
            text: value,
        };
        self.texts.push(annotation.clone());
        self.text_save_requested = true;
        self.record_drawing(MapAnnotation::Text(annotation));
    }

    fn cancel_text_edit(&mut self) {
        self.text_edit_world = None;
        self.text_edit_buffer.clear();
    }

    fn cancel_drawing_tools(&mut self) {
        self.rectangle_tool_active = false;
        self.circle_tool_active = false;
        self.arrow_tool_active = false;
        self.text_tool_active = false;
        self.text_edit_world = None;
        self.text_edit_buffer.clear();
    }

    fn drawing_tool_active(&self) -> bool {
        self.rectangle_tool_active || self.circle_tool_active || self.arrow_tool_active
    }

    pub fn cancel_rectangle_tool(&mut self) {
        self.cancel_drawing_tools();
        self.rectangle_start_world = None;
        self.rectangle_preview_end_world = None;
    }

    pub fn set_rectangles(&mut self, rectangles: Vec<MapRectangle>) {
        self.rectangles = rectangles;
        self.cancel_rectangle_tool();
        self.rectangle_save_requested = false;
        self.selected_rectangle = None;
        self.selected_circle = None;
        self.selected_arrow = None;
        self.rectangle_popup_position = None;
        self.moving_rectangle = None;
        self.rectangle_move_offset = None;
    }

    pub fn set_circles(&mut self, circles: Vec<MapCircle>) {
        self.circles = circles;
        self.circle_save_requested = false;
    }

    pub fn set_arrows(&mut self, arrows: Vec<MapArrow>) {
        self.arrows = arrows;
        self.arrow_save_requested = false;
    }

    pub fn set_texts(&mut self, texts: Vec<MapText>) {
        self.texts = texts;
        self.text_save_requested = false;
        self.selected_text = None;
        self.text_edit_world = None;
        self.text_edit_buffer.clear();
    }

    pub fn set_drawing_history(&mut self, history: Vec<MapAnnotation>) {
        self.drawing_history = history.into_iter().rev().take(15).collect::<Vec<_>>();
        self.drawing_history.reverse();
        self.drawing_history_save_requested = false;
    }

    fn record_drawing(&mut self, annotation: MapAnnotation) {
        self.drawing_history.push(annotation);
        if self.drawing_history.len() > 15 {
            let remove_count = self.drawing_history.len() - 15;
            self.drawing_history.drain(0..remove_count);
        }
        self.drawing_history_save_requested = true;
    }

    fn forget_drawing(&mut self, annotation: MapAnnotation) {
        if let Some(index) = self
            .drawing_history
            .iter()
            .rposition(|candidate| *candidate == annotation)
        {
            self.drawing_history.remove(index);
            self.drawing_history_save_requested = true;
        }
    }

    pub fn take_rectangle_save(&mut self) -> Option<Vec<MapRectangle>> {
        if !self.rectangle_save_requested {
            return None;
        }
        self.rectangle_save_requested = false;
        Some(self.rectangles.clone())
    }

    pub fn take_circle_save(&mut self) -> Option<Vec<MapCircle>> {
        if !self.circle_save_requested {
            return None;
        }
        self.circle_save_requested = false;
        Some(self.circles.clone())
    }

    pub fn take_arrow_save(&mut self) -> Option<Vec<MapArrow>> {
        if !self.arrow_save_requested {
            return None;
        }
        self.arrow_save_requested = false;
        Some(self.arrows.clone())
    }

    pub fn take_text_save(&mut self) -> Option<Vec<MapText>> {
        if !self.text_save_requested {
            return None;
        }
        self.text_save_requested = false;
        Some(self.texts.clone())
    }

    pub fn take_drawing_history_save(&mut self) -> Option<Vec<MapAnnotation>> {
        if !self.drawing_history_save_requested {
            return None;
        }
        self.drawing_history_save_requested = false;
        Some(self.drawing_history.clone())
    }

    pub fn undo_last_rectangle(&mut self) -> bool {
        if self.drawing_tool_active() {
            self.cancel_rectangle_tool();
            return true;
        }
        if let Some(annotation) = self.drawing_history.pop() {
            match annotation {
                MapAnnotation::Rectangle(rectangle) => {
                    if let Some(index) = self
                        .rectangles
                        .iter()
                        .rposition(|candidate| *candidate == rectangle)
                    {
                        self.rectangles.remove(index);
                        self.rectangle_save_requested = true;
                    }
                }
                MapAnnotation::Circle(circle) => {
                    if let Some(index) = self
                        .circles
                        .iter()
                        .rposition(|candidate| *candidate == circle)
                    {
                        self.circles.remove(index);
                        self.circle_save_requested = true;
                    }
                }
                MapAnnotation::Arrow(arrow) => {
                    if let Some(index) = self
                        .arrows
                        .iter()
                        .rposition(|candidate| *candidate == arrow)
                    {
                        self.arrows.remove(index);
                        self.arrow_save_requested = true;
                    }
                }
                MapAnnotation::Text(text) => {
                    if let Some(index) = self.texts.iter().rposition(|candidate| *candidate == text)
                    {
                        self.texts.remove(index);
                        self.text_save_requested = true;
                    }
                }
            }
            self.drawing_history_save_requested = true;
            self.selected_rectangle = None;
            self.selected_circle = None;
            self.selected_arrow = None;
            self.selected_text = None;
            self.rectangle_popup_position = None;
            return true;
        }
        if self.rectangles.pop().is_some() {
            self.selected_rectangle = None;
            self.rectangle_popup_position = None;
            self.rectangle_save_requested = true;
            true
        } else {
            false
        }
    }

    pub fn show_node_names(&self) -> bool {
        self.show_node_names
    }

    pub fn set_show_node_names(&mut self, show: bool) {
        self.show_node_names = show;
    }

    pub fn node_scale(&self) -> f32 {
        self.node_scale
    }

    pub fn set_node_scale(&mut self, scale: f32) {
        self.node_scale = scale.clamp(0.5, 1.5);
    }

    pub fn show_grid(&self) -> bool {
        self.show_grid
    }

    pub fn set_show_grid(&mut self, show: bool) {
        self.show_grid = show;
    }

    pub fn show_rails(&self) -> bool {
        self.show_rails
    }

    pub fn set_show_rails(&mut self, show: bool) {
        self.show_rails = show;
    }

    pub fn show_foundations(&self) -> bool {
        self.show_foundations
    }

    pub fn set_show_foundations(&mut self, show: bool) {
        self.show_foundations = show;
    }

    pub fn show_belts(&self) -> bool {
        self.show_belts
    }

    pub fn set_show_belts(&mut self, show: bool) {
        self.show_belts = show;
    }

    pub fn unclaimed_miner_tier(&self) -> u8 {
        self.unclaimed_miner_tier
    }

    pub fn set_unclaimed_miner_tier(&mut self, tier: u8) {
        let tier = tier.clamp(1, 3);
        if self.unclaimed_miner_tier != tier {
            self.unclaimed_miner_tier = tier;
            self.resource_summary_dirty = true;
        }
    }

    pub fn resource_order(&self) -> Vec<String> {
        self.resource_order.clone()
    }

    pub fn set_resource_order(&mut self, order: Vec<String>) {
        self.resource_order = order;
    }

    pub fn filter_settings(&self) -> (String, String, bool, bool) {
        (
            self.resource_filter.clone(),
            self.purity_filter.clone(),
            self.only_used,
            self.only_partial,
        )
    }

    pub fn set_filter_settings(
        &mut self,
        resource_filter: String,
        purity_filter: String,
        only_claimed: bool,
        only_partial: bool,
    ) {
        self.resource_filter = normalize_resource_filter(&resource_filter);
        self.purity_filter = normalize_purity_filter(&purity_filter);
        self.only_used = only_claimed;
        self.only_partial = only_partial;
    }

    pub fn take_filter_settings_changed(&mut self) -> bool {
        std::mem::take(&mut self.filter_settings_dirty)
    }

    pub fn zoom_level(&self) -> f32 {
        self.zoom
    }

    pub fn claimed_node_count(&self) -> usize {
        self.nodes
            .iter()
            .filter(|node| node.extractor_instance.is_some())
            .count()
    }

    pub fn rail_count(&self) -> usize {
        self.rails.len()
    }

    pub fn foundation_count(&self) -> usize {
        self.foundation_instance_count
    }

    pub fn belt_count(&self) -> usize {
        self.belts.len()
    }

    pub fn rail_length_meters(&self) -> f32 {
        self.rail_length_meters
    }

    pub fn belt_length_meters(&self) -> f32 {
        self.belt_length_meters
    }

    pub fn play_duration_in_seconds(&self) -> u32 {
        self.play_duration_in_seconds
    }

    pub fn set_play_duration_in_seconds(&mut self, seconds: u32) {
        self.play_duration_in_seconds = seconds;
    }

    pub fn resource_summary(&mut self, ui: &mut egui::Ui) -> bool {
        self.ensure_resource_icons(ui.ctx());

        if self.resource_summary_dirty {
            for node in &mut self.nodes {
                node.clamp_usage_to_capacity();
            }
            let mut summaries = std::collections::BTreeMap::<String, ResourceSummary>::new();
            for node in &self.nodes {
                let capacity = summary_capacity_per_minute(node, self.unclaimed_miner_tier);
                if !capacity.is_finite() || capacity <= 0.0 {
                    continue;
                }
                let summary = summaries.entry(node.resource.clone()).or_default();
                let remaining = if node.extractor_instance.is_some() {
                    (capacity - node.used_per_minute).max(0.0)
                } else {
                    capacity
                };
                summary.total_per_minute += capacity;
                summary.remaining_per_minute += remaining;
                if remaining > 0.01 {
                    summary.partially_used_nodes += 1;
                }
            }
            self.resource_summary_cache = summaries;
            self.resource_summary_dirty = false;
        }

        let summaries = self.resource_summary_cache.clone();

        ui.heading(text(self.language, "resource_remaining"));
        if summaries.is_empty() {
            ui.small(text(self.language, "no_extractors"));
            return false;
        }

        let mut ordered_resources = self
            .resource_order
            .iter()
            .filter(|resource| summaries.contains_key(*resource))
            .cloned()
            .collect::<Vec<_>>();
        for resource in summaries.keys() {
            if !ordered_resources.contains(resource) {
                ordered_resources.push(resource.clone());
            }
        }
        let mut order_changed = ordered_resources != self.resource_order;
        self.resource_order = ordered_resources.clone();

        let total_capacity: f32 = summaries
            .values()
            .map(|summary| summary.total_per_minute)
            .sum();
        let total_remaining: f32 = summaries
            .values()
            .map(|summary| summary.remaining_per_minute)
            .sum();
        let partial_nodes: usize = summaries
            .values()
            .map(|summary| summary.partially_used_nodes)
            .sum();
        ui.small(format!(
            "{} / {} {} · {} {}",
            format_resource_amount(total_remaining),
            format_resource_amount(total_capacity),
            text(self.language, "available_per_minute"),
            partial_nodes,
            text(self.language, "not_fully_used")
        ));
        ui.add_space(6.0);

        let mut pending_drop = None;
        for resource in ordered_resources {
            let Some(summary) = summaries.get(&resource) else {
                continue;
            };
            let fraction = if summary.total_per_minute > 0.0 {
                (summary.remaining_per_minute / summary.total_per_minute).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let low_remaining = summary.remaining_per_minute <= 500.0;
            let color = self
                .resource_icon_colors
                .get(&resource)
                .copied()
                .unwrap_or_else(|| egui::Color32::from_rgb(0xB9, 0xC2, 0xC6));

            let (_, dropped) = ui.dnd_drop_zone::<String, _>(egui::Frame::default(), |ui| {
                let row_tint = if low_remaining {
                    normal_purity_color().gamma_multiply(0.16)
                } else {
                    egui::Color32::TRANSPARENT
                };
                egui::Frame::default()
                    .fill(row_tint)
                    .inner_margin(egui::Margin::symmetric(4.0, 2.0))
                    .show(ui, |ui| {
                        ui.dnd_drag_source(
                            egui::Id::new(("resource-summary-drag", resource.as_str())),
                            resource.clone(),
                            |ui| {
                                ui.horizontal(|ui| {
                                    ui.small("↕");
                                    if let Some(icon) = self.resource_icons.get(&resource) {
                                        ui.add(
                                            egui::Image::from_texture(icon)
                                                .fit_to_exact_size(egui::vec2(28.0, 28.0)),
                                        );
                                    } else {
                                        ui.allocate_space(egui::vec2(28.0, 28.0));
                                    }
                                    let warning_width = if low_remaining { 22.0 } else { 0.0 };
                                    let content_width =
                                        (ui.available_width() - warning_width).max(80.0);
                                    ui.allocate_ui_with_layout(
                                        egui::vec2(content_width, 50.0),
                                        egui::Layout::top_down(egui::Align::Min),
                                        |ui| {
                                            ui.label(&resource);
                                            ui.add(
                                                egui::ProgressBar::new(fraction)
                                                    .fill(color)
                                                    .desired_width(content_width)
                                                    .text(format!(
                                                        "{} / {} {}",
                                                        format_resource_amount(
                                                            summary.remaining_per_minute
                                                        ),
                                                        format_resource_amount(
                                                            summary.total_per_minute
                                                        ),
                                                        text(self.language, "remaining_short")
                                                    )),
                                            );
                                            ui.small(format!(
                                                "{} {} {}",
                                                summary.partially_used_nodes,
                                                text(self.language, "nodes"),
                                                text(self.language, "not_fully_used")
                                            ));
                                        },
                                    );
                                    if low_remaining {
                                        let (_, badge_rect) =
                                            ui.allocate_space(egui::vec2(20.0, 28.0));
                                        draw_resource_warning_badge(
                                            ui.painter(),
                                            badge_rect.center(),
                                            8.0,
                                        );
                                    }
                                });
                            },
                        );
                    });
            });
            if let Some(payload) = dropped {
                if payload.as_str() != resource {
                    pending_drop = Some((payload.to_string(), resource.clone()));
                }
            }
            ui.add_space(4.0);
        }

        if let Some((dragged, target)) = pending_drop {
            if let (Some(from), Some(to)) = (
                self.resource_order.iter().position(|item| item == &dragged),
                self.resource_order.iter().position(|item| item == &target),
            ) {
                let item = self.resource_order.remove(from);
                let insert_at = self
                    .resource_order
                    .iter()
                    .position(|entry| entry == &target)
                    .unwrap_or(to.min(self.resource_order.len()));
                self.resource_order.insert(insert_at, item);
                order_changed = true;
            }
        }
        order_changed
    }

    pub fn replace_map_layers(
        &mut self,
        rails: Vec<ParsedRailSegment>,
        foundations: Vec<ParsedFoundation>,
        belts: Vec<ParsedBeltSegment>,
    ) {
        self.foundation_instance_count = foundations.len();
        self.foundation_clusters = build_foundation_clusters(&foundations);
        self.rail_length_meters = rail_segments_length_meters(&rails);
        self.belt_length_meters = belt_segments_length_meters(&belts);
        self.rails = rails;
        self.belts = belts;
    }

    pub fn visible_node_count(&self) -> usize {
        self.visible_nodes().count()
    }

    pub fn apply_allocations(
        &mut self,
        allocations: &std::collections::BTreeMap<String, NodeAllocation>,
    ) {
        for node in &mut self.nodes {
            let Some(allocation) = allocations.get(&node.id) else {
                continue;
            };
            node.usage_overridden = allocation
                .usage_overridden
                .unwrap_or(allocation.used_per_minute > 0.0);
            node.used_per_minute = if node.usage_overridden {
                allocation.used_per_minute.max(0.0)
            } else {
                0.0
            };
            node.note = allocation.note.clone();
        }
        self.resource_summary_dirty = true;
    }

    pub fn replace_allocations(
        &mut self,
        allocations: &std::collections::BTreeMap<String, NodeAllocation>,
    ) {
        for node in &mut self.nodes {
            node.capacity_per_minute = 0.0;
            node.used_per_minute = 0.0;
            node.usage_overridden = false;
            node.note.clear();
            node.extractor_instance = None;
            node.extractor_kind = None;
            node.power_shards = 0;
            node.current_overclock = 1.0;
            node.max_overclock = 1.0;
        }
        self.rails.clear();
        self.foundation_clusters.clear();
        self.foundation_instance_count = 0;
        self.belts.clear();
        self.rail_length_meters = 0.0;
        self.belt_length_meters = 0.0;
        self.apply_allocations(allocations);
    }

    pub fn take_allocation_save(
        &mut self,
    ) -> Option<std::collections::BTreeMap<String, NodeAllocation>> {
        if !self.allocation_save_requested {
            return None;
        }
        self.allocation_save_requested = false;
        self.allocation_dirty = false;
        Some(
            self.nodes
                .iter()
                .map(|node| {
                    (
                        node.id.clone(),
                        NodeAllocation {
                            capacity_per_minute: node.capacity_per_minute,
                            used_per_minute: node.used_per_minute,
                            note: node.note.clone(),
                            usage_overridden: Some(node.usage_overridden),
                        },
                    )
                })
                .collect(),
        )
    }

    pub fn close_popup_for_background(&mut self) {
        if self.selected_id.take().is_some() && self.allocation_dirty {
            self.allocation_save_requested = true;
        }
    }

    pub fn apply_extractors(&mut self, extractors: &[ParsedExtractor]) -> (usize, usize) {
        for node in &mut self.nodes {
            node.extractor_instance = None;
            node.extractor_kind = None;
            node.power_shards = 0;
            node.current_overclock = 1.0;
            node.max_overclock = 1.0;
        }

        let max_distance_squared = 3_000.0_f32.powi(2);
        let mut matched_indices = BTreeSet::new();
        for extractor in extractors {
            let nearest = self
                .nodes
                .iter()
                .enumerate()
                .filter(|(index, _)| !matched_indices.contains(index))
                .map(|(index, node)| {
                    let dx = node.world_x - extractor.world_x;
                    let dy = node.world_y - extractor.world_y;
                    let dz = (node.world_z - extractor.world_z) * 0.25;
                    (index, dx * dx + dy * dy + dz * dz)
                })
                .min_by(|(_, left), (_, right)| left.total_cmp(right));

            let Some((index, distance_squared)) = nearest else {
                continue;
            };
            if distance_squared > max_distance_squared {
                continue;
            }

            matched_indices.insert(index);
            let node = &mut self.nodes[index];
            node.extractor_instance = Some(extractor.instance_name.clone());
            node.extractor_kind = Some(extractor.kind.clone());
            node.power_shards = extractor.power_shards;
            node.current_overclock = extractor.current_overclock;
            node.max_overclock = extractor.max_overclock;
            node.capacity_per_minute =
                default_capacity_per_minute(&node.purity, &extractor.kind, extractor.max_overclock);
            node.used_per_minute = if node.usage_overridden {
                node.used_per_minute.min(node.capacity_per_minute)
            } else {
                node.capacity_per_minute
            };
        }

        self.resource_summary_dirty = true;
        (matched_indices.len(), extractors.len())
    }

    pub fn filters(&mut self, ui: &mut egui::Ui) {
        let previous_settings = self.filter_settings();
        let previous_grid = self.show_grid;
        ui.heading(text(self.language, "resource_nodes"));
        ui.label(format!(
            "{} {}",
            self.nodes.len(),
            text(self.language, "nodes_loaded")
        ));
        ui.add_space(8.0);

        let resources = self.resource_options();
        let selected_resource = if self.resource_filter == ALL_RESOURCES_FILTER {
            text(self.language, "all_resources")
        } else {
            &self.resource_filter
        };
        egui::ComboBox::from_label(text(self.language, "resource"))
            .selected_text(selected_resource)
            .show_ui(ui, |ui| {
                ui.selectable_value(
                    &mut self.resource_filter,
                    ALL_RESOURCES_FILTER.into(),
                    text(self.language, "all_resources"),
                );
                for resource in resources {
                    ui.selectable_value(&mut self.resource_filter, resource.clone(), resource);
                }
            });

        let selected_purity = if self.purity_filter == ALL_PURITY_FILTER {
            text(self.language, "all_purities")
        } else {
            &self.purity_filter
        };
        egui::ComboBox::from_label(text(self.language, "purity"))
            .selected_text(selected_purity)
            .show_ui(ui, |ui| {
                ui.selectable_value(
                    &mut self.purity_filter,
                    ALL_PURITY_FILTER.into(),
                    text(self.language, "all_purities"),
                );
                for purity in ["Impure", "Normal", "Pure"] {
                    ui.selectable_value(&mut self.purity_filter, purity.into(), purity);
                }
            });

        ui.checkbox(&mut self.only_used, text(self.language, "claimed_only"));
        ui.checkbox(&mut self.only_partial, text(self.language, "partial_only"));
        ui.checkbox(&mut self.show_grid, text(self.language, "grid"));
        if previous_settings != self.filter_settings() || previous_grid != self.show_grid {
            self.filter_settings_dirty = true;
        }
        ui.add_space(12.0);
        ui.label(&self.data_source);
        ui.small(text(self.language, "world_data_hint"));
    }

    pub fn canvas(&mut self, ui: &mut egui::Ui) {
        for node in &mut self.nodes {
            node.clamp_usage_to_capacity();
        }
        self.ensure_background(ui.ctx());
        self.ensure_resource_icons(ui.ctx());

        ui.horizontal(|ui| {
            if ui.button("−").clicked() {
                self.zoom = (self.zoom * 0.8).clamp(0.2, 32.0);
            }
            ui.label(format!("{} {:.1}×", text(self.language, "zoom"), self.zoom));
            if ui.button("+").clicked() {
                self.zoom = (self.zoom * 1.25).clamp(0.2, 32.0);
            }
            if ui.button(text(self.language, "reset_view")).clicked() {
                self.zoom = 1.0;
                self.pan = egui::Vec2::ZERO;
            }
        });
        ui.add_space(4.0);

        let available_size = ui.available_size();
        let (response, painter) =
            ui.allocate_painter(available_size, egui::Sense::click_and_drag());
        let rect = response.rect;

        if response.hovered()
            && self.selected_id.is_none()
            && ui.input(|input| input.modifiers.ctrl && input.key_pressed(egui::Key::Z))
        {
            self.undo_last_rectangle();
            ui.ctx().request_repaint();
        }

        let drawing_tool_was_active = self.drawing_tool_active();
        let text_tool_was_active = self.text_tool_active;
        if drawing_tool_was_active {
            if response.drag_started() {
                self.rectangle_start_world = response
                    .interact_pointer_pos()
                    .map(|position| self.world_at_screen(rect, position));
                self.rectangle_preview_end_world = self.rectangle_start_world;
            }
            if self.rectangle_start_world.is_some() && response.dragged() {
                self.rectangle_preview_end_world = response
                    .interact_pointer_pos()
                    .map(|position| self.world_at_screen(rect, position));
                ui.ctx().request_repaint();
            }
            if response.drag_stopped() {
                if let (Some(start), Some(end)) = (
                    self.rectangle_start_world,
                    self.rectangle_preview_end_world.or_else(|| {
                        response
                            .interact_pointer_pos()
                            .map(|position| self.world_at_screen(rect, position))
                    }),
                ) {
                    if self.rectangle_tool_active {
                        let rectangle = normalize_rectangle(start, end);
                        if (rectangle.max_world_x - rectangle.min_world_x).abs() > 1.0
                            && (rectangle.max_world_y - rectangle.min_world_y).abs() > 1.0
                        {
                            self.rectangles.push(rectangle);
                            self.rectangle_save_requested = true;
                            self.record_drawing(MapAnnotation::Rectangle(rectangle));
                        }
                    } else if self.circle_tool_active {
                        let radius = (end - start).length();
                        if radius > 1.0 {
                            self.circles.push(MapCircle {
                                center_world_x: start.x,
                                center_world_y: start.y,
                                radius_world: radius,
                            });
                            self.circle_save_requested = true;
                            self.record_drawing(MapAnnotation::Circle(
                                *self.circles.last().unwrap(),
                            ));
                        }
                    } else if self.arrow_tool_active && (end - start).length() > 1.0 {
                        self.arrows.push(MapArrow {
                            start_world_x: start.x,
                            start_world_y: start.y,
                            end_world_x: end.x,
                            end_world_y: end.y,
                        });
                        self.arrow_save_requested = true;
                        self.record_drawing(MapAnnotation::Arrow(*self.arrows.last().unwrap()));
                    }
                }
                self.cancel_rectangle_tool();
            }
        } else if let Some(index) = self.moving_rectangle {
            if response.drag_started() {
                if let Some(pointer) = response.interact_pointer_pos() {
                    let pointer_world = self.world_at_screen(rect, pointer);
                    let rectangle = self.rectangles.get(index).copied();
                    let screen_rectangle = rectangle.map(|rectangle| {
                        world_rectangle_screen_rect(
                            rect,
                            rectangle.min_world_x,
                            rectangle.min_world_y,
                            rectangle.max_world_x,
                            rectangle.max_world_y,
                            self.zoom,
                            self.pan,
                        )
                    });
                    if screen_rectangle.is_some_and(|rectangle| rectangle.contains(pointer)) {
                        if let Some(rectangle) = rectangle {
                            let center = egui::vec2(
                                (rectangle.min_world_x + rectangle.max_world_x) * 0.5,
                                (rectangle.min_world_y + rectangle.max_world_y) * 0.5,
                            );
                            self.rectangle_move_offset = Some(pointer_world - center);
                        }
                    }
                }
            }
            if response.dragged() {
                if let (Some(pointer), Some(offset), Some(rectangle)) = (
                    response.interact_pointer_pos(),
                    self.rectangle_move_offset,
                    self.rectangles.get(index).copied(),
                ) {
                    let pointer_world = self.world_at_screen(rect, pointer);
                    let center = pointer_world - offset;
                    let half_width = (rectangle.max_world_x - rectangle.min_world_x) * 0.5;
                    let half_height = (rectangle.max_world_y - rectangle.min_world_y) * 0.5;
                    if let Some(target) = self.rectangles.get_mut(index) {
                        target.min_world_x = center.x - half_width;
                        target.max_world_x = center.x + half_width;
                        target.min_world_y = center.y - half_height;
                        target.max_world_y = center.y + half_height;
                    }
                    ui.ctx().request_repaint();
                }
            }
            if response.drag_stopped() {
                if self.rectangle_move_offset.is_some() {
                    self.rectangle_save_requested = true;
                }
                self.moving_rectangle = None;
                self.rectangle_move_offset = None;
            }
        } else if response.dragged() {
            self.pan += response.drag_delta();
        }

        let text_map_clicked = response.clicked()
            || (response.hovered() && ui.input(|input| input.pointer.primary_clicked()));
        if text_tool_was_active && text_map_clicked {
            if let Some(position) = ui.input(|input| input.pointer.interact_pos()) {
                self.text_edit_world = Some(self.world_at_screen(rect, position));
                self.text_edit_buffer.clear();
                self.text_tool_active = false;
                ui.ctx().request_repaint();
            }
        }

        let scroll = ui.input(|input| input.smooth_scroll_delta.y);
        if response.hovered() && scroll.abs() > f32::EPSILON {
            let old_zoom = self.zoom;
            self.zoom = (self.zoom * (1.0 + scroll / 800.0)).clamp(0.2, 32.0);
            if let Some(pointer) = response.hover_pos() {
                let relative = pointer - rect.center() - self.pan;
                self.pan -= relative * (self.zoom / old_zoom - 1.0);
            }
        }

        painter.rect_filled(rect, 0.0, egui::Color32::from_rgb(19, 35, 31));
        let map_scale = (rect.width().min(rect.height()) / MAP_SIZE) * self.zoom;
        let map_rect = egui::Rect::from_center_size(
            rect.center() + self.pan,
            egui::vec2(MAP_SIZE * map_scale, MAP_SIZE * map_scale),
        );
        if let Some(background) = &self.background {
            painter.image(
                background.id(),
                map_rect,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
        }
        if self.show_grid {
            self.draw_grid(&painter, rect);
        }
        if self.show_foundations {
            if !self.foundation_clusters.is_empty() {
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_millis(33));
            }
            self.draw_foundations(&painter, rect, ui.ctx().input(|input| input.time) as f32);
        }
        if self.show_belts {
            self.draw_belts(&painter, rect);
        }
        if self.show_rails {
            self.draw_rails(&painter, rect);
        }
        self.draw_rectangles(&painter, rect);
        self.draw_circles(&painter, rect);
        self.draw_arrows(&painter, rect);
        self.draw_texts(&painter, rect);
        self.draw_resource_well_links(&painter, rect);

        let mut rectangle_context_opened = false;
        if !drawing_tool_was_active && !text_tool_was_active && self.moving_rectangle.is_none() {
            for (index, rectangle) in self.rectangles.iter().copied().enumerate() {
                let screen_rectangle = world_rectangle_screen_rect(
                    rect,
                    rectangle.min_world_x,
                    rectangle.min_world_y,
                    rectangle.max_world_x,
                    rectangle.max_world_y,
                    self.zoom,
                    self.pan,
                );
                if !screen_rectangle.intersects(rect) {
                    continue;
                }
                let rectangle_response = ui.interact(
                    screen_rectangle.expand(4.0),
                    egui::Id::new(("map-rectangle", index)),
                    egui::Sense::click(),
                );
                if rectangle_response.secondary_clicked() {
                    self.selected_rectangle = Some(index);
                    self.selected_circle = None;
                    self.selected_arrow = None;
                    self.rectangle_popup_position = rectangle_response
                        .interact_pointer_pos()
                        .or_else(|| ui.input(|input| input.pointer.interact_pos()));
                    rectangle_context_opened = true;
                }
            }
        }

        if !drawing_tool_was_active && self.moving_rectangle.is_none() {
            for (index, circle) in self.circles.iter().copied().enumerate() {
                let center = self.to_screen(rect, circle.center_world_x, circle.center_world_y);
                let edge = self.to_screen(
                    rect,
                    circle.center_world_x + circle.radius_world,
                    circle.center_world_y,
                );
                let radius = center.distance(edge);
                let circle_response = ui.interact(
                    egui::Rect::from_center_size(center, egui::vec2(radius * 2.0, radius * 2.0))
                        .expand(4.0),
                    egui::Id::new(("map-circle", index)),
                    egui::Sense::click(),
                );
                if circle_response.secondary_clicked() {
                    self.selected_rectangle = None;
                    self.selected_circle = Some(index);
                    self.selected_arrow = None;
                    self.rectangle_popup_position = circle_response
                        .interact_pointer_pos()
                        .or_else(|| ui.input(|input| input.pointer.interact_pos()));
                    rectangle_context_opened = true;
                }
            }
            for (index, arrow) in self.arrows.iter().copied().enumerate() {
                let start = self.to_screen(rect, arrow.start_world_x, arrow.start_world_y);
                let end = self.to_screen(rect, arrow.end_world_x, arrow.end_world_y);
                let arrow_response = ui.interact(
                    egui::Rect::from_two_pos(start, end).expand(8.0),
                    egui::Id::new(("map-arrow", index)),
                    egui::Sense::click(),
                );
                if arrow_response.secondary_clicked() {
                    self.selected_rectangle = None;
                    self.selected_circle = None;
                    self.selected_arrow = Some(index);
                    self.rectangle_popup_position = arrow_response
                        .interact_pointer_pos()
                        .or_else(|| ui.input(|input| input.pointer.interact_pos()));
                    rectangle_context_opened = true;
                }
            }
            for (index, annotation) in self.texts.iter().enumerate() {
                let position = self.to_screen(rect, annotation.world_x, annotation.world_y);
                let width = (annotation.text.chars().count() as f32 * 9.0).max(48.0);
                let text_response = ui.interact(
                    egui::Rect::from_center_size(position, egui::vec2(width, 26.0)).expand(4.0),
                    egui::Id::new(("map-text", index)),
                    egui::Sense::click(),
                );
                if text_response.secondary_clicked() {
                    self.selected_rectangle = None;
                    self.selected_circle = None;
                    self.selected_arrow = None;
                    self.selected_text = Some(index);
                    self.rectangle_popup_position = text_response
                        .interact_pointer_pos()
                        .or_else(|| ui.input(|input| input.pointer.interact_pos()));
                    rectangle_context_opened = true;
                }
            }
        }

        let mut clicked_node = None;
        for node in self.visible_nodes() {
            let position = self.to_screen(rect, node.world_x, node.world_y);
            let radius = (6.0 * self.zoom.sqrt() * 1.5 * self.node_scale)
                .clamp(7.5 * self.node_scale, 21.0 * self.node_scale);
            let marker_id = egui::Id::new(("resource-node", node.id.as_str()));
            let marker_rect = egui::Rect::from_center_size(
                position,
                egui::vec2((radius + 8.0) * 2.0, (radius + 8.0) * 2.0),
            );
            if !marker_rect.intersects(rect) {
                continue;
            }
            let marker_response = ui.interact(marker_rect, marker_id, egui::Sense::click());
            let hover_progress = ui.ctx().animate_bool_with_time(
                egui::Id::new(("resource-node-hover", node.id.as_str())),
                marker_response.hovered(),
                0.12,
            );
            if hover_progress < 0.999 || marker_response.hovered() {
                ui.ctx().request_repaint();
            }
            let visual_radius = radius + hover_progress * 1.8;

            painter.circle_filled(
                position,
                visual_radius,
                claimed_background_color(node.extractor_instance.is_some()),
            );
            painter.circle_filled(position, visual_radius * 0.66, purity_color(node));
            if let Some(icon) = self.resource_icons.get(&node.resource) {
                let icon_size = visual_radius * 1.05;
                let icon_rect =
                    egui::Rect::from_center_size(position, egui::vec2(icon_size, icon_size));
                painter.image(
                    icon.id(),
                    icon_rect,
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    egui::Color32::WHITE,
                );
            }
            draw_usage_ring(&painter, position, visual_radius, node);
            if node.extractor_instance.is_some() {
                draw_claimed_badge(&painter, position, visual_radius);
                if has_remaining_capacity(node) {
                    draw_partial_usage_badge(&painter, position, visual_radius);
                }
            }

            let marker_hovered = marker_response.hovered();
            let marker_clicked = marker_response.clicked();
            if marker_hovered {
                let well_totals = self.resource_well_totals_for(&node.id);
                marker_response
                    .clone()
                    .on_hover_ui(|ui| node_tooltip(ui, node, well_totals, self.language));
            }

            if marker_clicked
                && !drawing_tool_was_active
                && !text_tool_was_active
                && !rectangle_context_opened
            {
                clicked_node = Some(node.id.clone());
            }

            if self.show_node_names && self.zoom > 1.2 {
                painter.text(
                    position + egui::vec2(radius + 4.0, -8.0),
                    egui::Align2::LEFT_TOP,
                    &node.resource,
                    egui::FontId::proportional(12.0),
                    egui::Color32::WHITE,
                );
            }
        }

        let node_was_clicked = clicked_node.is_some();
        if let Some(clicked_id) = clicked_node {
            if self.selected_id.as_deref() != Some(clicked_id.as_str()) {
                if self.selected_id.is_some() && self.allocation_dirty {
                    self.allocation_save_requested = true;
                }
                self.selected_id = Some(clicked_id);
            }
        }

        let text_editor_rect = self.draw_text_editor(ui.ctx(), rect);
        let rectangle_popup_rect = self.draw_selected_rectangle_popup(ui.ctx(), rect);
        let simple_annotation_popup_rect =
            self.draw_selected_simple_annotation_popup(ui.ctx(), rect);
        let popup_rect = self.draw_selected_popup(ui.ctx(), rect);
        if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
            if self.drawing_tool_active() || self.text_tool_active || self.text_edit_world.is_some()
            {
                self.cancel_rectangle_tool();
                self.cancel_text_edit();
            } else if self.moving_rectangle.take().is_some() {
                self.rectangle_move_offset = None;
            } else if self.selected_rectangle.take().is_some() {
                // Close the rectangle action popup.
                self.rectangle_popup_position = None;
            } else if self.selected_circle.take().is_some()
                || self.selected_arrow.take().is_some()
                || self.selected_text.take().is_some()
            {
                self.rectangle_popup_position = None;
            } else if self.selected_id.take().is_some() && self.allocation_dirty {
                self.allocation_save_requested = true;
            }
        }
        let pointer_clicked = ui.input(|input| input.pointer.any_click());
        if pointer_clicked
            && !node_was_clicked
            && !drawing_tool_was_active
            && !text_tool_was_active
            && !rectangle_context_opened
        {
            let pointer_position = ui.input(|input| input.pointer.interact_pos());
            let inside_popup = pointer_position
                .zip(popup_rect)
                .is_some_and(|(position, popup)| popup.contains(position));
            let inside_rectangle_popup = pointer_position
                .zip(rectangle_popup_rect)
                .is_some_and(|(position, popup)| popup.contains(position));
            let inside_simple_annotation_popup = pointer_position
                .zip(simple_annotation_popup_rect)
                .is_some_and(|(position, popup)| popup.contains(position));
            let inside_text_editor = pointer_position
                .zip(text_editor_rect)
                .is_some_and(|(position, popup)| popup.contains(position));
            if !inside_popup
                && !inside_rectangle_popup
                && !inside_simple_annotation_popup
                && !inside_text_editor
            {
                if self.selected_rectangle.is_some() {
                    self.selected_rectangle = None;
                    self.rectangle_popup_position = None;
                }
                if self.selected_circle.is_some() || self.selected_arrow.is_some() {
                    self.selected_circle = None;
                    self.selected_arrow = None;
                    self.rectangle_popup_position = None;
                }
            }
            if !inside_popup
                && !inside_rectangle_popup
                && !inside_simple_annotation_popup
                && !inside_text_editor
                && self.selected_id.take().is_some()
                && self.allocation_dirty
            {
                self.allocation_save_requested = true;
            }
        }

        let visible_count = self.visible_nodes().count();
        painter.text(
            rect.left_top() + egui::vec2(12.0, 12.0),
            egui::Align2::LEFT_TOP,
            format!(
                "{} {:.2} · {} {}",
                text(self.language, "zoom"),
                self.zoom,
                visible_count,
                text(self.language, "visible_nodes")
            ),
            egui::FontId::proportional(13.0),
            egui::Color32::from_gray(210),
        );

        let cursor_text = response
            .hover_pos()
            .map(|position| {
                let world = self.world_at_screen(rect, position);
                format!(
                    "{} · X {:.0} · Y {:.0}",
                    text(self.language, "cursor_world"),
                    world.x,
                    world.y
                )
            })
            .unwrap_or_else(|| format!("{} · X — · Y —", text(self.language, "cursor_world")));
        painter.text(
            rect.center_bottom() - egui::vec2(0.0, 10.0),
            egui::Align2::CENTER_BOTTOM,
            cursor_text,
            egui::FontId::monospace(12.0),
            egui::Color32::from_gray(210),
        );
    }

    fn draw_selected_rectangle_popup(
        &mut self,
        context: &egui::Context,
        map_rect: egui::Rect,
    ) -> Option<egui::Rect> {
        let index = self.selected_rectangle?;
        let rectangle = self.rectangles.get(index).copied()?;
        let rectangle_screen = world_rectangle_screen_rect(
            map_rect,
            rectangle.min_world_x,
            rectangle.min_world_y,
            rectangle.max_world_x,
            rectangle.max_world_y,
            self.zoom,
            self.pan,
        );
        let available = context.available_rect();
        let popup_size = egui::vec2(250.0, 150.0);
        let mut popup_position = self
            .rectangle_popup_position
            .unwrap_or_else(|| rectangle_screen.right_top() + egui::vec2(12.0, 12.0));
        if popup_position.x + popup_size.x > available.right() {
            popup_position.x = rectangle_screen.left() - popup_size.x - 12.0;
        }
        if popup_position.y + popup_size.y > available.bottom() {
            popup_position.y = rectangle_screen.top() - popup_size.y - 12.0;
        }

        let mut delete_requested = false;
        let mut move_requested = false;
        let mut close_requested = false;
        let inner = egui::Area::new(egui::Id::new("selected-map-rectangle-popup"))
            .order(egui::Order::Foreground)
            .fixed_pos(popup_position)
            .show(context, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_min_width(230.0);
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(text(self.language, "rectangle")).strong());
                        if ui
                            .button("×")
                            .on_hover_text(text(self.language, "close"))
                            .clicked()
                        {
                            close_requested = true;
                        }
                    });
                    ui.small(format!(
                        "X {:.0}–{:.0} · Y {:.0}–{:.0}",
                        rectangle.min_world_x,
                        rectangle.max_world_x,
                        rectangle.min_world_y,
                        rectangle.max_world_y
                    ));
                    ui.horizontal(|ui| {
                        if ui.button(text(self.language, "move_rectangle")).clicked() {
                            move_requested = true;
                        }
                        if ui.button(text(self.language, "delete_rectangle")).clicked() {
                            delete_requested = true;
                        }
                    });
                });
            });

        if delete_requested {
            if let Some(rectangle) = self.rectangles.get(index).copied() {
                self.rectangles.remove(index);
                self.forget_drawing(MapAnnotation::Rectangle(rectangle));
                self.rectangle_save_requested = true;
            }
            self.selected_rectangle = None;
            self.rectangle_popup_position = None;
        } else if move_requested {
            self.moving_rectangle = Some(index);
            self.rectangle_move_offset = None;
            self.selected_rectangle = None;
            self.rectangle_popup_position = None;
        } else if close_requested {
            self.selected_rectangle = None;
            self.rectangle_popup_position = None;
        }
        Some(inner.response.rect)
    }

    fn draw_selected_simple_annotation_popup(
        &mut self,
        context: &egui::Context,
        map_rect: egui::Rect,
    ) -> Option<egui::Rect> {
        let (kind, index) = if let Some(index) = self.selected_circle {
            (0_u8, index)
        } else if let Some(index) = self.selected_arrow {
            (1_u8, index)
        } else if let Some(index) = self.selected_text {
            (2_u8, index)
        } else {
            return None;
        };

        let (label, position) = if kind == 0 {
            let circle = self.circles.get(index).copied()?;
            (
                text(self.language, "circle"),
                self.to_screen(map_rect, circle.center_world_x, circle.center_world_y),
            )
        } else if kind == 1 {
            let arrow = self.arrows.get(index).copied()?;
            (
                text(self.language, "arrow"),
                self.to_screen(map_rect, arrow.end_world_x, arrow.end_world_y),
            )
        } else {
            let annotation = self.texts.get(index)?;
            (
                text(self.language, "text_label"),
                self.to_screen(map_rect, annotation.world_x, annotation.world_y),
            )
        };
        let available = context.available_rect();
        let popup_size = egui::vec2(210.0, 100.0);
        let mut popup_position = self
            .rectangle_popup_position
            .unwrap_or_else(|| position + egui::vec2(12.0, 12.0));
        if popup_position.x + popup_size.x > available.right() {
            popup_position.x = available.right() - popup_size.x - 8.0;
        }
        if popup_position.y + popup_size.y > available.bottom() {
            popup_position.y = available.bottom() - popup_size.y - 8.0;
        }

        let mut delete_requested = false;
        let mut close_requested = false;
        let inner = egui::Area::new(egui::Id::new("selected-map-simple-annotation-popup"))
            .order(egui::Order::Foreground)
            .fixed_pos(popup_position)
            .show(context, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(label).strong());
                        if ui
                            .button("×")
                            .on_hover_text(text(self.language, "close"))
                            .clicked()
                        {
                            close_requested = true;
                        }
                    });
                    if ui
                        .button(text(self.language, "delete_annotation"))
                        .clicked()
                    {
                        delete_requested = true;
                    }
                });
            });

        if delete_requested {
            if kind == 0 {
                if let Some(circle) = self.circles.get(index).copied() {
                    self.circles.remove(index);
                    self.forget_drawing(MapAnnotation::Circle(circle));
                    self.circle_save_requested = true;
                }
                self.selected_circle = None;
            } else if kind == 1 {
                if let Some(arrow) = self.arrows.get(index).copied() {
                    self.arrows.remove(index);
                    self.forget_drawing(MapAnnotation::Arrow(arrow));
                    self.arrow_save_requested = true;
                }
                self.selected_arrow = None;
            } else {
                if let Some(annotation) = self.texts.get(index).cloned() {
                    self.texts.remove(index);
                    self.forget_drawing(MapAnnotation::Text(annotation));
                    self.text_save_requested = true;
                }
                self.selected_text = None;
            }
            self.rectangle_popup_position = None;
        } else if close_requested {
            self.selected_circle = None;
            self.selected_arrow = None;
            self.selected_text = None;
            self.rectangle_popup_position = None;
        }
        Some(inner.response.rect)
    }

    fn draw_selected_popup(
        &mut self,
        context: &egui::Context,
        map_rect: egui::Rect,
    ) -> Option<egui::Rect> {
        let selected_id = self.selected_id.clone()?;
        let node_position = self
            .nodes
            .iter()
            .find(|node| node.id == selected_id)
            .map(|node| self.to_screen(map_rect, node.world_x, node.world_y))?;

        let available = context.available_rect();
        let popup_size = egui::vec2(340.0, 390.0);
        let popup_progress = context.animate_bool_with_time(
            egui::Id::new(("node-popup-animation", selected_id.as_str())),
            true,
            0.16,
        );
        if popup_progress < 0.999 {
            context.request_repaint();
        }
        let mut popup_position = node_position + egui::vec2(20.0, 20.0);
        if popup_position.x + popup_size.x > available.right() {
            popup_position.x = node_position.x - popup_size.x - 20.0;
        }
        if popup_position.y + popup_size.y > available.bottom() {
            popup_position.y = node_position.y - popup_size.y - 20.0;
        }
        popup_position.y += (1.0 - popup_progress) * 10.0;
        let well_totals = self.resource_well_totals_for(&selected_id);

        let mut changed = false;
        let mut close_requested = false;
        let inner = egui::Area::new(egui::Id::new("selected-node-popup"))
            .order(egui::Order::Foreground)
            .fixed_pos(popup_position)
            .show(context, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_min_width(310.0);
                    let Some(node) = self.nodes.iter_mut().find(|node| node.id == selected_id)
                    else {
                        return;
                    };

                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(&node.resource).heading());
                        ui.label(
                            egui::RichText::new(format!("· {}", node.purity))
                                .color(purity_color(node)),
                        );
                        if ui
                            .button("×")
                            .on_hover_text(text(self.language, "close"))
                            .clicked()
                        {
                            close_requested = true;
                        }
                    });
                    node_details(ui, node, self.language);
                    ui.separator();
                    if let Some(totals) = well_totals {
                        show_resource_well_totals(ui, totals, self.language);
                    } else {
                        ui.label(egui::RichText::new(text(self.language, "usage")).strong());
                        ui.label(format!(
                            "{}: {:.0} / min",
                            text(self.language, "maximum_settable"),
                            node.capacity_per_minute
                        ));
                        ui.label(format!(
                            "{}: {} · Overclock: {:.0}% · Maximum: {:.0}%",
                            text(self.language, "powershards_clock"),
                            node.power_shards,
                            node.current_overclock * 100.0,
                            node.max_overclock * 100.0
                        ));
                        let usage_changed = ui
                            .add(
                                egui::DragValue::new(&mut node.used_per_minute)
                                    .speed(1.0)
                                    .range(0.0..=node.capacity_per_minute)
                                    .suffix(format!(
                                        " / min {}",
                                        text(self.language, "used_per_minute")
                                    )),
                            )
                            .changed();
                        if usage_changed {
                            node.usage_overridden = true;
                            changed = true;
                        }
                        node.clamp_usage_to_capacity();

                        let utilization = node.utilization();
                        ui.label(format!(
                            "{}: {:.0} / min",
                            text(self.language, "free_per_minute"),
                            node.remaining_per_minute()
                        ));
                        ui.label(format!(
                            "{}: {:.1}%",
                            text(self.language, "occupancy"),
                            utilization * 100.0
                        ));
                        ui.add(
                            egui::ProgressBar::new(utilization.clamp(0.0, 1.0)).show_percentage(),
                        );
                        ui.label(text(self.language, "note"));
                    }
                    changed |= ui.text_edit_multiline(&mut node.note).changed();
                });
            });

        if changed {
            self.allocation_dirty = true;
            self.resource_summary_dirty = true;
        }
        if close_requested {
            self.allocation_save_requested = self.allocation_dirty;
            self.selected_id = None;
        }
        Some(inner.response.rect)
    }

    fn visible_nodes(&self) -> impl Iterator<Item = &ResourceNode> {
        self.nodes.iter().filter(|node| {
            (self.resource_filter == ALL_RESOURCES_FILTER || node.resource == self.resource_filter)
                && (self.purity_filter == ALL_PURITY_FILTER || node.purity == self.purity_filter)
                && (!self.only_used || node.extractor_instance.is_some())
                && (!self.only_partial || has_remaining_capacity(node))
        })
    }

    fn resource_options(&self) -> Vec<String> {
        self.nodes
            .iter()
            .map(|node| node.resource.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    fn to_screen(&self, rect: egui::Rect, world_x: f32, world_y: f32) -> egui::Pos2 {
        let map_scale = (rect.width().min(rect.height()) / MAP_SIZE) * self.zoom;
        let map_position = project_world(world_x, world_y);
        rect.center()
            + (map_position - egui::vec2(MAP_SIZE / 2.0, MAP_SIZE / 2.0)) * map_scale
            + self.pan
    }

    fn world_at_screen(&self, rect: egui::Rect, screen: egui::Pos2) -> egui::Vec2 {
        let map_scale = (rect.width().min(rect.height()) / MAP_SIZE) * self.zoom;
        if map_scale <= f32::EPSILON {
            return egui::Vec2::ZERO;
        }
        let map_position = (screen - rect.center() - self.pan) / map_scale
            + egui::vec2(MAP_SIZE / 2.0, MAP_SIZE / 2.0);
        unproject_map(map_position)
    }

    fn ensure_background(&mut self, context: &egui::Context) {
        if self.background.is_some() {
            return;
        }

        let Ok(image) = image::open("data/map_highres.png") else {
            return;
        };
        let image = image.to_rgba8();
        let size = [image.width() as usize, image.height() as usize];
        let color_image = egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw());
        self.background = Some(context.load_texture(
            "satisfactory-world-map",
            color_image,
            egui::TextureOptions::LINEAR,
        ));
    }

    fn ensure_resource_icons(&mut self, context: &egui::Context) {
        if self.resource_icons_loaded {
            return;
        }
        self.resource_icons_loaded = true;

        for (resource, filename) in resource_icon_files() {
            let path = Path::new("data/SMT-icons").join(filename);
            let Ok(image) = image::open(&path) else {
                continue;
            };
            let image = image.to_rgba8();
            if let Some(color) = average_icon_color(&image) {
                self.resource_icon_colors.insert(resource.into(), color);
            }
            let size = [image.width() as usize, image.height() as usize];
            let color_image = egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw());
            let texture = context.load_texture(
                format!("satisfactory-resource-icon-{resource}"),
                color_image,
                egui::TextureOptions::LINEAR,
            );
            self.resource_icons.insert(resource.into(), texture);
        }
    }

    fn draw_grid(&self, painter: &egui::Painter, rect: egui::Rect) {
        let grid_color = egui::Color32::from_rgba_unmultiplied(160, 210, 180, 28);
        let (min_world_x, max_world_x, min_world_y, max_world_y) = self.visible_world_bounds(rect);

        let first_x = (min_world_x / GRID_WORLD_CHUNK_SIZE).floor() * GRID_WORLD_CHUNK_SIZE;
        let mut x = first_x;
        let mut line_count = 0;
        while x <= max_world_x && line_count < MAX_VISIBLE_GRID_LINES {
            let screen_x = self.to_screen(rect, x, min_world_y).x;
            painter.line_segment(
                [
                    egui::pos2(screen_x, rect.top()),
                    egui::pos2(screen_x, rect.bottom()),
                ],
                egui::Stroke::new(1.0_f32, grid_color),
            );
            x += GRID_WORLD_CHUNK_SIZE;
            line_count += 1;
        }

        let first_y = (min_world_y / GRID_WORLD_CHUNK_SIZE).floor() * GRID_WORLD_CHUNK_SIZE;
        let mut y = first_y;
        line_count = 0;
        while y <= max_world_y && line_count < MAX_VISIBLE_GRID_LINES {
            let screen_y = self.to_screen(rect, min_world_x, y).y;
            painter.line_segment(
                [
                    egui::pos2(rect.left(), screen_y),
                    egui::pos2(rect.right(), screen_y),
                ],
                egui::Stroke::new(1.0_f32, grid_color),
            );
            y += GRID_WORLD_CHUNK_SIZE;
            line_count += 1;
        }
    }

    fn draw_rectangles(&self, painter: &egui::Painter, rect: egui::Rect) {
        let fill = annotation_color(24);
        let stroke = egui::Stroke::new(2.0_f32, annotation_color(255));

        for rectangle in &self.rectangles {
            draw_world_rectangle(
                painter,
                rect,
                rectangle.min_world_x,
                rectangle.min_world_y,
                rectangle.max_world_x,
                rectangle.max_world_y,
                self.zoom,
                self.pan,
                fill,
                stroke,
            );
        }

        if self.rectangle_tool_active {
            if let (Some(start), Some(end)) =
                (self.rectangle_start_world, self.rectangle_preview_end_world)
            {
                let rectangle = normalize_rectangle(start, end);
                draw_world_rectangle(
                    painter,
                    rect,
                    rectangle.min_world_x,
                    rectangle.min_world_y,
                    rectangle.max_world_x,
                    rectangle.max_world_y,
                    self.zoom,
                    self.pan,
                    annotation_color(46),
                    egui::Stroke::new(2.0_f32, annotation_color(255)),
                );
            }
        }
    }

    fn draw_circles(&self, painter: &egui::Painter, rect: egui::Rect) {
        let stroke = egui::Stroke::new(2.0_f32, annotation_color(255));
        let fill = annotation_color(42);
        for circle in &self.circles {
            let center = self.to_screen(rect, circle.center_world_x, circle.center_world_y);
            let edge = self.to_screen(
                rect,
                circle.center_world_x + circle.radius_world,
                circle.center_world_y,
            );
            let radius = center.distance(edge);
            painter.circle_filled(center, radius, fill);
            painter.circle_stroke(center, radius, stroke);
        }
        if self.circle_tool_active {
            if let (Some(start), Some(end)) =
                (self.rectangle_start_world, self.rectangle_preview_end_world)
            {
                let center = self.to_screen(rect, start.x, start.y);
                let edge = self.to_screen(rect, end.x, end.y);
                let radius = center.distance(edge);
                painter.circle_filled(center, radius, annotation_color(55));
                painter.circle_stroke(
                    center,
                    radius,
                    egui::Stroke::new(2.0_f32, annotation_color(255)),
                );
            }
        }
    }

    fn draw_arrows(&self, painter: &egui::Painter, rect: egui::Rect) {
        let stroke = egui::Stroke::new(2.4_f32, annotation_color(255));
        for arrow in &self.arrows {
            draw_screen_arrow(
                painter,
                self.to_screen(rect, arrow.start_world_x, arrow.start_world_y),
                self.to_screen(rect, arrow.end_world_x, arrow.end_world_y),
                stroke,
            );
        }
        if self.arrow_tool_active {
            if let (Some(start), Some(end)) =
                (self.rectangle_start_world, self.rectangle_preview_end_world)
            {
                draw_screen_arrow(
                    painter,
                    self.to_screen(rect, start.x, start.y),
                    self.to_screen(rect, end.x, end.y),
                    egui::Stroke::new(2.4_f32, annotation_color(255)),
                );
            }
        }
    }

    fn draw_texts(&self, painter: &egui::Painter, rect: egui::Rect) {
        for annotation in &self.texts {
            painter.text(
                self.to_screen(rect, annotation.world_x, annotation.world_y),
                egui::Align2::CENTER_CENTER,
                &annotation.text,
                // Keep the annotation readable at a constant screen size. Its
                // anchor still uses world coordinates, so it remains attached
                // to the map while zooming and panning.
                egui::FontId::proportional(16.0),
                annotation_color(255),
            );
        }
    }

    fn draw_text_editor(
        &mut self,
        context: &egui::Context,
        map_rect: egui::Rect,
    ) -> Option<egui::Rect> {
        let world = self.text_edit_world?;
        let available = context.available_rect();
        let anchor = self.to_screen(map_rect, world.x, world.y);
        let popup_size = egui::vec2(270.0, 92.0);
        let mut position = anchor + egui::vec2(10.0, 10.0);
        if position.x + popup_size.x > available.right() {
            position.x = available.right() - popup_size.x - 8.0;
        }
        if position.y + popup_size.y > available.bottom() {
            position.y = available.bottom() - popup_size.y - 8.0;
        }

        let mut commit = false;
        let mut cancel = false;
        let editor_id = egui::Id::new("map-text-editor-input");
        let inner = egui::Area::new(egui::Id::new("map-text-editor"))
            .order(egui::Order::Foreground)
            .fixed_pos(position)
            .show(context, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.label(egui::RichText::new(text(self.language, "text_label")).strong());
                    let response = ui
                        .add(egui::TextEdit::singleline(&mut self.text_edit_buffer).id(editor_id));
                    if self.text_edit_buffer.is_empty() {
                        ui.memory_mut(|memory| memory.request_focus(editor_id));
                    }
                    if response.lost_focus()
                        && ui.input(|input| input.key_pressed(egui::Key::Enter))
                    {
                        commit = true;
                    }
                    ui.horizontal(|ui| {
                        if ui.button(text(self.language, "save_text")).clicked() {
                            commit = true;
                        }
                        if ui.button(text(self.language, "cancel_text_edit")).clicked() {
                            cancel = true;
                        }
                    });
                });
            });
        if commit {
            self.commit_text_edit();
        } else if cancel {
            self.cancel_text_edit();
        }
        Some(inner.response.rect)
    }

    fn draw_foundations(&self, painter: &egui::Painter, rect: egui::Rect, time: f32) {
        let visible_rect = rect.expand(8.0);
        let fill = egui::Color32::from_rgba_unmultiplied(185, 194, 198, 24);
        let outline = egui::Color32::from_rgba_unmultiplied(185, 194, 198, 150);
        let stripe = egui::Color32::from_rgba_unmultiplied(185, 194, 198, 72);
        let mut fill_mesh = egui::Mesh::default();
        let mut stripe_mesh = egui::Mesh::default();
        let mut outline_mesh = egui::Mesh::default();
        for cluster in &self.foundation_clusters {
            let bounds = egui::Rect::from_min_max(
                self.to_screen(rect, cluster.min_x, cluster.min_y),
                self.to_screen(rect, cluster.max_x, cluster.max_y),
            )
            .expand(2.0);
            if !bounds.intersects(visible_rect) {
                continue;
            }
            for contour in &cluster.contours {
                let points = contour
                    .iter()
                    .map(|point| self.to_screen(rect, point[0], point[1]))
                    .collect::<Vec<_>>();
                if points.len() < 3 {
                    continue;
                }
                let contour_bounds = egui::Rect::from_points(&points);
                if !contour_bounds.intersects(visible_rect) {
                    continue;
                }
                append_scanline_fill(&mut fill_mesh, &points, fill, FOUNDATION_FILL_STEP);
                append_animated_stripes(
                    &mut stripe_mesh,
                    &points,
                    stripe,
                    time,
                    FOUNDATION_STRIPE_SPACING,
                );
                for pair in points
                    .iter()
                    .copied()
                    .zip(points.iter().copied().cycle().skip(1))
                    .take(points.len())
                {
                    push_thick_line(&mut outline_mesh, pair.0, pair.1, 1.1_f32, outline);
                }
            }
        }

        if !fill_mesh.indices.is_empty() {
            painter.add(egui::Shape::mesh(fill_mesh));
        }
        if !stripe_mesh.indices.is_empty() {
            painter.add(egui::Shape::mesh(stripe_mesh));
        }
        if !outline_mesh.indices.is_empty() {
            painter.add(egui::Shape::mesh(outline_mesh));
        }
    }

    fn draw_belts(&self, painter: &egui::Painter, rect: egui::Rect) {
        self.draw_spline_layer(
            painter,
            rect,
            self.belts.iter().map(|segment| segment.points.as_slice()),
            egui::Color32::from_rgb(0xEA, 0xD5, 0x6E),
            2.5_f32,
        );
    }

    fn draw_spline_layer<'a, I>(
        &self,
        painter: &egui::Painter,
        rect: egui::Rect,
        segments: I,
        color: egui::Color32,
        line_width: f32,
    ) where
        I: IntoIterator<Item = &'a [crate::save_parser::ParsedRailPoint]>,
    {
        let visible_rect = rect.expand(12.0);
        let mut underlay_mesh = egui::Mesh::default();
        let mut overlay_mesh = egui::Mesh::default();

        for points in segments {
            self.push_spline_mesh(
                rect,
                visible_rect,
                points,
                &mut underlay_mesh,
                &mut overlay_mesh,
                line_width,
                color,
                10,
            );
        }

        if !underlay_mesh.indices.is_empty() {
            painter.add(egui::Shape::mesh(underlay_mesh));
        }
        if !overlay_mesh.indices.is_empty() {
            painter.add(egui::Shape::mesh(overlay_mesh));
        }
    }

    fn draw_rails(&self, painter: &egui::Painter, rect: egui::Rect) {
        self.draw_spline_layer(
            painter,
            rect,
            self.rails.iter().map(|segment| segment.points.as_slice()),
            egui::Color32::from_rgb(0xB9, 0xC2, 0xC6),
            1.8_f32,
        );
    }

    fn push_spline_mesh(
        &self,
        rect: egui::Rect,
        visible_rect: egui::Rect,
        points: &[crate::save_parser::ParsedRailPoint],
        underlay_mesh: &mut egui::Mesh,
        overlay_mesh: &mut egui::Mesh,
        line_width: f32,
        color: egui::Color32,
        max_samples: usize,
    ) {
        for pair in points.windows(2) {
            let start = self.to_screen(rect, pair[0].location[0], pair[0].location[1]);
            let end = self.to_screen(rect, pair[1].location[0], pair[1].location[1]);
            let control_one = self.to_screen_world_offset(
                rect,
                pair[0].location,
                pair[0].leave_tangent,
                1.0 / 3.0,
            );
            let control_two = self.to_screen_world_offset(
                rect,
                pair[1].location,
                pair[1].arrive_tangent,
                -1.0 / 3.0,
            );
            let bounds = egui::Rect::from_points(&[start, control_one, control_two, end]);
            if !bounds.intersects(visible_rect) {
                continue;
            }

            let samples = (((end - start).length() / 12.0).ceil() as usize).clamp(2, max_samples);
            let mut previous = None;
            for index in 0..=samples {
                let t = index as f32 / samples as f32;
                let current = cubic_bezier(start, control_one, control_two, end, t);
                if let Some(previous) = previous {
                    push_thick_line(
                        underlay_mesh,
                        previous,
                        current,
                        line_width + 2.0,
                        egui::Color32::from_rgb(0x1B, 0x25, 0x2B),
                    );
                    push_thick_line(overlay_mesh, previous, current, line_width, color);
                }
                previous = Some(current);
            }
        }
    }

    fn to_screen_world_offset(
        &self,
        rect: egui::Rect,
        world_position: [f32; 3],
        world_offset: [f32; 3],
        factor: f32,
    ) -> egui::Pos2 {
        self.to_screen(
            rect,
            world_position[0] + world_offset[0] * factor,
            world_position[1] + world_offset[1] * factor,
        )
    }

    fn draw_resource_well_links(&self, painter: &egui::Painter, rect: egui::Rect) {
        let cores: Vec<&ResourceNode> = self
            .visible_nodes()
            .filter(|node| node.extraction_method == ExtractionMethod::ResourceWellCore)
            .collect();
        let satellites: Vec<&ResourceNode> = self
            .visible_nodes()
            .filter(|node| {
                node.extraction_method == ExtractionMethod::ResourceWellExtractor
                    && node.extractor_instance.is_some()
            })
            .collect();

        for satellite in satellites {
            let Some(core) = cores
                .iter()
                .filter(|core| core.resource == satellite.resource)
                .min_by(|left, right| {
                    world_distance_squared(left, satellite)
                        .total_cmp(&world_distance_squared(right, satellite))
                })
                .copied()
            else {
                continue;
            };

            // Resource-well groups are compact. This guard avoids connecting
            // unrelated wells when static node data is incomplete.
            if world_distance_squared(core, satellite) > RESOURCE_WELL_MAX_DISTANCE.powi(2) {
                continue;
            }

            let start = self.to_screen(rect, core.world_x, core.world_y);
            let end = self.to_screen(rect, satellite.world_x, satellite.world_y);
            let delta = end - start;
            let length = delta.length();
            if length <= f32::EPSILON {
                continue;
            }

            let direction = delta / length;
            let normal = egui::vec2(-direction.y, direction.x);
            let sign = satellite
                .id
                .bytes()
                .fold(0_u32, |sum, byte| sum.wrapping_add(byte as u32))
                .is_multiple_of(2);
            let side = if sign { 1.0 } else { -1.0 };
            let bend = (length * 0.18).clamp(14.0, 90.0) * side;
            let control_one = start + delta * 0.28 + normal * bend;
            let control_two = start + delta * 0.72 + normal * bend;

            let segments = 28;
            let mut points = Vec::with_capacity(segments + 1);
            for index in 0..=segments {
                let t = index as f32 / segments as f32;
                points.push(cubic_bezier(start, control_one, control_two, end, t));
            }

            if !points.iter().any(|point| rect.contains(*point)) {
                continue;
            }

            let line_color = egui::Color32::from_rgba_unmultiplied(0x08, 0xC4, 0x62, 175);
            let line_width = 2.6_f32;
            painter.add(egui::Shape::line(
                points,
                egui::Stroke::new(line_width, line_color),
            ));
            painter.circle_filled(start, line_width * 0.5, line_color);
            painter.circle_filled(end, line_width * 0.5, line_color);
        }
    }

    fn resource_well_totals_for(&self, core_id: &str) -> Option<ResourceWellTotals> {
        let core = self
            .nodes
            .iter()
            .find(|node| node.id == core_id)
            .filter(|node| node.extraction_method == ExtractionMethod::ResourceWellCore)?;

        let mut totals = ResourceWellTotals::default();
        for satellite in self.nodes.iter().filter(|node| {
            node.extraction_method == ExtractionMethod::ResourceWellExtractor
                && node.resource == core.resource
                && self.satellite_belongs_to_core(core, node)
        }) {
            let capacity = resource_well_satellite_capacity(satellite);
            totals.capacity_per_minute += capacity;
            totals.used_per_minute += if satellite.extractor_instance.is_some() {
                satellite.used_per_minute.clamp(0.0, capacity)
            } else {
                0.0
            };
            totals.satellite_count += 1;
        }

        Some(totals)
    }

    fn satellite_belongs_to_core(&self, core: &ResourceNode, satellite: &ResourceNode) -> bool {
        if world_distance_squared(core, satellite) > RESOURCE_WELL_MAX_DISTANCE.powi(2) {
            return false;
        }

        self.nodes
            .iter()
            .filter(|candidate| {
                candidate.extraction_method == ExtractionMethod::ResourceWellCore
                    && candidate.resource == satellite.resource
            })
            .min_by(|left, right| {
                world_distance_squared(left, satellite)
                    .total_cmp(&world_distance_squared(right, satellite))
            })
            .is_some_and(|nearest| nearest.id == core.id)
    }

    fn visible_world_bounds(&self, rect: egui::Rect) -> (f32, f32, f32, f32) {
        let map_scale = (rect.width().min(rect.height()) / MAP_SIZE) * self.zoom;
        let map_at_screen = |screen: egui::Pos2| {
            (screen - rect.center() - self.pan) / map_scale
                + egui::vec2(MAP_SIZE / 2.0, MAP_SIZE / 2.0)
        };

        let top_left = unproject_map(map_at_screen(rect.left_top()));
        let bottom_right = unproject_map(map_at_screen(rect.right_bottom()));
        (
            top_left.x.min(bottom_right.x),
            top_left.x.max(bottom_right.x),
            top_left.y.min(bottom_right.y),
            top_left.y.max(bottom_right.y),
        )
    }
}

fn normalize_resource_filter(value: &str) -> String {
    if value == ALL_RESOURCES_FILTER
        || value.eq_ignore_ascii_case("Alle Ressourcen")
        || value.eq_ignore_ascii_case("All resources")
        || value.eq_ignore_ascii_case("Toutes les ressources")
        || value.eq_ignore_ascii_case("Todos los recursos")
    {
        ALL_RESOURCES_FILTER.to_owned()
    } else {
        value.to_owned()
    }
}

fn normalize_rectangle(start: egui::Vec2, end: egui::Vec2) -> MapRectangle {
    MapRectangle {
        min_world_x: start.x.min(end.x),
        min_world_y: start.y.min(end.y),
        max_world_x: start.x.max(end.x),
        max_world_y: start.y.max(end.y),
    }
}

fn draw_world_rectangle(
    painter: &egui::Painter,
    rect: egui::Rect,
    min_world_x: f32,
    min_world_y: f32,
    max_world_x: f32,
    max_world_y: f32,
    zoom: f32,
    pan: egui::Vec2,
    fill: egui::Color32,
    stroke: egui::Stroke,
) {
    let screen_rect = world_rectangle_screen_rect(
        rect,
        min_world_x,
        min_world_y,
        max_world_x,
        max_world_y,
        zoom,
        pan,
    );
    let screen_points = [
        screen_rect.left_top(),
        screen_rect.right_top(),
        screen_rect.right_bottom(),
        screen_rect.left_bottom(),
    ];
    if !screen_rect.intersects(rect.expand(8.0)) {
        return;
    }
    painter.rect_filled(screen_rect, 0.0, fill);
    for pair in screen_points
        .iter()
        .copied()
        .zip(screen_points.iter().copied().cycle().skip(1))
        .take(4)
    {
        painter.line_segment([pair.0, pair.1], stroke);
    }
}

fn world_rectangle_screen_rect(
    rect: egui::Rect,
    min_world_x: f32,
    min_world_y: f32,
    max_world_x: f32,
    max_world_y: f32,
    zoom: f32,
    pan: egui::Vec2,
) -> egui::Rect {
    let map_scale = (rect.width().min(rect.height()) / MAP_SIZE) * zoom;
    let points = [
        project_world(min_world_x, min_world_y),
        project_world(max_world_x, min_world_y),
        project_world(max_world_x, max_world_y),
        project_world(min_world_x, max_world_y),
    ]
    .map(|map_position| {
        rect.center()
            + (map_position - egui::vec2(MAP_SIZE / 2.0, MAP_SIZE / 2.0)) * map_scale
            + pan
    });
    egui::Rect::from_points(&points)
}

fn draw_screen_arrow(
    painter: &egui::Painter,
    start: egui::Pos2,
    end: egui::Pos2,
    stroke: egui::Stroke,
) {
    let direction = end - start;
    let length = direction.length();
    if length <= f32::EPSILON {
        return;
    }
    let unit = direction / length;
    let perpendicular = egui::vec2(-unit.y, unit.x);
    let head_length = length.min(16.0);
    let head_width = head_length * 0.55;
    let base = end - unit * head_length;
    painter.line_segment([start, end], stroke);
    painter.add(egui::Shape::convex_polygon(
        vec![
            end,
            base + perpendicular * head_width,
            base - perpendicular * head_width,
        ],
        stroke.color,
        egui::Stroke::NONE,
    ));
}

fn normalize_purity_filter(value: &str) -> String {
    if value == ALL_PURITY_FILTER
        || value.eq_ignore_ascii_case("Alle Reinheiten")
        || value.eq_ignore_ascii_case("All purities")
        || value.eq_ignore_ascii_case("Toutes les puretés")
        || value.eq_ignore_ascii_case("Todas las purezas")
    {
        ALL_PURITY_FILTER.to_owned()
    } else {
        value.to_owned()
    }
}

fn world_distance_squared(left: &ResourceNode, right: &ResourceNode) -> f32 {
    let dx = left.world_x - right.world_x;
    let dy = left.world_y - right.world_y;
    let dz = (left.world_z - right.world_z) * 0.25;
    dx * dx + dy * dy + dz * dz
}

fn build_foundation_clusters(foundations: &[ParsedFoundation]) -> Vec<FoundationCluster> {
    type GridPoint = (i64, i64);
    type AxisEdge = (i64, i64, i8);

    let mut horizontal: HashMap<i64, Vec<AxisEdge>> = HashMap::new();
    let mut vertical: HashMap<i64, Vec<AxisEdge>> = HashMap::new();
    let mut generic_edges: HashMap<(GridPoint, GridPoint), i32> = HashMap::new();

    for foundation in foundations {
        let points = foundation.corners.map(|corner| (corner[0], corner[1]));
        if !points.iter().all(|(x, y)| x.is_finite() && y.is_finite()) {
            continue;
        }

        for pair in points
            .iter()
            .copied()
            .zip(points.iter().copied().cycle().skip(1))
            .take(points.len())
        {
            let start = (
                quantize_foundation_coordinate(pair.0 .0),
                quantize_foundation_coordinate(pair.0 .1),
            );
            let end = (
                quantize_foundation_coordinate(pair.1 .0),
                quantize_foundation_coordinate(pair.1 .1),
            );
            if start == end {
                continue;
            }

            if start.1 == end.1 {
                let direction = if start.0 < end.0 { 1 } else { -1 };
                let interval = (start.0.min(end.0), start.0.max(end.0), direction);
                horizontal.entry(start.1).or_default().push(interval);
            } else if start.0 == end.0 {
                let direction = if start.1 < end.1 { 1 } else { -1 };
                let interval = (start.1.min(end.1), start.1.max(end.1), direction);
                vertical.entry(start.0).or_default().push(interval);
            } else {
                let key = (start, end);
                let reverse = (end, start);
                if generic_edges.remove(&reverse).is_none() {
                    *generic_edges.entry(key).or_insert(0) += 1;
                }
            }
        }
    }

    let mut boundary_edges = collect_axis_boundary_edges(horizontal, true);
    boundary_edges.extend(collect_axis_boundary_edges(vertical, false));
    boundary_edges.extend(
        generic_edges
            .into_iter()
            .filter(|(_, count)| *count != 0)
            .map(|((start, end), _)| (start, end)),
    );

    trace_foundation_contours(boundary_edges)
        .into_iter()
        .filter_map(|contour| {
            if contour.len() < 3 {
                return None;
            }
            let min_x = contour
                .iter()
                .map(|point| point[0])
                .fold(f32::INFINITY, f32::min);
            let min_y = contour
                .iter()
                .map(|point| point[1])
                .fold(f32::INFINITY, f32::min);
            let max_x = contour
                .iter()
                .map(|point| point[0])
                .fold(f32::NEG_INFINITY, f32::max);
            let max_y = contour
                .iter()
                .map(|point| point[1])
                .fold(f32::NEG_INFINITY, f32::max);
            Some(FoundationCluster {
                contours: vec![contour],
                min_x,
                min_y,
                max_x,
                max_y,
            })
        })
        .collect()
}

fn quantize_foundation_coordinate(value: f32) -> i64 {
    (value * FOUNDATION_COORDINATE_SCALE).round() as i64
}

fn dequantize_foundation_coordinate(value: i64) -> f32 {
    value as f32 / FOUNDATION_COORDINATE_SCALE
}

fn collect_axis_boundary_edges(
    lines: HashMap<i64, Vec<(i64, i64, i8)>>,
    horizontal: bool,
) -> Vec<((i64, i64), (i64, i64))> {
    let mut result = Vec::new();
    for (line, edges) in lines {
        let mut breakpoints = edges
            .iter()
            .flat_map(|edge| [edge.0, edge.1])
            .collect::<Vec<_>>();
        breakpoints.sort_unstable();
        breakpoints.dedup();

        for pair in breakpoints.windows(2) {
            let left = pair[0];
            let right = pair[1];
            if left == right {
                continue;
            }
            let midpoint = (left as f64 + right as f64) * 0.5;
            let direction_sum: i32 = edges
                .iter()
                .filter(|edge| (edge.0 as f64) < midpoint && midpoint < edge.1 as f64)
                .map(|edge| edge.2 as i32)
                .sum();
            if direction_sum == 0 {
                continue;
            }

            let (start, end) = if direction_sum > 0 {
                (left, right)
            } else {
                (right, left)
            };
            if horizontal {
                result.push(((start, line), (end, line)));
            } else {
                result.push(((line, start), (line, end)));
            }
        }
    }
    result
}

fn trace_foundation_contours(edges: Vec<((i64, i64), (i64, i64))>) -> Vec<Vec<[f32; 2]>> {
    let mut outgoing: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
    for (index, (_, end)) in edges.iter().enumerate() {
        outgoing.entry(*end).or_default();
        outgoing.entry(edges[index].0).or_default().push(index);
    }
    let mut used = vec![false; edges.len()];
    let mut contours = Vec::new();

    for start_index in 0..edges.len() {
        if used[start_index] {
            continue;
        }
        let start = edges[start_index].0;
        let mut current_index = start_index;
        let mut contour = Vec::new();
        for _ in 0..edges.len() {
            if used[current_index] {
                break;
            }
            used[current_index] = true;
            let (edge_start, edge_end) = edges[current_index];
            if contour.is_empty() {
                contour.push([
                    dequantize_foundation_coordinate(edge_start.0),
                    dequantize_foundation_coordinate(edge_start.1),
                ]);
            }
            contour.push([
                dequantize_foundation_coordinate(edge_end.0),
                dequantize_foundation_coordinate(edge_end.1),
            ]);
            if edge_end == start {
                break;
            }
            let Some(next_index) = outgoing
                .get(&edge_end)
                .and_then(|indices| indices.iter().copied().find(|index| !used[*index]))
            else {
                break;
            };
            current_index = next_index;
        }

        if contour.len() >= 4 && contour.last() == contour.first() {
            contour.pop();
            simplify_collinear_contour(&mut contour);
            contours.push(contour);
        }
    }
    contours
}

fn simplify_collinear_contour(contour: &mut Vec<[f32; 2]>) {
    if contour.len() < 3 {
        return;
    }
    let mut simplified = Vec::with_capacity(contour.len());
    for index in 0..contour.len() {
        let previous = contour[(index + contour.len() - 1) % contour.len()];
        let current = contour[index];
        let next = contour[(index + 1) % contour.len()];
        let cross = (current[0] - previous[0]) * (next[1] - current[1])
            - (current[1] - previous[1]) * (next[0] - current[0]);
        if cross.abs() > 0.01 {
            simplified.push(current);
        }
    }
    if simplified.len() >= 3 {
        *contour = simplified;
    }
}

fn rail_segments_length_meters(segments: &[ParsedRailSegment]) -> f32 {
    segments
        .iter()
        .map(|segment| spline_points_length_meters(&segment.points))
        .sum()
}

fn belt_segments_length_meters(segments: &[ParsedBeltSegment]) -> f32 {
    segments
        .iter()
        .map(|segment| spline_points_length_meters(&segment.points))
        .sum()
}

fn spline_points_length_meters(points: &[ParsedRailPoint]) -> f32 {
    let mut length_world_units = 0.0_f32;
    for pair in points.windows(2) {
        let start = pair[0].location;
        let control_one = add_scaled_vector(start, pair[0].leave_tangent, 1.0 / 3.0);
        let control_two = add_scaled_vector(pair[1].location, pair[1].arrive_tangent, -1.0 / 3.0);
        let end = pair[1].location;
        let mut previous = start;
        for index in 1..=12 {
            let t = index as f32 / 12.0;
            let current = cubic_bezier_world(start, control_one, control_two, end, t);
            length_world_units += distance_squared_vector(previous, current).sqrt();
            previous = current;
        }
    }
    length_world_units / WORLD_UNITS_PER_METER
}

fn add_scaled_vector(left: [f32; 3], right: [f32; 3], factor: f32) -> [f32; 3] {
    [
        left[0] + right[0] * factor,
        left[1] + right[1] * factor,
        left[2] + right[2] * factor,
    ]
}

fn distance_squared_vector(left: [f32; 3], right: [f32; 3]) -> f32 {
    let dx = right[0] - left[0];
    let dy = right[1] - left[1];
    let dz = right[2] - left[2];
    dx * dx + dy * dy + dz * dz
}

fn cubic_bezier_world(
    start: [f32; 3],
    control_one: [f32; 3],
    control_two: [f32; 3],
    end: [f32; 3],
    t: f32,
) -> [f32; 3] {
    let one_minus_t = 1.0 - t;
    [
        start[0] * one_minus_t.powi(3)
            + control_one[0] * 3.0 * one_minus_t.powi(2) * t
            + control_two[0] * 3.0 * one_minus_t * t.powi(2)
            + end[0] * t.powi(3),
        start[1] * one_minus_t.powi(3)
            + control_one[1] * 3.0 * one_minus_t.powi(2) * t
            + control_two[1] * 3.0 * one_minus_t * t.powi(2)
            + end[1] * t.powi(3),
        start[2] * one_minus_t.powi(3)
            + control_one[2] * 3.0 * one_minus_t.powi(2) * t
            + control_two[2] * 3.0 * one_minus_t * t.powi(2)
            + end[2] * t.powi(3),
    ]
}

fn cubic_bezier(
    start: egui::Pos2,
    control_one: egui::Pos2,
    control_two: egui::Pos2,
    end: egui::Pos2,
    t: f32,
) -> egui::Pos2 {
    let one_minus_t = 1.0 - t;
    let point = start.to_vec2() * one_minus_t.powi(3)
        + control_one.to_vec2() * 3.0 * one_minus_t.powi(2) * t
        + control_two.to_vec2() * 3.0 * one_minus_t * t.powi(2)
        + end.to_vec2() * t.powi(3);
    egui::pos2(point.x, point.y)
}

fn append_scanline_fill(
    mesh: &mut egui::Mesh,
    points: &[egui::Pos2],
    color: egui::Color32,
    step: f32,
) {
    let bounds = egui::Rect::from_points(points);
    let mut y = bounds.top() - step * 0.5;
    while y <= bounds.bottom() {
        let intersections = polygon_scanline_intersections(points, y);
        for pair in intersections.chunks_exact(2) {
            push_quad(
                mesh,
                [
                    egui::pos2(pair[0], y),
                    egui::pos2(pair[1], y),
                    egui::pos2(pair[1], y + step),
                    egui::pos2(pair[0], y + step),
                ],
                color,
            );
        }
        y += step;
    }
}

fn polygon_scanline_intersections(points: &[egui::Pos2], y: f32) -> Vec<f32> {
    let mut intersections = Vec::new();
    for pair in points
        .iter()
        .copied()
        .zip(points.iter().copied().cycle().skip(1))
        .take(points.len())
    {
        let (start, end) = pair;
        if (start.y <= y && end.y > y) || (end.y <= y && start.y > y) {
            let factor = (y - start.y) / (end.y - start.y);
            intersections.push(start.x + (end.x - start.x) * factor);
        }
    }
    intersections.sort_by(f32::total_cmp);
    intersections
}

fn append_animated_stripes(
    mesh: &mut egui::Mesh,
    points: &[egui::Pos2],
    color: egui::Color32,
    time: f32,
    spacing: f32,
) {
    let bounds = egui::Rect::from_points(points);
    let diagonal = bounds.width() + bounds.height();
    let phase = (time * 28.0).rem_euclid(spacing);
    let mut intercept = bounds.left() + bounds.top() - diagonal + phase;
    while intercept <= bounds.right() + bounds.bottom() + diagonal {
        let mut intersections = Vec::new();
        for pair in points
            .iter()
            .copied()
            .zip(points.iter().copied().cycle().skip(1))
            .take(points.len())
        {
            let (start, end) = pair;
            let start_value = start.x + start.y;
            let end_value = end.x + end.y;
            if (start_value <= intercept && end_value > intercept)
                || (end_value <= intercept && start_value > intercept)
            {
                let factor = (intercept - start_value) / (end_value - start_value);
                intersections.push(egui::pos2(
                    start.x + (end.x - start.x) * factor,
                    start.y + (end.y - start.y) * factor,
                ));
            }
        }
        intersections.sort_by(|left, right| left.x.total_cmp(&right.x));
        intersections.dedup_by(|left, right| left.distance(*right) < 0.5);
        for pair in intersections.chunks_exact(2) {
            push_thick_line(mesh, pair[0], pair[1], 1.2, color);
        }
        intercept += spacing;
    }
}

fn mesh_vertex(position: egui::Pos2, color: egui::Color32) -> egui::epaint::Vertex {
    egui::epaint::Vertex {
        pos: position,
        uv: egui::pos2(0.0, 0.0),
        color,
    }
}

fn push_quad(mesh: &mut egui::Mesh, corners: [egui::Pos2; 4], color: egui::Color32) {
    let base = mesh.vertices.len() as u32;
    mesh.vertices.extend(
        corners
            .into_iter()
            .map(|position| mesh_vertex(position, color)),
    );
    mesh.indices
        .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
}

fn push_thick_line(
    mesh: &mut egui::Mesh,
    start: egui::Pos2,
    end: egui::Pos2,
    width: f32,
    color: egui::Color32,
) {
    let delta = end - start;
    let length = delta.length();
    if length <= f32::EPSILON {
        return;
    }
    let normal = egui::vec2(-delta.y, delta.x) * (width * 0.5 / length);
    push_quad(
        mesh,
        [start + normal, end + normal, end - normal, start - normal],
        color,
    );
}

fn resource_well_satellite_capacity(node: &ResourceNode) -> f32 {
    if node.capacity_per_minute > 0.0 && node.capacity_per_minute.is_finite() {
        return node.capacity_per_minute;
    }

    // Unclaimed satellites have no extractor object in the save. Their
    // theoretical baseline is still part of the well's total potential.
    60.0 * purity_factor(node)
}

fn project_world(world_x: f32, world_y: f32) -> egui::Vec2 {
    let project_axis = |position: f32, offset: f32| {
        ((position / WORLD_TO_PIXEL_SCALE + offset) / OLD_MAP_DESCALE - CROP_LO) * SCALE_TO_HIGHRES
    };
    egui::vec2(
        project_axis(world_x, WORLD_OFFSET_X),
        project_axis(world_y, WORLD_OFFSET_Y),
    )
}

fn unproject_map(map_x: egui::Vec2) -> egui::Vec2 {
    let unproject_axis = |pixel: f32, offset: f32| {
        ((pixel / SCALE_TO_HIGHRES + CROP_LO) * OLD_MAP_DESCALE - offset) * WORLD_TO_PIXEL_SCALE
    };
    egui::vec2(
        unproject_axis(map_x.x, WORLD_OFFSET_X),
        unproject_axis(map_x.y, WORLD_OFFSET_Y),
    )
}

fn annotation_color(alpha: u8) -> egui::Color32 {
    egui::Color32::from_rgba_unmultiplied(0xFF, 0x98, 0x25, alpha)
}

fn purity_color(node: &ResourceNode) -> egui::Color32 {
    match node.purity.as_str() {
        "Pure" => egui::Color32::from_rgb(0x09, 0xE5, 0x89),
        "Impure" => egui::Color32::from_rgb(0xEA, 0x6E, 0x7F),
        _ => egui::Color32::from_rgb(0xEA, 0xD5, 0x6E),
    }
}

fn occupancy_color(occupied: bool) -> egui::Color32 {
    if occupied {
        egui::Color32::from_rgb(0x08, 0xC4, 0x62)
    } else {
        egui::Color32::from_rgb(0xB9, 0xC2, 0xC6)
    }
}

fn claimed_background_color(claimed: bool) -> egui::Color32 {
    if claimed {
        egui::Color32::from_rgb(0x34, 0x3F, 0x49)
    } else {
        egui::Color32::from_rgb(0x7A, 0x88, 0x95)
    }
}

fn resource_icon_files() -> [(&'static str, &'static str); 13] {
    [
        ("Bauxite", "Bauxite.webp"),
        ("Caterium Ore", "Caterium_Ore.webp"),
        ("Coal", "Coal.webp"),
        ("Copper Ore", "Copper_Ore.webp"),
        ("Crude Oil", "Crude_Oil.webp"),
        ("Iron Ore", "Iron_Ore.webp"),
        ("Limestone", "Limestone.webp"),
        ("Nitrogen Gas", "Nitrogen_Gas.webp"),
        ("Raw Quartz", "Raw_Quartz.webp"),
        ("SAM", "SAM_Ore.webp"),
        ("Sulfur", "Sulfur.webp"),
        ("Uranium", "Uranium.webp"),
        ("Water", "Water.webp"),
    ]
}

fn average_icon_color(image: &image::RgbaImage) -> Option<egui::Color32> {
    let mut red = 0_u64;
    let mut green = 0_u64;
    let mut blue = 0_u64;
    let mut weight = 0_u64;
    for pixel in image.pixels() {
        let alpha = u64::from(pixel[3]);
        if alpha == 0 {
            continue;
        }
        red += u64::from(pixel[0]) * alpha;
        green += u64::from(pixel[1]) * alpha;
        blue += u64::from(pixel[2]) * alpha;
        weight += alpha;
    }
    (weight > 0).then(|| {
        egui::Color32::from_rgb(
            (red / weight) as u8,
            (green / weight) as u8,
            (blue / weight) as u8,
        )
    })
}

fn format_resource_amount(value: f32) -> String {
    let rounded = value.max(0.0).round() as u64;
    let digits = rounded.to_string();
    let mut grouped = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, character) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            grouped.push('.');
        }
        grouped.push(character);
    }
    grouped
}

fn draw_claimed_badge(painter: &egui::Painter, position: egui::Pos2, radius: f32) {
    let badge_center = position + egui::vec2(-radius * 0.78, -radius * 0.78);
    let badge_radius = (radius * 0.46).clamp(4.0, 6.5);
    let background = egui::Color32::from_rgb(0x08, 0xC4, 0x62);

    painter.circle_filled(badge_center, badge_radius, background);
    painter.circle_stroke(
        badge_center,
        badge_radius,
        egui::Stroke::new(
            1.0_f32,
            egui::Color32::from_rgba_unmultiplied(12, 32, 24, 220),
        ),
    );

    // Material check icon geometry, rendered locally so no external font is needed.
    let check_stroke = egui::Stroke::new((badge_radius * 0.24).max(1.0), egui::Color32::WHITE);
    painter.line_segment(
        [
            badge_center + egui::vec2(-badge_radius * 0.48, 0.0),
            badge_center + egui::vec2(-badge_radius * 0.10, badge_radius * 0.38),
        ],
        check_stroke,
    );
    painter.line_segment(
        [
            badge_center + egui::vec2(-badge_radius * 0.10, badge_radius * 0.38),
            badge_center + egui::vec2(badge_radius * 0.52, -badge_radius * 0.44),
        ],
        check_stroke,
    );
}

fn draw_partial_usage_badge(painter: &egui::Painter, position: egui::Pos2, radius: f32) {
    let badge_center = position + egui::vec2(radius * 0.78, -radius * 0.78);
    let badge_radius = (radius * 0.46).clamp(4.0, 6.5);
    draw_resource_warning_badge(painter, badge_center, badge_radius);
}

fn draw_resource_warning_badge(
    painter: &egui::Painter,
    badge_center: egui::Pos2,
    badge_radius: f32,
) {
    let background = normal_purity_color();
    let icon_color = egui::Color32::from_rgb(0x34, 0x3F, 0x49);

    painter.circle_filled(badge_center, badge_radius, background);
    painter.circle_stroke(
        badge_center,
        badge_radius,
        egui::Stroke::new(
            1.0_f32,
            egui::Color32::from_rgba_unmultiplied(52, 63, 73, 220),
        ),
    );

    let exclamation_stroke = egui::Stroke::new((badge_radius * 0.24).max(1.0), icon_color);
    painter.line_segment(
        [
            badge_center + egui::vec2(0.0, -badge_radius * 0.48),
            badge_center + egui::vec2(0.0, badge_radius * 0.16),
        ],
        exclamation_stroke,
    );
    painter.circle_filled(
        badge_center + egui::vec2(0.0, badge_radius * 0.48),
        (badge_radius * 0.12).max(1.0),
        icon_color,
    );
}

fn has_remaining_capacity(node: &ResourceNode) -> bool {
    node.extractor_instance.is_some()
        && node.capacity_per_minute > 0.0
        && node.utilization() < 0.9999
}

fn normal_purity_color() -> egui::Color32 {
    egui::Color32::from_rgb(0xEA, 0xD5, 0x6E)
}

fn draw_usage_ring(
    painter: &egui::Painter,
    position: egui::Pos2,
    radius: f32,
    node: &ResourceNode,
) {
    let track_stroke = egui::Stroke::new(3.2_f32, occupancy_color(false));
    painter.circle_stroke(position, radius, track_stroke);

    let utilization = if node.extractor_instance.is_some() {
        node.utilization().clamp(0.0, 1.0)
    } else {
        0.0
    };
    if utilization <= f32::EPSILON {
        return;
    }

    let progress_stroke = egui::Stroke::new(3.2_f32, occupancy_color(true));
    if utilization >= 0.9999 {
        painter.circle_stroke(position, radius, progress_stroke);
        return;
    }

    // Start at 12 o'clock and fill clockwise. The track remains visible for
    // the unused part of the node's available output.
    let segments = ((utilization * 96.0).ceil() as usize).max(2);
    let mut points = Vec::with_capacity(segments + 1);
    for index in 0..=segments {
        let fraction = utilization * index as f32 / segments as f32;
        let angle = -std::f32::consts::FRAC_PI_2 + std::f32::consts::TAU * fraction;
        points.push(position + egui::vec2(angle.cos() * radius, angle.sin() * radius));
    }
    painter.add(egui::Shape::line(points, progress_stroke));
}

fn node_tooltip(
    ui: &mut egui::Ui,
    node: &ResourceNode,
    well_totals: Option<ResourceWellTotals>,
    language: Language,
) {
    ui.label(egui::RichText::new(&node.resource).strong());
    node_details(ui, node, language);

    if let Some(totals) = well_totals {
        ui.separator();
        show_resource_well_totals(ui, totals, language);
    }

    ui.separator();
    ui.label(egui::RichText::new(text(language, "yield")).strong());
    match node.extraction_method {
        ExtractionMethod::Miner => {
            ui.label(text(language, "theoretical_yield"));
            egui::Grid::new(("node-yield", node.id.as_str()))
                .num_columns(3)
                .striped(true)
                .show(ui, |ui| {
                    ui.label(text(language, "miner"));
                    ui.label("100%");
                    ui.label(format!("{:.0}% max.", node.max_overclock * 100.0));
                    ui.end_row();

                    for miner_level in 1..=3 {
                        ui.label(format!("Mk.{}", miner_level));
                        ui.label(format!(
                            "{:.0} items/min",
                            scaled_yield(node, miner_base_rate(miner_level), 1.0)
                        ));
                        ui.label(format!(
                            "{:.0} items/min",
                            scaled_yield(node, miner_base_rate(miner_level), node.max_overclock)
                        ));
                        ui.end_row();
                    }
                });
        }
        ExtractionMethod::OilExtractor => {
            show_fluid_yield(ui, node, text(language, "oil_extractor"), 120.0, language);
        }
        ExtractionMethod::ResourceWellExtractor => {
            show_fluid_yield(ui, node, text(language, "well_extractor"), 60.0, language);
            ui.small(text(language, "rate_per_satellite"));
        }
        ExtractionMethod::ResourceWellCore => {
            ui.label(text(language, "well_core"));
            ui.small(text(language, "well_yield_hint"));
        }
        ExtractionMethod::ManualDeposit => {
            ui.label(text(language, "manual_deposit"));
        }
    }

    if node.capacity_per_minute > 0.0 {
        ui.separator();
        ui.label(format!(
            "{}: {:.0} / {:.0} {}",
            text(language, "your_entry"),
            node.used_per_minute,
            node.capacity_per_minute,
            text(language, "available_per_minute")
        ));
    }
}

fn show_resource_well_totals(ui: &mut egui::Ui, totals: ResourceWellTotals, language: Language) {
    let utilization = if totals.capacity_per_minute > 0.0 {
        (totals.used_per_minute / totals.capacity_per_minute).clamp(0.0, 1.0)
    } else {
        0.0
    };
    ui.label(egui::RichText::new(text(language, "satellites_total")).strong());
    ui.label(format!(
        "{}: {:.0} / {:.0} {}",
        text(language, "satellite_usage"),
        totals.used_per_minute,
        totals.capacity_per_minute,
        text(language, "available_per_minute")
    ));
    ui.label(format!(
        "{} {}",
        totals.satellite_count,
        text(language, "satellites_in_well")
    ));
    ui.add(egui::ProgressBar::new(utilization).show_percentage());
}

fn node_details(ui: &mut egui::Ui, node: &ResourceNode, language: Language) {
    ui.small(format!("{}  {}", text(language, "node_id"), node.id));
    ui.small(format!(
        "{}  X {:.0} · Y {:.0} · Z {:.0}",
        text(language, "world"),
        node.world_x,
        node.world_y,
        node.world_z
    ));

    let claimed = node.extractor_instance.is_some();
    ui.label(if claimed {
        text(language, "claimed")
    } else {
        text(language, "unclaimed")
    });

    if let Some(kind) = &node.extractor_kind {
        ui.label(format!("Extractor  {}", short_type_name(kind)));
        ui.label(format!(
            "{}  {} · Overclock  {:.0}% / max {:.0}%",
            text(language, "powershards_clock"),
            node.power_shards,
            node.current_overclock * 100.0,
            node.max_overclock * 100.0
        ));
    } else {
        ui.label(format!(
            "Extractor  {}",
            text(language, "extractor_unknown")
        ));
    }
    ui.label(format!(
        "{}  {}",
        text(language, "method"),
        extraction_method_label(node, language)
    ));
}

fn miner_base_rate(miner_level: u8) -> f32 {
    match miner_level {
        1 => 60.0,
        2 => 120.0,
        _ => 240.0,
    }
}

fn scaled_yield(node: &ResourceNode, base_rate: f32, overclock: f32) -> f32 {
    base_rate * purity_factor(node) * overclock
}

fn purity_factor(node: &ResourceNode) -> f32 {
    let purity_factor = match node.purity.as_str() {
        "Impure" => 0.5,
        "Pure" => 2.0,
        _ => 1.0,
    };
    purity_factor
}

fn show_fluid_yield(
    ui: &mut egui::Ui,
    node: &ResourceNode,
    machine: &str,
    normal_rate: f32,
    language: Language,
) {
    ui.label(format!(
        "{} {machine} {}",
        text(language, "theoretical"),
        text(language, "yield")
    ));
    egui::Grid::new(("fluid-yield", node.id.as_str()))
        .num_columns(3)
        .striped(true)
        .show(ui, |ui| {
            ui.label(machine);
            ui.label("100%");
            ui.label(format!("{:.0}% max.", node.max_overclock * 100.0));
            ui.end_row();
            ui.label(text(language, "output"));
            ui.label(format!(
                "{:.0} m³/min",
                scaled_yield(node, normal_rate, 1.0)
            ));
            ui.label(format!(
                "{:.0} m³/min",
                scaled_yield(node, normal_rate, node.max_overclock)
            ));
            ui.end_row();
        });
}

fn extraction_method_label(node: &ResourceNode, language: Language) -> &'static str {
    match node.extraction_method {
        ExtractionMethod::Miner => text(language, "miner"),
        ExtractionMethod::OilExtractor => text(language, "oil_extractor"),
        ExtractionMethod::ResourceWellExtractor => text(language, "well_extractor"),
        ExtractionMethod::ResourceWellCore => "Resource Well Pressurizer",
        ExtractionMethod::ManualDeposit => text(language, "manual_deposit"),
    }
}

fn summary_capacity_per_minute(node: &ResourceNode, miner_tier: u8) -> f32 {
    if node.capacity_per_minute.is_finite() && node.capacity_per_minute > 0.0 {
        return node.capacity_per_minute;
    }

    // An unclaimed node has no extractor actor to provide miner tier and
    // shard data. For the overview it still contributes its full theoretical
    // potential using the savegame-bound miner selection at 250%.
    let extractor_kind = match node.extraction_method {
        ExtractionMethod::Miner => {
            return scaled_yield(
                node,
                miner_base_rate(miner_tier),
                UNCLAIMED_DEFAULT_MAX_OVERCLOCK,
            )
        }
        ExtractionMethod::OilExtractor => "OilPump",
        ExtractionMethod::ResourceWellExtractor => "FrackingExtractor",
        ExtractionMethod::ResourceWellCore | ExtractionMethod::ManualDeposit => return 0.0,
    };
    default_capacity_per_minute(
        &node.purity,
        extractor_kind,
        UNCLAIMED_DEFAULT_MAX_OVERCLOCK,
    )
}

fn default_capacity_per_minute(purity: &str, extractor_kind: &str, max_overclock: f32) -> f32 {
    let normal_rate = if extractor_kind.contains("OilPump") {
        120.0
    } else if extractor_kind.contains("FrackingExtractor") {
        60.0
    } else if extractor_kind.contains("FrackingSmasher") {
        0.0
    } else if extractor_kind.contains("Mk3") {
        240.0
    } else if extractor_kind.contains("Mk2") {
        120.0
    } else {
        60.0
    };

    let purity_factor = match purity {
        "Impure" => 0.5,
        "Pure" => 2.0,
        _ => 1.0,
    };
    normal_rate * purity_factor * max_overclock
}

fn short_type_name(value: &str) -> &str {
    value.rsplit('/').next().unwrap_or(value)
}

#[cfg(test)]
mod tests {
    use super::{
        build_foundation_clusters, claimed_background_color, cubic_bezier, occupancy_color,
        project_world, rail_segments_length_meters, unproject_map, MapView,
    };
    use crate::save_parser::{
        parse_save_data, ParsedFoundation, ParsedRailPoint, ParsedRailSegment,
    };
    use crate::world_data::ExtractionMethod;
    use eframe::egui;
    use std::path::Path;

    #[test]
    fn larger_world_y_projects_lower_on_screen() {
        let lower = project_world(0.0, -100_000.0);
        let upper = project_world(0.0, 100_000.0);
        assert!(upper.y > lower.y);
    }

    #[test]
    fn world_grid_projection_round_trips_coordinates() {
        let world = egui::vec2(123_456.0, -98_765.0);
        let projected = project_world(world.x, world.y);
        let round_trip = unproject_map(projected);

        assert!((round_trip.x - world.x).abs() < 0.1);
        assert!((round_trip.y - world.y).abs() < 0.1);
    }

    #[test]
    fn cursor_world_coordinates_round_trip_through_map_view() {
        let map = MapView::default();
        let rect = egui::Rect::from_min_size(egui::pos2(20.0, 30.0), egui::vec2(900.0, 700.0));
        let world = egui::vec2(-42_000.0, 87_000.0);
        let screen = map.to_screen(rect, world.x, world.y);
        let round_trip = map.world_at_screen(rect, screen);

        assert!((round_trip.x - world.x).abs() < 0.1);
        assert!((round_trip.y - world.y).abs() < 0.1);
    }

    #[test]
    fn foundation_clusters_merge_neighbouring_areas() {
        let make_foundation = |min_x: f32| ParsedFoundation {
            corners: [
                [min_x, 0.0, 0.0],
                [min_x + 8_000.0, 0.0, 0.0],
                [min_x + 8_000.0, 8_000.0, 0.0],
                [min_x, 8_000.0, 0.0],
            ],
        };
        let foundations = [make_foundation(0.0), make_foundation(8_000.0)];
        let clusters = build_foundation_clusters(&foundations);

        assert_eq!(clusters.len(), 1);
        assert_eq!(clusters[0].contours[0].len(), 4);
        assert_eq!(clusters[0].min_x, 0.0);
        assert_eq!(clusters[0].max_x, 16_000.0);
    }

    #[test]
    fn foundation_clusters_keep_separate_areas_separate() {
        let make_foundation = |min_x: f32| ParsedFoundation {
            corners: [
                [min_x, 0.0, 0.0],
                [min_x + 1_000.0, 0.0, 0.0],
                [min_x + 1_000.0, 1_000.0, 0.0],
                [min_x, 1_000.0, 0.0],
            ],
        };
        let clusters = build_foundation_clusters(&[make_foundation(0.0), make_foundation(5_000.0)]);

        assert_eq!(clusters.len(), 2);
    }

    #[test]
    fn spline_lengths_are_reported_in_meters() {
        let segment = ParsedRailSegment {
            points: vec![
                ParsedRailPoint {
                    location: [0.0, 0.0, 0.0],
                    arrive_tangent: [0.0, 0.0, 0.0],
                    leave_tangent: [0.0, 0.0, 0.0],
                },
                ParsedRailPoint {
                    location: [100.0, 0.0, 0.0],
                    arrive_tangent: [0.0, 0.0, 0.0],
                    leave_tangent: [0.0, 0.0, 0.0],
                },
            ],
        };
        assert!((rail_segments_length_meters(&[segment]) - 1.0).abs() < 0.001);
    }

    #[test]
    fn resource_well_curve_keeps_node_endpoints() {
        let start = egui::pos2(10.0, 20.0);
        let control_one = egui::pos2(30.0, 0.0);
        let control_two = egui::pos2(70.0, 0.0);
        let end = egui::pos2(90.0, 20.0);

        assert_eq!(
            cubic_bezier(start, control_one, control_two, end, 0.0),
            start
        );
        assert_eq!(cubic_bezier(start, control_one, control_two, end, 1.0), end);
    }

    #[test]
    fn occupancy_ring_uses_requested_colors() {
        assert_eq!(
            occupancy_color(false),
            egui::Color32::from_rgb(0xB9, 0xC2, 0xC6)
        );
        assert_eq!(
            occupancy_color(true),
            egui::Color32::from_rgb(0x08, 0xC4, 0x62)
        );
        assert_eq!(
            claimed_background_color(false),
            egui::Color32::from_rgb(0x7A, 0x88, 0x95)
        );
        assert_eq!(
            claimed_background_color(true),
            egui::Color32::from_rgb(0x34, 0x3F, 0x49)
        );
    }

    #[test]
    fn matches_oil_and_resource_well_extractors_when_present() {
        let path = Path::new(
            r#"C:\Users\nuua\AppData\Local\FactoryGame\Saved\SaveGames\76561198862437134\ Nuua_autosave_0.sav"#,
        );
        if !path.exists() {
            return;
        }

        let save_data = parse_save_data(path).expect("attached save should parse");
        let mut map = MapView::default();
        map.apply_extractors(&save_data.extractors);

        assert!(map
            .nodes
            .iter()
            .any(|node| { node.resource == "Crude Oil" && node.extractor_instance.is_some() }));
        assert!(map.nodes.iter().any(|node| {
            node.extraction_method == ExtractionMethod::ResourceWellExtractor
                && node.extractor_instance.is_some()
        }));
    }
}
