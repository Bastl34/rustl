use egui::{Ui, Color32, RichText, Align2, Id};

pub fn collapse<R>(ui: &mut Ui, id: String, open: bool, bg_color: Option<Color32>, header: impl FnOnce(&mut Ui) -> R, body: impl FnOnce(&mut Ui) -> R)
{

    let background_color;
    if let Some(color) = bg_color
    {
        background_color = color;
    }
    else
    {
        background_color = Color32::from_white_alpha(3);
    }

    let mut frame = egui::Frame::group(ui.style()).fill(background_color);
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

pub fn collapse_with_title<R>(ui: &mut Ui, id: &str, open: bool, title: &str, bg_color: Option<Color32>, body: impl FnOnce(&mut Ui) -> R)
{
    collapse(ui, id.to_string(), open, bg_color, |ui|
    {
        ui.label(RichText::new(title).heading().strong());

        // this is just to use the full width
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

pub fn drag_item(ui: &mut Ui, id: Id, body: impl FnOnce(&mut Ui))
{
    let is_being_dragged = ui.memory(|mem| mem.is_being_dragged(id));

    if !is_being_dragged
    {
        let response = ui.scope(body).response;

        // Check for drags:
        let response = ui.interact(response.rect, id, egui::Sense::drag());
        if response.hovered()
        {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
        }
    }
    else
    {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);

        let layer_id = egui::LayerId::new(egui::Order::Tooltip, id);
        let response = ui.with_layer_id(layer_id, body).response;

        if let Some(pointer_pos) = ui.ctx().pointer_interact_pos()
        {
            let delta = pointer_pos - response.rect.center();
            ui.ctx().translate_layer(layer_id, delta);
        }
    }
}

/*
pub fn enable_drag(ui: &mut Ui, response: &egui::Response, id: Id)
{
    let is_being_dragged = ui.memory(|mem| mem.is_being_dragged(id));

    if !is_being_dragged
    {
        // Check for drags:
        let response = ui.interact(response.rect, id, egui::Sense::drag());
        if response.hovered()
        {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
        }
    }
    else
    {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);

        let layer_id = egui::LayerId::new(egui::Order::Tooltip, id);

        if let Some(pointer_pos) = ui.ctx().pointer_interact_pos()
        {
            let delta = pointer_pos - response.rect.center();
            ui.ctx().translate_layer(layer_id, delta);
        }
    }
}
*/