//! Fixed fullscreen picture with a scrolling transport and related-video section.

use super::{FilmcraftApp, media_ids, video_grid_rows};
use egui::{Align2, Color32, Rect, Sense, pos2, vec2};

pub(super) fn controls_opacity(offset: f32, height: f32) -> f32 {
    (1.0 - offset / (height * 0.7).max(1.0)).clamp(0.0, 1.0)
}

fn toggle(app: &mut FilmcraftApp, ui: &mut egui::Ui, expanded: bool) {
    let (rect, response) = ui.allocate_exact_size(vec2(if expanded { 156.0 } else { 40.0 }, 40.0), Sense::click());
    ui.painter().rect_filled(rect, 20.0, Color32::from_black_alpha(if response.hovered() { 240 } else { 185 }));
    let center = if expanded { pos2(rect.left() + 22.0, rect.center().y) } else { rect.center() };
    super::icons::paint(
        ui.painter(),
        Rect::from_center_size(center, vec2(18.0, 18.0)),
        if expanded { super::Icon::ChevronDown } else { super::Icon::ArrowUp },
        Color32::WHITE,
    );
    let label = if expanded { "Hide videos" } else { "More videos" };
    if expanded {
        ui.painter().text(pos2(rect.left() + 40.0, rect.center().y), Align2::LEFT_CENTER, label, super::Tokens::ui(14.0), Color32::WHITE);
    }
    app.auto.add(if expanded { "player.fullscreen.more.close" } else { "player.fullscreen.more" }, rect, label);
    if response.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text("More videos (V); Escape closes the list").clicked() {
        app.player.fullscreen_scroll_target = Some(!expanded);
        app.player.pending_picture_click = None;
    }
}

pub(super) fn more_button(app: &mut FilmcraftApp, ui: &mut egui::Ui) {
    let Some(seek) = app.auto.find("player.seek").map(|element| element.rect) else { return };
    let rect = Rect::from_min_size(pos2(seek[0] + seek[2] * 0.5 - 20.0, seek[1] + seek[3] + 8.0), vec2(40.0, 40.0));
    ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| toggle(app, ui, false));
}

pub(super) fn show(app: &mut FilmcraftApp, ui: &mut egui::Ui) {
    let size = ui.available_size();
    let now = app.now();
    let scrolling = ui.input(|i| i.smooth_scroll_delta.y.abs() > 0.1);
    if scrolling {
        app.player.fullscreen_scroll_idle_since = Some(now);
    } else if let Some(since) = app.player.fullscreen_scroll_idle_since {
        if should_snap_back(app.player.fullscreen_scroll, size.y) && !ui.input(|i| i.pointer.any_down()) {
            if now - since >= 0.2 {
                app.player.fullscreen_scroll_target = Some(false);
                app.player.fullscreen_scroll_idle_since = None;
            } else {
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(50));
            }
        } else {
            app.player.fullscreen_scroll_idle_since = None;
        }
    }
    let source = app.session.state.source_item;
    let request = app.player.fullscreen_scroll_target.take();
    let output = egui::ScrollArea::vertical()
        .id_salt(("fullscreen-page", source))
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
        .auto_shrink([false, false])
        .show_viewport(ui, |ui, viewport| {
            app.player.fullscreen_scroll = viewport.min.y;
            let origin = ui.cursor().min;
            let (picture_page, _) = ui.allocate_exact_size(size, Sense::hover());
            ui.scope_builder(egui::UiBuilder::new().max_rect(picture_page), |ui| super::watch(app, ui));
            let list_background = Rect::from_min_max(pos2(picture_page.left(), picture_page.bottom()), ui.clip_rect().max);
            if list_background.is_positive() {
                ui.painter().rect_filled(list_background, 0.0, Color32::from_black_alpha(110));
            }
            ui.add_space(16.0);
            ui.horizontal(|ui| {
                ui.add_space((ui.available_width() - 156.0).max(0.0) * 0.5);
                toggle(app, ui, true);
            });
            ui.add_space(16.0);
            let ids: Vec<_> = media_ids(app).into_iter().filter(|id| Some(*id) != source).collect();
            if ids.is_empty() {
                ui.label("No other videos in your library. Open more videos from the library view.");
            } else {
                grid(app, ui, &ids);
            }
            if let Some(expanded) = request {
                let target = origin + vec2(0.0, if expanded { size.y * 0.75 } else { 0.0 });
                ui.scroll_to_rect(Rect::from_min_size(target, vec2(1.0, 1.0)), Some(egui::Align::TOP));
            }
        });
    app.player.fullscreen_scroll = output.state.clamped_offset().y;
}

fn should_snap_back(offset: f32, height: f32) -> bool {
    offset > 0.0 && offset < (height * 0.08).min(64.0)
}

fn grid(app: &mut FilmcraftApp, ui: &mut egui::Ui, ids: &[filmcraft_project::ItemId]) {
    let columns = ((ui.available_width() / 380.0).floor() as usize).clamp(1, 3);
    let width = ((ui.available_width() - 24.0 * (columns - 1) as f32 - 14.0) / columns as f32).max(20.0);
    let row_height = width * 0.5625 + 108.0;
    let stride = row_height + ui.spacing().item_spacing.y;
    let rows = ids.len().div_ceil(columns);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), rows as f32 * stride), Sense::hover());
    let clip = rect.intersect(ui.clip_rect());
    if !clip.is_positive() {
        return;
    }
    let first = ((clip.top() - rect.top()).max(0.0) / stride).floor() as usize;
    let end = (((clip.bottom() - rect.top()).max(0.0) / stride).ceil() as usize).min(rows);
    let visible = Rect::from_min_max(pos2(rect.left(), rect.top() + first as f32 * stride), rect.max);
    ui.scope_builder(egui::UiBuilder::new().max_rect(visible), |ui| video_grid_rows(app, ui, ids, columns, width, row_height, first..end));
    app.auto.add("player.fullscreen.more.grid", clip, "More local videos");
}

#[cfg(test)]
mod tests {
    use super::controls_opacity;

    #[test]
    fn transport_fades_with_scroll_and_recovers_on_return() {
        let values: Vec<_> = [0.0, 100.0, 300.0, 630.0, 900.0].map(|offset| controls_opacity(offset, 900.0)).into();
        assert_eq!(values[0], 1.0);
        assert!(values.windows(2).all(|pair| pair[0] >= pair[1]));
        assert!(values[1] > values[2] && values[2] > 0.0);
        assert_eq!(values[3], 0.0);
        assert_eq!(controls_opacity(0.0, 900.0), 1.0);
    }
}
