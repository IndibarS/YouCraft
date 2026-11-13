//! Floating player layout, gestures, and bounded resizing.

use super::{FilmcraftApp, display_name, picture_gestures, playback_feedback, seek, seek_bar_height, seek_hover_preview, timestamp, toggle, video_aspect};
use crate::{
    icons::{self, Icon},
    theme::Tokens,
};
use egui::{Align2, Color32, Rect, Sense, pos2, vec2};
use filmcraft_time::Tick;

fn mini_button(app: &mut FilmcraftApp, ui: &mut egui::Ui, rect: Rect, id: &str, icon: Icon, label: &str) -> bool {
    let response = ui.interact(rect, egui::Id::unique(id), Sense::click());
    ui.painter().circle_filled(rect.center(), rect.width() * 0.5, Color32::from_black_alpha(if response.hovered() { 180 } else { 95 }));
    icons::paint(ui.painter(), rect.shrink(rect.width() * 0.22), icon, Color32::WHITE);
    app.auto.add(id, rect, label);
    response.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(label).clicked()
}

fn mini_restore(app: &mut FilmcraftApp) {
    app.ui.player.miniplayer = false;
    app.ui.player.page = "watch".into();
}

fn mini_picture(app: &mut FilmcraftApp, ui: &mut egui::Ui, rect: Rect, duration: Tick, hovered: bool) {
    crate::panels::monitor::player_picture(app, ui, rect);
    app.auto.add("player.mini.picture", rect, "Miniplayer video");
    playback_feedback(app, ui, rect);
    if !hovered {
        return;
    }
    {
        ui.painter().rect_filled(rect, 0.0, Color32::from_black_alpha(45));
        let center = Rect::from_center_size(rect.center(), vec2(62.0, 62.0));
        let playing = app.source_playback.clock.playing;
        if mini_button(app, ui, center, "player.play", if playing { Icon::Pause } else { Icon::Play }, if playing { "Pause" } else { "Play" }) {
            toggle(app);
        }
    }
    let restore_inset = if app.player.mini_corner == [true, true] { 22.0 } else { 8.0 };
    let restore = Rect::from_min_size(rect.min + vec2(restore_inset, 8.0), vec2(32.0, 32.0));
    if mini_button(app, ui, restore, "player.mini.restore", Icon::Proxy, "Return to video (I)") {
        mini_restore(app);
    }
    let close_inset = if app.player.mini_corner == [false, true] { 54.0 } else { 40.0 };
    let close = Rect::from_min_size(pos2(rect.right() - close_inset, rect.top() + 8.0), vec2(32.0, 32.0));
    if mini_button(app, ui, close, "player.mini.close", Icon::Close, "Close miniplayer") {
        app.stop_source();
        app.ui.player.miniplayer = false;
        app.player.seek_preview = None;
    }
    let label_rect = Rect::from_min_size(pos2(rect.left() + 10.0, rect.bottom() - 30.0), vec2(130.0, 24.0));
    ui.painter().rect_filled(label_rect, 4.0, Color32::from_black_alpha(130));
    ui.painter().text(
        label_rect.center(),
        Align2::CENTER_CENTER,
        format!("{} / {}", timestamp(app.session.state.source_playhead), timestamp(duration)),
        Tokens::ui(12.0),
        Color32::WHITE,
    );
}

fn mini_drag(app: &mut FilmcraftApp, ctx: &egui::Context, response: &egui::Response, card: Rect) {
    if response.dragged() {
        app.player.pending_picture_click = None;
        let screen = ctx.content_rect();
        let pos = app.player.mini_drag_pos.get_or_insert(card.min);
        *pos += response.drag_delta();
        pos.x = pos.x.clamp(screen.left(), (screen.right() - card.width()).max(screen.left()));
        pos.y = pos.y.clamp(screen.top(), (screen.bottom() - card.height()).max(screen.top()));
        ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
        ctx.request_repaint();
    }
    if response.drag_stopped() {
        let center = app.player.mini_drag_pos.unwrap_or(card.min) + card.size() * 0.5;
        let screen = ctx.content_rect();
        app.player.mini_corner = [center.x >= screen.center().x, center.y >= screen.center().y];
        app.player.mini_drag_pos = None;
        ctx.request_repaint();
    }
}

