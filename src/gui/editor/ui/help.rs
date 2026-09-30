use egui::{Color32, RichText, Ui};

use crate::gui::{editor::editor_state::EditorState, helper::generic_items::modal_with_title};

fn key_chip(ui: &mut Ui, label: &str)
{
    let key_bg     = Color32::from_rgb(55, 55, 60);
    let key_fg     = Color32::from_rgb(220, 220, 225);
    let key_border = Color32::from_rgb(100, 100, 110);

    egui::Frame::new()
        .fill(key_bg)
        .stroke(egui::Stroke::new(1.0, key_border))
        .corner_radius(5.0)
        .inner_margin(egui::Margin::symmetric(7, 3))
        .shadow(egui::Shadow { color: Color32::from_black_alpha(80), offset: [0, 2], blur: 3, spread: 0 })
        .show(ui, |ui|
        {
            ui.label(RichText::new(label).strong().color(key_fg).size(12.0));
        });
}

fn mouse_chip(ui: &mut Ui, label: &str)
{
    let key_bg     = Color32::from_rgb(45, 60, 75);
    let key_fg     = Color32::from_rgb(140, 200, 255);
    let key_border = Color32::from_rgb(80, 120, 160);

    egui::Frame::new()
        .fill(key_bg)
        .stroke(egui::Stroke::new(1.0, key_border))
        .corner_radius(5.0)
        .inner_margin(egui::Margin::symmetric(7, 3))
        .shadow(egui::Shadow { color: Color32::from_black_alpha(80), offset: [0, 2], blur: 3, spread: 0 })
        .show(ui, |ui|
        {
            ui.label(RichText::new(label).strong().color(key_fg).size(12.0));
        });
}

