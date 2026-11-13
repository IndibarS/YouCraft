//! Plain-text fork identity; no ArtCraft trademark artwork is embedded.
pub fn paint_wordmark(ui: &egui::Ui, left_center: egui::Pos2, height: f32) -> egui::Rect {
    ui.painter().text(left_center, egui::Align2::LEFT_CENTER, "YouCraft", crate::theme::Tokens::semibold(height), ui.visuals().text_color())
}
