use egui::{Ui, Color32};

pub fn info_box(ui: &mut Ui, text: &str)
{
    let bg_color = Color32::from_white_alpha(5);
    egui::Frame::group(ui.style()).fill(bg_color).show(ui, |ui|
    {
        ui.horizontal_wrapped(|ui|
        {
            ui.vertical(|ui|
            {
                ui.label(egui::RichText::new("ℹ").strong().color(Color32::WHITE).size(20.0));
            });
            ui.vertical(|ui|
            {
                ui.label(egui::RichText::new(text).strong());
            });
        });
    });
}

pub fn info_box_with_body<R>(ui: &mut Ui, body: impl FnOnce(&mut Ui) -> R)
{
    let bg_color = Color32::from_white_alpha(5);
    egui::Frame::group(ui.style()).fill(bg_color).show(ui, |ui|
    {
        ui.horizontal_wrapped(|ui|
        {
            ui.vertical(|ui|
            {
                ui.label(egui::RichText::new("ℹ").strong().color(Color32::WHITE).size(20.0));
            });
            ui.vertical(|ui|
            {
                body(ui);
            });
        });
    });
}

pub fn success_box(ui: &mut Ui, text: &str)
{
    let bg_color = Color32::from_rgba_premultiplied(51, 165, 76, 5);
    egui::Frame::group(ui.style()).fill(bg_color).show(ui, |ui|
    {
        ui.horizontal_wrapped(|ui|
        {
            ui.vertical(|ui|
            {
                ui.label(egui::RichText::new("✔").strong().color(Color32::WHITE).size(20.0));
            });
            ui.vertical(|ui|
            {
                ui.label(egui::RichText::new(text).strong().color(Color32::WHITE));
            });
        });
    });
}

pub fn error_box(ui: &mut Ui, text: &str)
{
    let bg_color = Color32::from_rgba_premultiplied(175, 5, 34, 5);
    egui::Frame::group(ui.style()).fill(bg_color).show(ui, |ui|
    {
        ui.horizontal_wrapped(|ui|
        {
            ui.vertical(|ui|
            {
                ui.label(egui::RichText::new("❌").strong().color(Color32::WHITE).size(20.0));
            });
            ui.vertical(|ui|
            {
                ui.label(egui::RichText::new(text).strong().color(Color32::WHITE));
            });
        });
    });
}

pub fn warn_box(ui: &mut Ui, text: &str)
{
    let bg_color = Color32::from_rgba_premultiplied(255, 129, 36, 5);
    egui::Frame::group(ui.style()).fill(bg_color).show(ui, |ui|
    {
        ui.horizontal_wrapped(|ui|
        {
            ui.vertical(|ui|
            {
                ui.label(egui::RichText::new("❗").strong().color(Color32::WHITE).size(20.0));
            });
            ui.vertical(|ui|
            {
                ui.label(egui::RichText::new(text).strong().color(Color32::WHITE));
            });
        });
    });
}