pub fn fit_size(availiable_size: egui::Vec2, requested_size: egui::Vec2) -> egui::Vec2
{
    if requested_size.x <= 0.0 || requested_size.y <= 0.0
    {
        return egui::Vec2::ZERO;
    }
    let scale = (availiable_size.x / requested_size.x).min(availiable_size.y / requested_size.y);
    egui::vec2(requested_size.x * scale, requested_size.y * scale)
}

