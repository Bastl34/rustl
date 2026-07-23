#![allow(dead_code)]

use std::sync::Arc;

use nalgebra::{Point2, Point3};

use crate::{component_downcast, gui::editor::{editor::EDITOR_INTERNAL_TAG, editor_state::{BoxSelect, EditorState, SelectionType, SettingsPanel}, helper::apply_fly_camera_move_state}, helper::math::{approx_zero, clamp_point2}, input::{keyboard::{Key, Modifier}, mouse::MouseButton}, state::{scene::{camera::Camera, components::{component::Component, mesh::Mesh}, node::{Node, NodeItem}, scene::{PickPredicate, Scene}}, state::{ENGINE_INTERNAL_TAG, ENGINE_INTERNAL_TAG_PREFX, State}}};

// blender style box select:
// - b arms the mode (crosshair)
// - dragging with the left mouse button spans the selection rect
// - on release everything inside the rect gets selected (with x-ray mode on: also occluded objects, off: only visible ones)
// - ctrl while releasing extends the current selection instead of replacing it
// - shift while releasing removes the boxed objects from the current selection
// - escape / right click cancels

pub fn update_box_select(editor_state: &mut EditorState, state: &mut State)
{
    let ctrl_or_logo = state.io.input_manager.keyboard.is_holding_modifier(Modifier::LeftCtrl) || state.io.input_manager.keyboard.is_holding_modifier(Modifier::LeftLogo);

    // plain b toggles the mode (ctrl+b is the sidebar shortcut)
    if !ctrl_or_logo && editor_state.selectable && state.io.input_manager.keyboard.is_pressed(Key::B)
    {
        if editor_state.box_select.is_some()
        {
            cancel_box_select(editor_state, state);
        }
        else
        {
            editor_state.box_select = Some(BoxSelect::new());

            // do not move objects around while box selecting
            editor_state.edit_mode = None;
        }
    }

    if editor_state.box_select.is_none()
    {
        return;
    }

    // right click cancels
    if state.io.input_manager.mouse.is_pressed(MouseButton::Right)
    {
        cancel_box_select(editor_state, state);
        return;
    }

    let pos = state.io.input_manager.mouse.point.pos;
    let lmb_holding = state.io.input_manager.mouse.is_holding(MouseButton::Left);

    let dragging = editor_state.box_select.as_ref().unwrap().drag_start.is_some();

    if !dragging
    {
        // start the drag when the left button goes down inside an editor camera viewport
        if lmb_holding && state.io.input_manager.mouse.is_first_action(MouseButton::Left, state.stats.frame)
        {
            if let Some(pos) = pos
            {
                if let Some(camera_id) = find_editor_camera_at(state, &pos)
                {
                    let box_select = editor_state.box_select.as_mut().unwrap();
                    box_select.camera_id = Some(camera_id);
                    box_select.drag_start = Some(pos);
                    box_select.drag_current = Some(pos);

                    // stop the fly camera from rotating while dragging
                    set_fly_camera_move_state(state, false);
                }
            }
        }
    }
    else
    {
        if let Some(pos) = pos
        {
            editor_state.box_select.as_mut().unwrap().drag_current = Some(pos);
        }

        // apply the selection when the left button is released
        if !lmb_holding
        {
            let extend = ctrl_or_logo;
            let subtract = !extend && state.io.input_manager.keyboard.is_holding_modifier(Modifier::LeftShift);
            apply_box_selection(editor_state, state, extend, subtract);

            editor_state.box_select = None;
            set_fly_camera_move_state(state, true);
        }
    }
}

pub fn cancel_box_select(editor_state: &mut EditorState, state: &mut State)
{
    editor_state.box_select = None;
    set_fly_camera_move_state(state, true);
}

fn set_fly_camera_move_state(state: &mut State, move_state: bool)
{
    if let Some(scene_id) = state.get_active_scene_id()
    {
        if let Some(scene) = state.find_scene_by_id_mut(scene_id)
        {
            apply_fly_camera_move_state(scene, move_state);
        }
    }
}

