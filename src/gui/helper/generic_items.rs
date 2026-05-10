use egui::{Color32, Frame, RichText, Ui};

pub fn collapse<R>(ui: &mut Ui, id: String, open: bool, bg_color: Option<Color32>, header: impl FnOnce(&mut Ui) -> R, body: impl FnOnce(&mut Ui) -> R)
{
    let background_color;
    if let Some(color) = bg_color
    {
        background_color = color;
    }
    else
    {
        background_color = Color32::from_white_alpha(0);
    }

    let mut frame = egui::Frame::group(ui.style()).fill(background_color).stroke(egui::Stroke::NONE);
    //let mut frame = egui::Frame::group(ui.style()).fill(background_color);
    // horizontal margin keeps content off the edges; vertical margin is 0 so the
    // header bg can sit flush with the frame's top/bottom (no transparent strip showing through)
    frame.inner_margin = egui::Margin::symmetric(2, 0);
    frame = frame.shadow(egui::Shadow
    {
        color: Color32::from_white_alpha(35),
        offset: [0, 0],
        blur: 5,
        spread: 0,
    });

    frame.show(ui, |ui|
    {
        ui.scope(|ui|
        {
            ui.style_mut().visuals.indent_has_left_vline = false;

            // reserve a shape slot to paint the header background underneath the header content
            let header_bg_idx = ui.painter().add(egui::Shape::Noop);
            let header_bg_color = Color32::from_white_alpha(40);
            let pading = 5.0;

            let cursor_top = ui.cursor().min.y;
            ui.add_space(pading);

            let ui_id = ui.make_persistent_id(id.clone());
            let (_toggle_resp, header_inner, body_resp) = egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), ui_id, open).show_header(ui, |ui|
            {
                ui.horizontal(|ui|
                {
                    header(ui);
                });
            }).body(|ui|
            {
                ui.add_space(pading * 2.0);

                ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), body);

                ui.add_space(pading);
            });

            // expand the header bg to the full inner width and pad it vertically
            let mut header_rect = header_inner.response.rect;
            header_rect.min.x = ui.max_rect().left();
            header_rect.max.x = ui.max_rect().right();
            header_rect.min.y = cursor_top;
            header_rect.max.y += pading;
            ui.painter().set(header_bg_idx, egui::Shape::rect_filled(header_rect, 3.0, header_bg_color));

            // when closed, push the cursor past the bg extension so siblings don't overlap
            if body_resp.is_none()
            {
                ui.add_space(pading);
            }
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

pub fn modal_with_title<R>(ctx: &egui::Context, open: &mut bool, title: &str, movable: bool, resizable: bool, body: impl FnOnce(&mut Ui) -> R)
{
    egui::Window::new(title)
        .pivot(egui::Align2::CENTER_CENTER)
        .default_pos(ctx.content_rect().center())
        .collapsible(false)
        .resizable(resizable)
        .movable(movable)
        .open(open)
        .show(ctx, body);
}

pub fn separator_colored(ui: &mut Ui, color: Color32, height: f32)
{
    let available_width = ui.available_width();

    let (rect, _response) = ui.allocate_exact_size(egui::vec2(available_width, height), egui::Sense::hover());

    let painter = ui.painter();
    painter.rect_filled(rect, 0.0, color);
}

pub fn label_with_background(ui: &mut Ui, text: &str, bg_color: Color32, text_color: Option<Color32>)
{
    Frame::new().fill(bg_color).corner_radius(2.0).inner_margin(egui::Margin::symmetric(8, 4)).show(ui, |ui|
    {
        if let Some(color) = text_color
        {
            ui.label(RichText::new(text).strong().color(color));
            return;
        }
        ui.label(RichText::new(text).strong().color(Color32::WHITE));
    });
}

pub fn button_with_background(ui: &mut Ui, text: &str, bg_color: Color32, text_color: Option<Color32>) -> egui::Response
{
    let rich_text = if let Some(color) = text_color
    {
        RichText::new(text).strong().color(color)
    }
    else
    {
        RichText::new(text).strong().color(Color32::WHITE)
    };
    ui.add(egui::Button::new(rich_text).fill(bg_color))
}
