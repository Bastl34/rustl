use egui::{Ui, Color32};

pub fn collapse<R>(ui: &mut Ui, id: u64, open: bool, header: impl FnOnce(&mut Ui) -> R, body: impl FnOnce(&mut Ui) -> R)
{
    let bg_color = Color32::from_white_alpha(3);

    let mut frame = egui::Frame::group(ui.style()).fill(bg_color);
    frame.inner_margin = egui::Margin::same(2.0);

    frame.show(ui, |ui|
    {
        let ui_id = ui.make_persistent_id(id.clone());
        egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), ui_id, open).show_header(ui, |ui|
        {
            header(ui);
        }).body(|ui|
        {
            ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), body);
        });
    });
}