pub(super) fn show(app: &mut FilmcraftApp, ctx: &egui::Context) {
    let Some(id) = app.session.state.source_item else { return };
    let Some(view) = filmcraft_engine::clip_ops::source_view(&app.session, id) else { return };
    let title = app.session.project.item(id).map(|item| display_name(&item.name)).unwrap_or_default();
    let screen = ctx.content_rect();
    let aspect = video_aspect(app, id).max(0.1);
    let chrome = 80.0;
    let max_width = (screen.width() - 32.0).max(1.0).min((screen.height() - chrome - 32.0).max(1.0) * aspect);
    let min_width = 240.0_f32.min(max_width);
    let preferred = app.ui.player.mini_width.filter(|width| width.is_finite()).unwrap_or_else(|| (screen.width() * 0.28).clamp(280.0, 480.0));
    let width = preferred.clamp(min_width, max_width);
    let height = width / aspect;
    let size = vec2(width, height + chrome);
    let corner = app.player.mini_corner;
    let position = app.player.mini_drag_pos.unwrap_or_else(|| {
        pos2(
            if corner[0] { (screen.right() - width - 16.0).max(screen.left()) } else { screen.left() + 16.0 },
            if corner[1] { (screen.bottom() - size.y - 16.0).max(screen.top()) } else { screen.top() + 16.0 },
        )
    });
    let hovered = ctx.pointer_latest_pos().is_some_and(|p| Rect::from_min_size(position, size).contains(p)) || app.player.mini_drag_pos.is_some();
    let output = egui::Area::new(egui::Id::unique("youcraft-miniplayer")).fixed_pos(position).movable(false).order(egui::Order::Foreground).show(ctx, |ui| {
        let frame = egui::Frame::new().fill(Color32::from_gray(30)).corner_radius(12).inner_margin(0).show(ui, |ui| {
            ui.set_width(width);
            ui.spacing_mut().item_spacing.y = 0.0;
            let (rect, picture_response) = ui.allocate_exact_size(vec2(width, height), Sense::click_and_drag());
            if picture_response.dragged() {
                app.player.pending_picture_click = None;
            } else if !picture_response.drag_stopped() {
                picture_gestures(app, ctx, &picture_response, rect);
            }
            picture_response.clone().on_hover_cursor(egui::CursorIcon::Grab);
            mini_picture(app, ui, rect, view.end, hovered);
            let mut seconds = app.player.seek_preview.unwrap_or_else(|| app.session.state.source_playhead.seconds());
            let response = seek_bar_height(ui, &mut seconds, view.start.seconds(), view.end.seconds(), 12.0);
            app.auto.add("player.mini.seek", response.rect, "Seek miniplayer video");
            if response.dragged() {
                app.player.seek_preview = Some(seconds);
            }
            if response.drag_stopped() || (response.changed() && !response.dragged()) {
                app.player.seek_preview = None;
                seek(app, Tick::from_seconds_f64(seconds));
            }
            seek_hover_preview(app, ui, id, (view.start.seconds(), view.end.seconds()), rect, &response);
            let title_response = egui::Frame::new()
                .inner_margin(12)
                .show(ui, |ui| {
                    ui.set_width((width - 24.0).max(1.0));
                    let title = ui.add(egui::Label::new(egui::RichText::new(&title).strong().size(15.0)).truncate().sense(Sense::click_and_drag()));
                    app.auto.add("player.mini.title", title.rect, "Return to video");
                    if title.clicked() {
                        mini_restore(app);
                    }
                    ui.add_space(5.0);
                    ui.label(egui::RichText::new("Local video").size(12.0).color(app.tokens.text_dim));
                    title.on_hover_cursor(egui::CursorIcon::Grab)
                })
                .inner;
            (picture_response, title_response)
        });
        let card = frame.response.rect;
        let origin = pos2(if corner[0] { card.left() } else { card.right() }, if corner[1] { card.top() } else { card.bottom() });
        let inward = vec2(if corner[0] { 1.0 } else { -1.0 }, if corner[1] { 1.0 } else { -1.0 });
        // Edge bands sit inside the card so they remain reachable at window boundaries.
        // Reserve the shared corner for diagonal resizing; all other edge points resize on one axis.
        let corner_size = 20.0_f32.min(card.width() * 0.5).min(card.height() * 0.5);
        let band = 7.0;
        let vertical = Rect::from_min_max(
            pos2(if corner[0] { card.left() } else { card.right() - band }, card.top() + if corner[1] { corner_size } else { 0.0 }),
            pos2(if corner[0] { card.left() + band } else { card.right() }, card.bottom() - if corner[1] { 0.0 } else { corner_size }),
        );
        let horizontal = Rect::from_min_max(
            pos2(card.left() + if corner[0] { corner_size } else { 0.0 }, if corner[1] { card.top() } else { card.bottom() - band }),
            pos2(card.right() - if corner[0] { 0.0 } else { corner_size }, if corner[1] { card.top() + band } else { card.bottom() }),
        );
        let diagonal = if corner[0] == corner[1] { egui::CursorIcon::ResizeNwSe } else { egui::CursorIcon::ResizeNeSw };
        for (id, target, cursor, axis) in [
            ("player.mini.resize.horizontal", vertical, egui::CursorIcon::ResizeHorizontal, 0),
            ("player.mini.resize.vertical", horizontal, egui::CursorIcon::ResizeVertical, 1),
            ("player.mini.resize", Rect::from_two_pos(origin, origin + inward * corner_size), diagonal, 2),
        ] {
            let response = ui.interact(target, egui::Id::unique(id), Sense::drag());
            app.auto.add(id, target, "Resize miniplayer");
            response.clone().on_hover_cursor(cursor);
            if response.hovered() || response.dragged() {
                ctx.set_cursor_icon(cursor);
            }
            if response.dragged() {
                app.player.pending_picture_click = None;
                let delta = response.drag_delta();
                let growth = match axis {
                    0 => -delta.x * inward.x,
                    1 => -delta.y * inward.y * aspect,
                    _ => (-delta.x * inward.x - delta.y * inward.y / aspect) / (1.0 + 1.0 / (aspect * aspect)),
                };
                app.ui.player.mini_width = Some((width + growth).clamp(min_width, max_width));
                ctx.request_repaint();
            }
        }
        frame
    });
    let card = output.response.rect;
    app.auto.add("player.mini.card", card, "Miniplayer — drag to a corner");
    mini_drag(app, ctx, &output.inner.inner.0, card);
    mini_drag(app, ctx, &output.inner.inner.1, card);
}