// Token types for a binding row: either a keyboard key, a mouse button, or a "+" separator.
enum Chip<'a>
{
    Key(&'a str),
    Mouse(&'a str),
}

fn binding_row_mixed(ui: &mut Ui, chips: &[Chip], description: &str)
{
    ui.horizontal(|ui|
    {
        ui.set_min_width(300.0);

        ui.horizontal(|ui|
        {
            ui.set_min_width(180.0);
            for (i, chip) in chips.iter().enumerate()
            {
                if i > 0
                {
                    ui.label(RichText::new("+").color(Color32::from_rgb(150, 150, 160)).size(11.0));
                }
                match chip
                {
                    Chip::Key(label)   => key_chip(ui, label),
                    Chip::Mouse(label) => mouse_chip(ui, label),
                }
            }
        });

        ui.label(RichText::new(description).color(Color32::from_rgb(200, 200, 205)));
    });
    ui.add_space(2.0);
}

fn binding_row(ui: &mut Ui, keys: &[&str], description: &str)
{
    let chips: Vec<Chip> = keys.iter().map(|k| Chip::Key(k)).collect();
    binding_row_mixed(ui, &chips, description);
}

fn category_header(ui: &mut Ui, title: &str)
{
    ui.add_space(6.0);
    ui.horizontal(|ui|
    {
        ui.label(RichText::new(title).strong().color(Color32::from_rgb(130, 180, 255)).size(13.0));
    });
    ui.add(egui::Separator::default().spacing(6.0));
}

pub fn create_modal_help_shortcuts(editor_state: &mut EditorState, ctx: &egui::Context)
{
    let mut dialog_help_shortcuts = editor_state.dialog_help_shortcuts;

    modal_with_title(ctx, &mut dialog_help_shortcuts, "Shortcuts", true, true, |ui|
    {
        ui.set_min_width(380.0);

        egui::ScrollArea::vertical().max_height(520.0).show(ui, |ui|
        {
            // mouse
            category_header(ui, "  Mouse");
            binding_row_mixed(ui, &[Chip::Mouse("LMB")],                              "Select object in viewport");
            binding_row_mixed(ui, &[Chip::Key("Ctrl"), Chip::Mouse("LMB")],           "Multi-select (toggle) in viewport & hierarchy");
            binding_row_mixed(ui, &[Chip::Key("Shift"), Chip::Mouse("LMB")],          "Range-select in hierarchy");
            binding_row_mixed(ui, &[Chip::Mouse("LMB"), Chip::Key("drag")],           "Move / rotate object (in transform mode)");
            binding_row_mixed(ui, &[Chip::Mouse("Wheel")],                            "Step-rotate object (in rotate mode)");
            binding_row_mixed(ui, &[Chip::Mouse("drag asset")],                       "Drop asset from panel into viewport");

            // camera
            category_header(ui, "  Camera");
            binding_row_mixed(ui, &[Chip::Mouse("drag")],                             "Look around (perspective view)");
            binding_row(ui, &["W / A / S / D"],                                       "Move camera");
            binding_row(ui, &["Space / C"],                                           "Up / down (ortho views: zoom)");
            binding_row(ui, &["Shift"],                                               "Move faster (while moving)");
            binding_row_mixed(ui, &[Chip::Mouse("MMB"), Chip::Mouse("drag")],         "Pan (ortho views)");
            binding_row_mixed(ui, &[Chip::Mouse("Wheel")],                            "Zoom (ortho views)");

            // project
            category_header(ui, "  Project");
            binding_row(ui, &["Ctrl", "S"], "Save project");
            binding_row(ui, &["Ctrl", "Shift", "S"], "Save project as");
            binding_row(ui, &["Ctrl", "O"], "Open project");
            binding_row(ui, &["Ctrl", "N"], "New project");

            // view
            category_header(ui, "  View");
            binding_row(ui, &["H"],   "Hide / show UI");
            binding_row(ui, &["F"],   "Toggle fullscreen");
            binding_row(ui, &["Ctrl", "Alt", "Q"], "Quad view");
            binding_row(ui, &["Ctrl", "B"],        "Toggle left sidebar");
            binding_row(ui, &["Ctrl", "Alt", "B"], "Toggle right sidebar");
            binding_row(ui, &["Ctrl", "J"],        "Toggle bottom panel");

            // play / simulate
            category_header(ui, "  Play / Simulate");
            binding_row(ui, &["Ctrl", "R"],          "Play");
            binding_row(ui, &["Ctrl", "Shift", "R"], "Play in fullscreen");
            binding_row(ui, &["Ctrl", "T"],          "Simulate - physics runs, the editor stays open");
            binding_row(ui, &["Ctrl", "Shift", "T"], "Simulate in fullscreen");
            binding_row(ui, &["P"],                  "Pause / resume");
            binding_row(ui, &["Escape"],             "Stop - back to edit mode");

            // selection
            category_header(ui, "  Selection");
            binding_row(ui, &["B"],            "Box select (drag a rect, x-ray mode selects through)");
            binding_row_mixed(ui, &[Chip::Key("B"), Chip::Key("Ctrl"), Chip::Mouse("drag")], "Box select: extend current selection");
            binding_row_mixed(ui, &[Chip::Key("B"), Chip::Key("Shift"), Chip::Mouse("drag")], "Box select: deselect boxed objects");
            binding_row_mixed(ui, &[Chip::Key("B"), Chip::Mouse("RMB")], "Box select: cancel (also Escape or B again)");
            binding_row(ui, &["Escape"],       "Deselect object / cancel action");
            binding_row(ui, &["Ctrl", "C"],    "Copy selected object");
            binding_row(ui, &["Ctrl", "V"],    "Paste object");
            binding_row(ui, &["Ctrl", "D"],    "Duplicate selected object");
            binding_row(ui, &["I"],            "Create instance of selected object");
            binding_row(ui, &["Del / Back"],   "Delete selected object(s)");

            // transform (Edit Mode)
            category_header(ui, "  Transform  (select an object first)");
            binding_row(ui, &["G"],           "Grab / move");
            binding_row(ui, &["R"],           "Rotate");
            binding_row(ui, &["X"],           "Constrain to X axis");
            binding_row(ui, &["Y"],           "Constrain to Y axis");
            binding_row(ui, &["Z"],           "Constrain to Z axis");
            binding_row(ui, &["Shift", "X"],  "Constrain to YZ plane");
            binding_row(ui, &["Shift", "Y"],  "Constrain to XZ plane");
            binding_row(ui, &["Shift", "Z"],  "Constrain to XY plane");
            binding_row(ui, &["Ctrl"],        "Grab: snap to grid center / rotate: 22.5° steps");
            binding_row(ui, &["Shift"],       "Grab: snap bottom-left to grid / rotate: snap to 22.5°");
            binding_row_mixed(ui, &[Chip::Mouse("LMB")], "End rotate (continues as grab)");
            binding_row(ui, &["Escape"],      "Cancel transform");

            // gizmo
            category_header(ui, "  Gizmo  (while dragging a handle)");
            binding_row(ui, &["Ctrl"],        "Snap (move: grid center, rotate / scale: steps)");
            binding_row(ui, &["Shift"],       "Move: snap bottom-left / rotate: slow / scale: uniform");

            // rendering
            category_header(ui, "  Rendering");
            binding_row(ui, &["Shift", "Z"],  "Toggle wireframe mode");
            binding_row(ui, &["Alt", "Z"],    "Toggle x-ray mode");

            // grid
            category_header(ui, "  Grid");
            binding_row(ui, &["+"],         "Move grid up");
            binding_row(ui, &["-"],         "Move grid down");
            binding_row(ui, &["Num 8"],     "Grid forward");
            binding_row(ui, &["Num 2"],     "Grid backward");
            binding_row(ui, &["Num 0"],     "Reset grid position");

            ui.add_space(4.0);
        });
    });

    if !dialog_help_shortcuts
    {
        editor_state.dialog_help_shortcuts = dialog_help_shortcuts;
    }
}