fn find_editor_camera_at(state: &State, pos: &Point2<f32>) -> Option<u32>
{
    let scene = state.get_active_scene()?;

    for camera in &scene.cameras
    {
        if camera.enabled && camera.is_point_in_viewport(pos) && camera.tags.contains_starts_with(ENGINE_INTERNAL_TAG_PREFX)
        {
            return Some(camera.id);
        }
    }

    None
}

fn apply_box_selection(editor_state: &mut EditorState, state: &mut State, extend: bool, subtract: bool)
{
    let box_select = editor_state.box_select.unwrap();

    let (Some(camera_id), Some(start), Some(current)) = (box_select.camera_id, box_select.drag_start, box_select.drag_current) else { return; };

    let xray = state.rendering.xray_mode;

    let scene_id;
    let new_ids;
    {
        let Some(scene) = state.get_active_scene() else { return; };
        scene_id = scene.id;

        let Some(camera) = scene.get_camera_by_id(camera_id) else { return; };

        // selection rect (physical px, origin bottom left), clamped to the camera viewport
        let cam_data = camera.get_data();
        let viewport = cam_data.get_viewport();

        let vp_min_x = viewport.x * cam_data.resolution_width as f32;
        let vp_min_y = viewport.y * cam_data.resolution_height as f32;
        let vp_max_x = vp_min_x + viewport.width * cam_data.resolution_width as f32;
        let vp_max_y = vp_min_y + viewport.height * cam_data.resolution_height as f32;

        let rect_min = Point2::<f32>::new(start.x.min(current.x).max(vp_min_x), start.y.min(current.y).max(vp_min_y));
        let rect_max = Point2::<f32>::new(start.x.max(current.x).min(vp_max_x), start.y.max(current.y).min(vp_max_y));

        // a click without dragging (or a rect fully outside of the viewport) changes nothing
        if rect_min.x >= rect_max.x || rect_min.y >= rect_max.y
        {
            return;
        }

        new_ids = collect_nodes_in_rect(scene, camera, &rect_min, &rect_max, xray);
    }

    // merge with the existing selection
    let mut selection: Vec<u32> = if extend || subtract { editor_state.hierarchy_multi_select.clone() } else { vec![] };

    // when extending/subtracting: include the current single selection too
    if (extend || subtract) && selection.is_empty() && editor_state.selected_type == SelectionType::Object
    {
        if let Some(selected_id) = editor_state.get_selected_node_id()
        {
            selection.push(selected_id);
        }
    }

    if subtract
    {
        // shift: deselect everything inside the rect
        selection.retain(|id| !new_ids.contains(id));
    }
    else
    {
        for id in new_ids
        {
            if !selection.contains(&id)
            {
                selection.push(id);
            }
        }
    }

    // clear the current active object - it is re-set below when the selection is a single node
    editor_state.de_select_current_item(state);

    editor_state.hierarchy_multi_select = selection.clone();
    EditorState::apply_highlight_for_node_ids(state, &selection);

    // single node -> make it the active object (properties panel, gizmos, ...)
    if selection.len() == 1
    {
        if let Some(scene) = state.find_scene_by_id_mut(scene_id)
        {
            let selected = editor_state.set_selected_object(scene, selection[0], None, SelectionType::Object, editor_state.use_highlight);

            if selected && editor_state.settings_panel != SettingsPanel::Object && editor_state.settings_panel != SettingsPanel::Components
            {
                editor_state.settings_panel = SettingsPanel::Object;
            }
        }
    }
}

