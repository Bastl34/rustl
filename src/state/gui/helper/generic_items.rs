use egui::{Ui, Color32, RichText, Align2};

pub fn collapse<R>(ui: &mut Ui, id: String, open: bool, header: impl FnOnce(&mut Ui) -> R, body: impl FnOnce(&mut Ui) -> R)
{
    let bg_color = Color32::from_white_alpha(3);

    let mut frame = egui::Frame::group(ui.style()).fill(bg_color);
    frame.inner_margin = egui::Margin::same(2.0);

    frame.show(ui, |ui|
    {
        let ui_id = ui.make_persistent_id(id);
        egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), ui_id, open).show_header(ui, |ui|
        {
            ui.horizontal(|ui|
            {
                header(ui);
            });
        }).body(|ui|
        {
            ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), body);
        });
    });
}

pub fn collapse_with_title<R>(ui: &mut Ui, id: &str, open: bool, title: &str, body: impl FnOnce(&mut Ui) -> R)
{
    collapse(ui, id.to_string(), open, |ui|
    {
        ui.label(RichText::new(title).heading().strong());

        // this is just to use the full with
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui|
        {
            ui.label("");
        });
    },
    |ui|
    {
        body(ui);
    });
}

pub fn modal_with_title<R>(ctx: &egui::Context, open: &mut bool, title: &str, body: impl FnOnce(&mut Ui) -> R)
{
    egui::Window::new(title)
    .anchor(Align2::CENTER_CENTER, egui::Vec2::new(0.0, 0.0))
    .collapsible(false)
    .open(open)
    .show(ctx, body);
}