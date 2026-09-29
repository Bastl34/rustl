use std::{hash::Hash, ops::RangeInclusive};

use egui::{emath::Numeric, Ui};
use nalgebra::Vector3;
use strum::IntoEnumIterator;

// one property row: "Label: ℹ <widget>" - an empty label or info is left out
pub fn property_row<R>(ui: &mut Ui, label: &str, info: &str, add: impl FnOnce(&mut Ui) -> R) -> R
{
    ui.horizontal(|ui|
    {
        if !label.is_empty()
        {
            ui.label(format!("{}: ", label));
        }

        if !info.is_empty()
        {
            ui.label("ℹ").on_hover_text(info);
        }

        add(ui)
    }).inner
}

pub fn slider<Num: Numeric>(ui: &mut Ui, label: &str, info: &str, value: &mut Num, range: RangeInclusive<Num>, decimals: usize) -> bool
{
    slider_widget(ui, label, info, egui::Slider::new(value, range).fixed_decimals(decimals))
}

// for a slider with more settings, e.g. a suffix
pub fn slider_widget(ui: &mut Ui, label: &str, info: &str, slider: egui::Slider) -> bool
{
    property_row(ui, label, info, |ui| ui.add(slider).changed())
}

pub fn vector_edit(ui: &mut Ui, label: &str, info: &str, value: &mut Vector3<f32>) -> bool
{
    property_row(ui, label, info, |ui|
    {
        let mut changed = false;
        changed |= ui.add(egui::DragValue::new(&mut value.x).speed(0.01).prefix("x: ")).changed();
        changed |= ui.add(egui::DragValue::new(&mut value.y).speed(0.01).prefix("y: ")).changed();
        changed |= ui.add(egui::DragValue::new(&mut value.z).speed(0.01).prefix("z: ")).changed();
        changed
    })
}

// the given options, named by Debug
pub fn combo<T: PartialEq + Copy + std::fmt::Debug>(ui: &mut Ui, id: impl Hash + std::fmt::Debug, label: &str, info: &str, value: &mut T, options: &[T]) -> bool
{
    property_row(ui, label, info, |ui|
    {
        let before = *value;

        egui::ComboBox::from_id_salt(id).selected_text(format!("{:?}", value)).show_ui(ui, |ui|
        {
            for option in options
            {
                ui.selectable_value(value, *option, format!("{:?}", option));
            }
        });

        before != *value
    })
}

// every variant of a strum enum, named by Display
pub fn enum_combo<T: IntoEnumIterator + PartialEq + Copy + std::fmt::Display>(ui: &mut Ui, id: impl Hash + std::fmt::Debug, label: &str, info: &str, value: &mut T) -> bool
{
    property_row(ui, label, info, |ui|
    {
        let before = *value;

        egui::ComboBox::from_id_salt(id).selected_text(value.to_string()).show_ui(ui, |ui|
        {
            for option in T::iter().filter(|option| option.to_string() != "Unkown")
            {
                ui.selectable_value(value, option, option.to_string());
            }
        });

        before != *value
    })
}