fn collect_nodes_in_rect(scene: &Scene, camera: &Camera, rect_min: &Point2<f32>, rect_max: &Point2<f32>, xray: bool) -> Vec<u32>
{
    let cam_data = camera.get_data();
    let culling_mask = cam_data.culling_mask;

    // same filter as the normal click pick (helper::pick)
    let pick_predicate: PickPredicate = Arc::new(move |node_arc: NodeItem, _instance_id: Option<u32>| -> bool
    {
        let node = node_arc.read().unwrap();

        if node.tags.contains(ENGINE_INTERNAL_TAG) || node.tags.contains(EDITOR_INTERNAL_TAG)
        {
            return false;
        }

        (node.settings.layer_mask & culling_mask) != 0
    });

    let mesh_nodes = Scene::list_all_child_nodes_with_mesh(&scene.nodes);

    let mut target_ids: Vec<u32> = vec![];

    for node_arc in &mesh_nodes
    {
        // node filters (same rules as the ray based picking)
        let instance_ids: Vec<u32>;
        {
            let node = node_arc.read().unwrap();

            if node.tags.contains(ENGINE_INTERNAL_TAG) || node.tags.contains(EDITOR_INTERNAL_TAG)
            {
                continue;
            }

            if !node.is_visible() || !node.settings.pickable
            {
                continue;
            }

            if (node.settings.layer_mask & culling_mask) == 0
            {
                continue;
            }

            let Some(mesh) = node.find_component::<Mesh>() else { continue; };
            {
                component_downcast!(mesh, Mesh);
                if !mesh.get_base().is_enabled
                {
                    continue;
                }
            }

            instance_ids = node.instances.get_ref().iter().filter_map(|instance|
            {
                let instance = instance.read().unwrap();

                if !instance.get_data().visible || !instance.pickable || approx_zero(instance.get_cached_alpha())
                {
                    return None;
                }

                Some(instance.id)
            }).collect();
        }

        if instance_ids.is_empty()
        {
            continue;
        }

        // the selection target is the root node (same as a click in the viewport)
        let target_id = match Node::find_root_node(node_arc.clone())
        {
            Some(root) => root.read().unwrap().id,
            None => node_arc.read().unwrap().id,
        };

        if target_ids.contains(&target_id)
        {
            continue;
        }

        'instances: for instance_id in instance_ids
        {
            let bounding_info = node_arc.read().unwrap().get_world_bounding_info(Some(instance_id), false, None);

            let Some((b_min, b_max)) = bounding_info else { continue; };

            let corners =
            [
                Point3::<f32>::new(b_min.x, b_min.y, b_min.z),
                Point3::<f32>::new(b_max.x, b_min.y, b_min.z),
                Point3::<f32>::new(b_min.x, b_max.y, b_min.z),
                Point3::<f32>::new(b_max.x, b_max.y, b_min.z),
                Point3::<f32>::new(b_min.x, b_min.y, b_max.z),
                Point3::<f32>::new(b_max.x, b_min.y, b_max.z),
                Point3::<f32>::new(b_min.x, b_max.y, b_max.z),
                Point3::<f32>::new(b_max.x, b_max.y, b_max.z),
            ];

            // project the bounding box corners onto the screen
            let mut screen_points: Vec<Point2<f32>> = vec![];
            for corner in &corners
            {
                if let Some(screen_point) = camera.world_to_screen(corner)
                {
                    screen_points.push(screen_point);
                }
            }

            if screen_points.is_empty()
            {
                continue;
            }

            // screen space aabb of the bounding box
            let mut screen_min = screen_points[0];
            let mut screen_max = screen_points[0];
            for point in &screen_points
            {
                screen_min.x = screen_min.x.min(point.x);
                screen_min.y = screen_min.y.min(point.y);
                screen_max.x = screen_max.x.max(point.x);
                screen_max.y = screen_max.y.max(point.y);
            }

            // overlap with the selection rect?
            if screen_max.x < rect_min.x || screen_min.x > rect_max.x || screen_max.y < rect_min.y || screen_min.y > rect_max.y
            {
                continue;
            }

            // x-ray on: everything inside the rect counts ("select through")
            if xray
            {
                target_ids.push(target_id);
                break 'instances;
            }

            // x-ray off: only visible objects count -> cast a few rays into the rect and check if the object is hit first
            let mut samples: Vec<Point2<f32>> = vec![];

            // center of the overlap region (most likely point on the object)
            let overlap_min = Point2::<f32>::new(screen_min.x.max(rect_min.x), screen_min.y.max(rect_min.y));
            let overlap_max = Point2::<f32>::new(screen_max.x.min(rect_max.x), screen_max.y.min(rect_max.y));
            samples.push(Point2::<f32>::new((overlap_min.x + overlap_max.x) / 2.0, (overlap_min.y + overlap_max.y) / 2.0));

            // projected bounding box center + corners (clamped into the rect)
            let center = Point3::<f32>::new((b_min.x + b_max.x) / 2.0, (b_min.y + b_max.y) / 2.0, (b_min.z + b_max.z) / 2.0);
            if let Some(screen_center) = camera.world_to_screen(&center)
            {
                samples.push(clamp_point2(&screen_center, rect_min, rect_max));
            }

            for point in &screen_points
            {
                samples.push(clamp_point2(point, rect_min, rect_max));
            }

            for sample in &samples
            {
                if !camera.is_point_in_viewport(sample)
                {
                    continue;
                }

                let ray = camera.get_ray_from_viewport_coordinates(sample);

                if let Some(hit) = scene.pick(&ray, false, false, false, false, Some(pick_predicate.clone()))
                {
                    let hit_target_id = match Node::find_root_node(hit.node.clone())
                    {
                        Some(root) => root.read().unwrap().id,
                        None => hit.node.read().unwrap().id,
                    };

                    if hit_target_id == target_id
                    {
                        target_ids.push(target_id);
                        break 'instances;
                    }
                }
            }
        }
    }

    target_ids
}

