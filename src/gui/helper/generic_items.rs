use egui::{Color32, Frame, RichText, Ui};

pub const TAB_CORNER_RADIUS: egui::CornerRadius = egui::CornerRadius { nw: 3, ne: 3, sw: 0, se: 0 };

pub const TAB_BG_SELECTED: Color32 = Color32::from_rgba_premultiplied(60, 60, 60, 60);
pub const TAB_BG_HOVER: Color32        = Color32::from_rgba_premultiplied(22, 22, 22, 22);
pub const TAB_BG_INACTIVE: Color32     = Color32::from_rgba_premultiplied(8, 8, 8, 8);

pub struct TabResponse
{
    pub response: egui::Response,
    pub clicked: bool,
    pub close_clicked: bool,
}

pub fn tab_separator(ui: &mut Ui)
{
    let prev_spacing = ui.spacing().item_spacing.y;
    ui.spacing_mut().item_spacing.y = 0.0;
    ui.add(egui::Separator::default().spacing(2.0));
    ui.spacing_mut().item_spacing.y = prev_spacing;
}

pub fn tab(ui: &mut Ui, label: impl Into<egui::WidgetText>, selected: bool, closable: bool) -> TabResponse
{
    let h_pad = 10.0;
    let v_pad = 5.0;
    let gap = 6.0;

    let label_galley = label.into().into_galley(ui, Some(egui::TextWrapMode::Extend), f32::INFINITY, egui::TextStyle::Button);

    let close_galley = if closable
    {
        Some(ui.painter().layout_no_wrap("🗙".to_string(), egui::FontId::proportional(11.0), ui.visuals().text_color()))
    }
    else
    {
        None
    };

    let mut tab_w = h_pad + label_galley.size().x + h_pad;
    if let Some(close) = &close_galley
    {
        tab_w += gap + close.size().x;
    }
    let tab_h = label_galley.size().y + v_pad * 2.0;

    let (tab_rect, tab_response) = ui.allocate_exact_size(egui::vec2(tab_w, tab_h), egui::Sense::click());

    let bg_color = if selected
    {
        TAB_BG_SELECTED
    }
    else if tab_response.hovered()
    {
        TAB_BG_HOVER
    }
    else
    {
        TAB_BG_INACTIVE
    };
    ui.painter().rect_filled(tab_rect, TAB_CORNER_RADIUS, bg_color);

    // label, vertically centered
    let label_pos = egui::pos2(tab_rect.left() + h_pad, tab_rect.center().y - label_galley.size().y / 2.0);
    ui.painter().galley(label_pos, label_galley, ui.visuals().text_color());

    // close button
    let mut close_clicked = false;
    if let Some(close_galley) = close_galley
    {
        let close_x = tab_rect.right() - h_pad - close_galley.size().x;
        let close_y = tab_rect.center().y - close_galley.size().y / 2.0;
        let close_rect = egui::Rect::from_min_size
        (
            egui::pos2(close_x - 3.0, close_y - 2.0),
            egui::vec2(close_galley.size().x + 6.0, close_galley.size().y + 4.0),
        );
        let close_response = ui.allocate_rect(close_rect, egui::Sense::click());

        let close_color = if close_response.hovered() { Color32::WHITE } else { ui.visuals().weak_text_color() };
        if close_response.hovered()
        {
            ui.painter().rect_filled(close_rect, 3.0, Color32::from_rgba_unmultiplied(180, 50, 50, 200));
        }
        ui.painter().galley(egui::pos2(close_x, close_y), close_galley, close_color);

        close_clicked = close_response.clicked();
    }

    let clicked = tab_response.clicked() && !close_clicked;

    TabResponse { response: tab_response, clicked, close_clicked }
}

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
            let header_bg_color = TAB_BG_SELECTED;
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
            ui.painter().set(header_bg_idx, egui::Shape::rect_filled(header_rect, TAB_CORNER_RADIUS, header_bg_color));

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