// draws the selection rect (or the crosshair while waiting for the drag) as an egui overlay
pub fn draw_box_select_overlay(ui: &egui::Ui, editor_state: &EditorState, state: &State)
{
    let Some(box_select) = &editor_state.box_select else { return; };

    let ctx = ui.ctx();

    ctx.set_cursor_icon(egui::CursorIcon::Crosshair);

    // clip to the viewport area (the space left over after all panels) so the overlay never covers the ui
    let viewport_rect = ui.available_rect_before_wrap();

    let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("box_select_overlay"))).with_clip_rect(viewport_rect);

    // engine mouse coordinates (physical px, origin bottom left) -> egui points (origin top left)
    let scale = state.scale_factor.max(0.001);
    let to_egui = |point: &Point2<f32>| egui::pos2(point.x / scale, (state.height as f32 - point.y) / scale);

    let stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(230, 230, 230));

    if let (Some(start), Some(current)) = (box_select.drag_start.as_ref(), box_select.drag_current.as_ref())
    {
        let rect = egui::Rect::from_two_pos(to_egui(start), to_egui(current));

        painter.rect_filled(rect, 0.0, egui::Color32::from_rgba_unmultiplied(255, 255, 255, 10));

        let corners = [rect.left_top(), rect.right_top(), rect.right_bottom(), rect.left_bottom()];
        for i in 0..4
        {
            painter.extend(egui::Shape::dashed_line(&[corners[i], corners[(i + 1) % 4]], stroke, 4.0, 4.0));
        }
    }
    else if let Some(pos) = &state.io.input_manager.mouse.point.pos
    {
        // crosshair while waiting for the drag
        let pos = to_egui(pos);

        painter.extend(egui::Shape::dashed_line(&[egui::pos2(viewport_rect.left(), pos.y), egui::pos2(viewport_rect.right(), pos.y)], stroke, 4.0, 4.0));
        painter.extend(egui::Shape::dashed_line(&[egui::pos2(pos.x, viewport_rect.top()), egui::pos2(pos.x, viewport_rect.bottom())], stroke, 4.0, 4.0));
    }
}
