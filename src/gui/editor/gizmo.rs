use std::{f32::consts::PI, sync::{Arc, RwLock}};

use nalgebra::{distance, Point2, Point3, UnitQuaternion, Vector3, Vector4};

use crate::{component_downcast, component_downcast_mut, console_error, gui::editor::helper::transform_vec_to_parent_local, helper::{concurrency::thread::spawn_thread, math::{self, extract_rotation_as_euler_vec, extract_rotation_only, signed_angle_between_points, snap_to_grid}}, input::{keyboard::Modifier, mouse::MouseButton}, state::{scene::{components::{material::{BlendMode, Material}, transformation::Transformation}, utilities::scene_utils::{self, execute_on_scene_mut_and_wait}}, state::State}};

use super::{editor_state::{EditorState, GizmoTypeAndAxis}, grid::create_grid, helper::{apply_fly_camera_move_state, find_transform_component, get_parent_world_transform_from_selected_node, get_world_transform_from_selected_node, pick_node, set_internal_tag_for_utils_nodes}};

const GIZMO_MOVEMENT_CLAMP: f32 = 10.0;
const GIZMO_SCALE_CLAMP: f32 = 10.0;
const GIZMO_SCALE_MIN: f32 = 0.01;
const GIZMO_SCALE_STEP: f32 = 0.1;

const GIZMO_ROTATION_STEP: f32 = PI / 16.0;
const GIZMO_ROTATION_SLOW_FACTOR: f32 = 0.1;

const GIZMO_SCALE_DISTANCE_FACTOR: f32 = 0.1;

pub fn create_gizmo_objects(editor_state: &mut EditorState, state: &mut State, editor_utils_id: u32)
{
    let scene = state.scenes.get_mut(0);
    if scene.is_none()
    {
        return;
    }
    let scene = scene.unwrap();
    let scene_id = scene.id.clone();

    let grid_size = editor_state.grid_size;
    let grid_amount = editor_state.grid_amount;

    let main_queue = state.main_thread_execution_queue.clone();
    let main_queue_clone = main_queue.clone();

    spawn_thread(move ||
    {
        create_grid(scene_id, Some(editor_utils_id), main_queue_clone.clone(), grid_amount, grid_size);

        let pos = scene_utils::load_object("objects/gizmo/gizmo_pos.glb", scene_id, Some(editor_utils_id), main_queue_clone.clone(), true, false, true, false, 0);
        let rot = scene_utils::load_object("objects/gizmo/gizmo_rot.glb", scene_id, Some(editor_utils_id), main_queue_clone.clone(), true, false, true, false, 0);
        let scale = scene_utils::load_object("objects/gizmo/gizmo_scale.glb", scene_id, Some(editor_utils_id), main_queue_clone.clone(), true, false, true, false, 0);

        if pos.is_err() || rot.is_err() || scale.is_err()
        {
            console_error!("can not load gizmo objects");
            return;
        }

        let pos = pos.unwrap();
        let rot = rot.unwrap();
        let scale = scale.unwrap();

        let mut gizmo_nodes = vec![];
        gizmo_nodes.extend(&pos);
        gizmo_nodes.extend(&rot);
        gizmo_nodes.extend(&scale);

        let main_queue_clone = main_queue.clone();
        execute_on_scene_mut_and_wait(main_queue_clone.clone(), scene_id, Box::new(move |scene|
        {
            let pos_root = pos.get(0).unwrap();
            let rot_root = rot.get(0).unwrap();
            let sacle_root = scale.get(0).unwrap();

            if let Some(node) = scene.find_node_by_id(*pos_root)
            {
                node.write().unwrap().settings.visible = true;
                node.write().unwrap().name = "gizmo_position".to_string();
            }

            if let Some(node) = scene.find_node_by_id(*rot_root)
            {
                node.write().unwrap().settings.visible = true;
                node.write().unwrap().name = "gizmo_rotation".to_string();
            }

            if let Some(node) = scene.find_node_by_id(*sacle_root)
            {
                node.write().unwrap().settings.visible = true;
                node.write().unwrap().name = "gizmo_scale".to_string();
            }

            for node_id in &gizmo_nodes
            {
                if let Some(node) = scene.find_node_by_id(*node_id)
                {
                    if node.read().unwrap().root_node
                    {
                        if node.read().unwrap().find_component::<Transformation>().is_none()
                        {
                            node.write().unwrap().add_component(Arc::new(RwLock::new(Box::new(Transformation::identity("Transform")))));
                        }

                        node.write().unwrap().settings.visible = false;
                    }

                    // rename to just x, y, z
                    if node.read().unwrap().is_name_matching_regex("^x .*") { node.write().unwrap().name = "x".to_string(); }
                    if node.read().unwrap().is_name_matching_regex("^y .*") { node.write().unwrap().name = "y".to_string(); }
                    if node.read().unwrap().is_name_matching_regex("^z .*") { node.write().unwrap().name = "z".to_string(); }
                    if node.read().unwrap().is_name_matching_regex("^xy .*") { node.write().unwrap().name = "xy".to_string(); }
                    if node.read().unwrap().is_name_matching_regex("^xz .*") { node.write().unwrap().name = "xz".to_string(); }
                    if node.read().unwrap().is_name_matching_regex("^yz .*") { node.write().unwrap().name = "yz".to_string(); }

                    node.write().unwrap().settings.render_group_id = 1;
                    node.write().unwrap().settings.depth_write = false;
                    node.write().unwrap().settings.depth_test = false;
                    node.write().unwrap().settings.pickable = false;

                    if let Some(material) = node.read().unwrap().find_component::<Material>()
                    {
                        component_downcast_mut!(material, Material);
                        material.get_data_mut().get_mut().unlit_shading = true;

                        material.get_data_mut().get_mut().alpha = 0.8;
                        material.get_data_mut().get_mut().blend_mode = BlendMode::Blend;
                        material.get_data_mut().get_mut().highlight_color = Vector3::new(1.0, 1.0, 1.0);
                    }
                }
            }

            // run internal tagging
            set_internal_tag_for_utils_nodes(scene);
        }));
    });
}

pub fn update_gizmo_visibility(editor_state: &mut EditorState, state: &mut State)
{
    let (_, node, _) = editor_state.get_selected_node(state);

    for scene in &mut state.scenes
    {
        let gizmo_translation = scene.find_node_by_name("gizmo_position");
        let gizmo_rotation = scene.find_node_by_name("gizmo_rotation");
        let gizmo_scale = scene.find_node_by_name("gizmo_scale");

        if let Some(gizmo_translation) = gizmo_translation
        {
            gizmo_translation.write().unwrap().settings.visible = editor_state.gizmo_position && node.is_some() && !node.as_ref().unwrap().read().unwrap().is_locked();
        }

        if let Some(gizmo_rotation) = gizmo_rotation
        {
            gizmo_rotation.write().unwrap().settings.visible = editor_state.gizmo_rotation && node.is_some() && !node.as_ref().unwrap().read().unwrap().is_locked();
        }

        if let Some(gizmo_scale) = gizmo_scale
        {
            gizmo_scale.write().unwrap().settings.visible = editor_state.gizmo_scale && node.is_some() && !node.as_ref().unwrap().read().unwrap().is_locked();
        }
    }
}

pub fn update_gizmos(editor_state: &mut EditorState, state: &mut State)
{
    update_gizmo_visibility(editor_state, state);

    if !editor_state.gizmo_position && !editor_state.gizmo_rotation && !editor_state.gizmo_scale
    {
        return;
    }

    let input_active = state.io.input_manager.mouse.is_holding(MouseButton::Left)  || state.io.input_manager.touch.has_touches();
    let first_action = state.io.input_manager.mouse.is_first_action(MouseButton::Left, state.stats.frame) || state.io.input_manager.touch.is_first_action(state.stats.frame);

    // reset state if needed
    if editor_state.selected_gizmo.is_some() && !input_active
    {
        editor_state.selected_gizmo = None;

        let (scene, _, _) = editor_state.get_selected_node(state);
        if let Some(scene) = scene
        {
            apply_fly_camera_move_state(scene, true);
        }
    }

    let pointer_pos = state.io.input_manager.mouse.point.pos;
    let pointer_pos_last = state.io.input_manager.mouse.point.last_pos;
    if pointer_pos.is_none() || pointer_pos_last.is_none()
    {
        return;
    }

    let pointer_pos = pointer_pos.unwrap();
    let pointer_pos_last = pointer_pos_last.unwrap();

    {
        let (scene, node, _) = editor_state.get_selected_node(state);

        if scene.is_none() || node.is_none()
        {
            return;
        }
    }

    // check locked
    if let (_, Some(node), _) = editor_state.get_selected_node(state)
    {
        if node.read().unwrap().is_locked()
        {
            return;
        }
    }

    let mut updated = false;
    updated = update_position_gizmo(pointer_pos, pointer_pos_last, first_action, input_active, editor_state, state) || updated;
    updated = update_rotation_gizmo(pointer_pos, pointer_pos_last, first_action, input_active, editor_state, state) || updated;
    updated = update_scale_gizmo(pointer_pos, pointer_pos_last, first_action, input_active, editor_state, state) || updated;

    move_gizmos(editor_state, state);

    hover_gizmos(pointer_pos, editor_state, state, updated);
}


pub fn update_position_gizmo(pointer_pos: Point2<f32>, pointer_pos_last: Point2<f32>, first_action: bool, input_active: bool, editor_state: &mut EditorState, state: &mut State) -> bool
{
    if !editor_state.gizmo_position
    {
        return false;
    }

    let grid_size = editor_state.grid_size;

    let (scene, node, instance_id) = editor_state.get_selected_node(state);
    let scene = scene.unwrap();
    let node = node.unwrap();

    let gizmo_position = scene.find_node_by_name("gizmo_position");

    if gizmo_position.is_none()
    {
        return false;
    }
    let gizmo_position = gizmo_position.unwrap();

    // ********** pointer input **********
    // set gizmo state based on axis
    if first_action && editor_state.selected_gizmo.is_none()
    {
        let pick_res = pick_node(state, gizmo_position.clone(), state.io.input_manager.mouse.point.pos.unwrap(), false, true);
        if let Some((_, pick_res)) = pick_res
        {
            let axis = pick_res.node.read().unwrap().name.clone();
            editor_state.selected_gizmo = match axis.as_str()
            {
                "x" => Some(GizmoTypeAndAxis::TranslateX),
                "y" => Some(GizmoTypeAndAxis::TranslateY),
                "z" => Some(GizmoTypeAndAxis::TranslateZ),
                "xy" => Some(GizmoTypeAndAxis::TranslateXY),
                "xz" => Some(GizmoTypeAndAxis::TranslateXZ),
                "yz" => Some(GizmoTypeAndAxis::TranslateYZ),
                _ => None
            };
        }

        editor_state.selected_object_gizmo_value = None;
    }

    let mut updated = false;

    // ********** move object **********
    if input_active && editor_state.selected_gizmo.is_some()
    {
        {
            let (scene, _, _) = editor_state.get_selected_node(state);
            let scene = scene.unwrap();
            apply_fly_camera_move_state(scene, false);
        }

        // ********** re-apply saved movement (without snapping) **********
        {
            let edit_transformation = find_transform_component(editor_state, state);

            // ********** re-apply saved movement (without snapping) **********
            if let Some(selected_object_position_gizmo) = editor_state.selected_object_gizmo_value
            {
                component_downcast_mut!(edit_transformation, Transformation);
                edit_transformation.set_translation(selected_object_position_gizmo);
            }
        }

        let mut gizmo_pos = Point3::<f32>::new(0.0, 0.0, 0.0);
        let gizmo_translation = gizmo_position.read().unwrap();
        let transform_component = gizmo_translation.find_component::<Transformation>();

        if let Some(transform_component) = transform_component
        {
            component_downcast!(transform_component, Transformation);
            gizmo_pos = transform_component.get_data().position.into();
        }

        {
            let plane_origin = gizmo_pos;
            let mut plane_normal = None;

            let mut ray_last = None;
            let mut ray_now = None;

            {
                let selected_gizmo = editor_state.selected_gizmo.clone();

                let (scene, _, _) = editor_state.get_selected_node(state);
                for camera in &scene.unwrap().cameras
                {
                    if camera.enabled && camera.is_point_in_viewport(&pointer_pos) && camera.is_point_in_viewport(&pointer_pos_last)
                    {
                        ray_last = Some(camera.get_ray_from_viewport_coordinates(&pointer_pos_last));
                        ray_now = Some(camera.get_ray_from_viewport_coordinates(&pointer_pos));

                        let xy_plane = Vector3::new(0.0, 0.0, 1.0);
                        let xz_plane = Vector3::new(0.0, 1.0, 0.0);
                        let yz_plane = Vector3::new(1.0, 0.0, 0.0);

                        plane_normal = match selected_gizmo
                        {
                            Some(GizmoTypeAndAxis::TranslateX) => Some(xy_plane),
                            Some(GizmoTypeAndAxis::TranslateY) => Some(xy_plane),
                            Some(GizmoTypeAndAxis::TranslateZ) => Some(xz_plane),
                            Some(GizmoTypeAndAxis::TranslateXY) => Some(xy_plane),
                            Some(GizmoTypeAndAxis::TranslateXZ) => Some(xz_plane),
                            Some(GizmoTypeAndAxis::TranslateYZ) => Some(yz_plane),
                            _ => None,
                        };

                        break;
                    }
                }
            }

            if let (Some(plane_normal), Some(ray_last), Some(ray_now)) = (plane_normal, ray_last, ray_now)
            {
                let p0 = math::ray_plane_intersection(&ray_last, plane_normal, plane_origin);
                let p1 = math::ray_plane_intersection(&ray_now, plane_normal, plane_origin);

                if let (Some(p0), Some(p1)) = (p0, p1)
                {
                    let vec: nalgebra::Matrix<f32, nalgebra::Const<3>, nalgebra::Const<1>, nalgebra::ArrayStorage<f32, 3, 1>> = p1 - p0;
                    let movement_vec = match editor_state.selected_gizmo
                    {
                        Some(GizmoTypeAndAxis::TranslateX) => Vector3::new(vec.x, 0.0, 0.0),
                        Some(GizmoTypeAndAxis::TranslateY) => Vector3::new(0.0, vec.y, 0.0),
                        Some(GizmoTypeAndAxis::TranslateZ) => Vector3::new(0.0, 0.0, vec.z),
                        Some(GizmoTypeAndAxis::TranslateXY) => Vector3::new(vec.x, vec.y, 0.0),
                        Some(GizmoTypeAndAxis::TranslateXZ) => Vector3::new(vec.x, 0.0, vec.z),
                        Some(GizmoTypeAndAxis::TranslateYZ) => Vector3::new(0.0, vec.y, vec.z),
                        _ => Vector3::<f32>::zeros()
                    };

                    let mut movement_vec = movement_vec;
                    movement_vec.x = movement_vec.x.clamp(-GIZMO_MOVEMENT_CLAMP, GIZMO_MOVEMENT_CLAMP);
                    movement_vec.y = movement_vec.y.clamp(-GIZMO_MOVEMENT_CLAMP, GIZMO_MOVEMENT_CLAMP);
                    movement_vec.z = movement_vec.z.clamp(-GIZMO_MOVEMENT_CLAMP, GIZMO_MOVEMENT_CLAMP);

                    let local_transform  = transform_vec_to_parent_local(instance_id.clone(), node.clone(), movement_vec);

                    {
                        if let Some(selected_object_gizmo_value) = editor_state.selected_object_gizmo_value.as_mut()
                        {
                            selected_object_gizmo_value.x += local_transform.x;
                            selected_object_gizmo_value.y += local_transform.y;
                            selected_object_gizmo_value.z += local_transform.z;
                        }
                        else
                        {
                            let edit_transformation = find_transform_component(editor_state, state);
                            component_downcast!(edit_transformation, Transformation);

                            let pos = edit_transformation.get_data().position.clone() + local_transform;
                            editor_state.selected_object_gizmo_value = Some(pos);
                        }
                    }

                    // ********** without snap **********
                    {
                        let edit_transformation = find_transform_component(editor_state, state);
                        component_downcast_mut!(edit_transformation, Transformation);
                        edit_transformation.apply_translation(local_transform.xyz());
                    }

                    // ********** snap to grid center **********
                    if state.io.input_manager.keyboard.is_holding_modifier(Modifier::LeftCtrl) || state.io.input_manager.keyboard.is_holding_modifier(Modifier::LeftLogo)
                    {
                        let bounding_info = node.read().unwrap().get_world_bounding_info(instance_id, true, None);
                        if let Some((b_min, b_max)) = bounding_info
                        {
                            let center = b_min + (b_max - b_min) / 2.0;

                            let new_x = snap_to_grid(center.x, grid_size);
                            let new_y = snap_to_grid(center.y, grid_size);
                            let new_z = snap_to_grid(center.z, grid_size);

                            let mut delta = Vector3::<f32>::new(0.0, 0.0, 0.0);

                            if editor_state.selected_gizmo == Some(GizmoTypeAndAxis::TranslateX) || editor_state.selected_gizmo == Some(GizmoTypeAndAxis::TranslateXY) || editor_state.selected_gizmo == Some(GizmoTypeAndAxis::TranslateXZ)
                            {
                                delta.x = new_x - center.x;
                            }

                            if editor_state.selected_gizmo == Some(GizmoTypeAndAxis::TranslateY) || editor_state.selected_gizmo == Some(GizmoTypeAndAxis::TranslateXY) || editor_state.selected_gizmo == Some(GizmoTypeAndAxis::TranslateYZ)
                            {
                                delta.y = new_y - center.y;
                            }

                            if editor_state.selected_gizmo == Some(GizmoTypeAndAxis::TranslateZ) || editor_state.selected_gizmo == Some(GizmoTypeAndAxis::TranslateXZ) || editor_state.selected_gizmo == Some(GizmoTypeAndAxis::TranslateYZ)
                            {
                                delta.z = new_z - center.z;
                            }

                            let delta = transform_vec_to_parent_local(instance_id.clone(), node.clone(), delta);

                            let edit_transformation = find_transform_component(editor_state, state);
                            component_downcast_mut!(edit_transformation, Transformation);

                            edit_transformation.apply_translation(delta);
                        }
                    }
                    // ********** bottom left snapping **********
                    else if state.io.input_manager.keyboard.is_holding_modifier(Modifier::LeftShift)
                    {
                        let bounding_info = node.read().unwrap().get_world_bounding_info(instance_id, true, None);
                        if let Some((b_min, b_max)) = bounding_info
                        {
                            let bottom_left = Vector3::<f32>::new(b_min.x, b_min.y, b_max.z);

                            let new_x = snap_to_grid(bottom_left.x, grid_size);
                            let new_y = snap_to_grid(bottom_left.y, grid_size);
                            let new_z = snap_to_grid(bottom_left.z, grid_size);

                            let mut delta = Vector3::<f32>::new(0.0, 0.0, 0.0);

                            if editor_state.selected_gizmo == Some(GizmoTypeAndAxis::TranslateX) || editor_state.selected_gizmo == Some(GizmoTypeAndAxis::TranslateXY) || editor_state.selected_gizmo == Some(GizmoTypeAndAxis::TranslateXZ)
                            {
                                delta.x = new_x - bottom_left.x;
                            }

                            if editor_state.selected_gizmo == Some(GizmoTypeAndAxis::TranslateY) || editor_state.selected_gizmo == Some(GizmoTypeAndAxis::TranslateXY) || editor_state.selected_gizmo == Some(GizmoTypeAndAxis::TranslateYZ)
                            {
                                delta.y = new_y - bottom_left.y;
                            }

                            if editor_state.selected_gizmo == Some(GizmoTypeAndAxis::TranslateZ) || editor_state.selected_gizmo == Some(GizmoTypeAndAxis::TranslateXZ) || editor_state.selected_gizmo == Some(GizmoTypeAndAxis::TranslateYZ)
                            {
                                delta.z = new_z - bottom_left.z;
                            }

                            let delta = transform_vec_to_parent_local(instance_id.clone(), node.clone(), delta);

                            let edit_transformation = find_transform_component(editor_state, state);
                            component_downcast_mut!(edit_transformation, Transformation);

                            edit_transformation.apply_translation(delta);
                        }
                    }

                    updated = true;
                }
            }
        }
    }

    updated
}

pub fn update_rotation_gizmo(pointer_pos: Point2<f32>, pointer_pos_last: Point2<f32>, first_action: bool, input_active: bool, editor_state: &mut EditorState, state: &mut State) -> bool
{
    if !editor_state.gizmo_rotation
    {
        return false;
    }

    let parent_transform = get_parent_world_transform_from_selected_node(editor_state, state);
    let parent_world_rotation_only = extract_rotation_only(&parent_transform);

    let (scene, _, _) = editor_state.get_selected_node(state);
    let scene = scene.unwrap();

    let gizmo_rotation = scene.find_node_by_name("gizmo_rotation");

    if gizmo_rotation.is_none()
    {
        return false;
    }
    let gizmo_rotation = gizmo_rotation.unwrap();


    // ********** pointer input **********
    // set gizmo state based on axis
    if first_action && editor_state.selected_gizmo.is_none()
    {
        let pick_res = pick_node(state, gizmo_rotation.clone(), state.io.input_manager.mouse.point.pos.unwrap(), false, true);
        if let Some((_, pick_res)) = pick_res
        {
            let axis = pick_res.node.read().unwrap().name.clone();
            editor_state.selected_gizmo = match axis.as_str()
            {
                "x" => Some(GizmoTypeAndAxis::RotateX),
                "y" => Some(GizmoTypeAndAxis::RotateY),
                "z" => Some(GizmoTypeAndAxis::RotateZ),
                _ => None
            };
        }

        editor_state.selected_object_gizmo_value = None;
    }

    let mut updated = false;

    // ********** rotate object **********
    if input_active && editor_state.selected_gizmo.is_some()
    {
        {
            let (scene, _, _) = editor_state.get_selected_node(state);
            let scene = scene.unwrap();
            apply_fly_camera_move_state(scene, false);
        }

        let mut gizmo_pos = Point3::<f32>::new(0.0, 0.0, 0.0);
        let gizmo_rotation = gizmo_rotation.read().unwrap();
        let transform_component = gizmo_rotation.find_component::<Transformation>();

        if let Some(transform_component) = transform_component
        {
            component_downcast!(transform_component, Transformation);
            gizmo_pos = transform_component.get_data().position.into();
        }

        {
            let plane_origin = gizmo_pos;
            let mut plane_normal = None;

            let mut ray_last = None;
            let mut ray_now = None;

            {
                let selected_gizmo = editor_state.selected_gizmo.clone();

                let (scene, _, _) = editor_state.get_selected_node(state);
                for camera in &scene.unwrap().cameras
                {
                    if camera.enabled && camera.is_point_in_viewport(&pointer_pos) && camera.is_point_in_viewport(&pointer_pos_last)
                    {
                        ray_last = Some(camera.get_ray_from_viewport_coordinates(&pointer_pos_last));
                        ray_now = Some(camera.get_ray_from_viewport_coordinates(&pointer_pos));

                        let xy_plane = Vector3::new(0.0, 0.0, 1.0);
                        let xz_plane = Vector3::new(0.0, 1.0, 0.0);
                        let yz_plane = Vector3::new(1.0, 0.0, 0.0);

                        plane_normal = match selected_gizmo
                        {
                            Some(GizmoTypeAndAxis::RotateX) => Some(yz_plane),
                            Some(GizmoTypeAndAxis::RotateY) => Some(xz_plane),
                            Some(GizmoTypeAndAxis::RotateZ) => Some(xy_plane),
                            _ => None,
                        };

                        break;
                    }
                }
            }

            if let (Some(plane_normal), Some(ray_last), Some(ray_now)) = (plane_normal, ray_last, ray_now)
            {
                let plane_normal = (parent_world_rotation_only * Vector4::<f32>::new(plane_normal.x, plane_normal.y, plane_normal.z, 0.0)).xyz();

                let p0 = math::ray_plane_intersection(&ray_last, plane_normal, plane_origin);
                let p1 = math::ray_plane_intersection(&ray_now, plane_normal, plane_origin);

                if let (Some(p0), Some(p1)) = (p0, p1)
                {
                    let mut angle = signed_angle_between_points(&gizmo_pos, &p0, &p1, &plane_normal);

                    if state.io.input_manager.keyboard.is_holding_modifier(Modifier::LeftShift)
                    {
                        angle *= GIZMO_ROTATION_SLOW_FACTOR;
                    }

                    let rotation_vec = match editor_state.selected_gizmo
                    {
                        Some(GizmoTypeAndAxis::RotateX) => Vector3::new(angle, 0.0, 0.0),
                        Some(GizmoTypeAndAxis::RotateY) => Vector3::new(0.0, angle, 0.0),
                        Some(GizmoTypeAndAxis::RotateZ) => Vector3::new(0.0, 0.0, angle),
                        _ => Vector3::<f32>::zeros()
                    };

                    // ********** apply rotation **********
                    {
                        let edit_transformation = find_transform_component(editor_state, state);
                        component_downcast_mut!(edit_transformation, Transformation);

                        let rotation_quat = UnitQuaternion::from_euler_angles(rotation_vec.x, rotation_vec.y, rotation_vec.z);

                        if let Some(selected_object_gizmo_value) = editor_state.selected_object_gizmo_value.as_mut()
                        {
                            edit_transformation.get_data_mut().get_mut().rotation = selected_object_gizmo_value.clone();
                        }

                        edit_transformation.convert_euler_angles_to_quaternion();
                        edit_transformation.apply_rotation_quaternion(*rotation_quat.as_vector(), false);
                        edit_transformation.convert_quaternion_to_euler_angles();
                    }

                    // ********** save **********
                    {
                        let edit_transformation = find_transform_component(editor_state, state);
                        component_downcast!(edit_transformation, Transformation);

                        editor_state.selected_object_gizmo_value = Some(edit_transformation.get_data().rotation.clone());
                    }

                    // ********** snap if needed **********
                    if state.io.input_manager.keyboard.is_holding_modifier(Modifier::LeftCtrl) || state.io.input_manager.keyboard.is_holding_modifier(Modifier::LeftLogo)
                    {
                        let edit_transformation = find_transform_component(editor_state, state);
                        component_downcast_mut!(edit_transformation, Transformation);

                        let rotation_vec = &mut edit_transformation.get_data_mut().get_mut().rotation;

                        if editor_state.selected_gizmo == Some(GizmoTypeAndAxis::RotateX)
                        {
                            rotation_vec.x = snap_to_grid(rotation_vec.x, GIZMO_ROTATION_STEP);
                        }
                        else if editor_state.selected_gizmo == Some(GizmoTypeAndAxis::RotateY)
                        {
                            rotation_vec.y = snap_to_grid(rotation_vec.y, GIZMO_ROTATION_STEP);
                        }
                        else if editor_state.selected_gizmo == Some(GizmoTypeAndAxis::RotateZ)
                        {
                            rotation_vec.z = snap_to_grid(rotation_vec.z, GIZMO_ROTATION_STEP);
                        }

                        edit_transformation.calc_transform();
                    }

                    updated = true;
                }
            }
        }
    }

    updated
}

pub fn update_scale_gizmo(pointer_pos: Point2<f32>, pointer_pos_last: Point2<f32>, first_action: bool, input_active: bool, editor_state: &mut EditorState, state: &mut State) -> bool
{
    if !editor_state.gizmo_scale
    {
        return false;
    }

    let world_transform = get_world_transform_from_selected_node(editor_state, state);
    let world_rotation_only = extract_rotation_only(&world_transform);

    let (scene, _, _) = editor_state.get_selected_node(state);
    let scene = scene.unwrap();

    let gizmo_scale = scene.find_node_by_name("gizmo_scale");

    if gizmo_scale.is_none()
    {
        return false;
    }
    let gizmo_scale = gizmo_scale.unwrap();

    // ********** pointer input **********
    // set gizmo state based on axis
    if first_action && editor_state.selected_gizmo.is_none()
    {
        let pick_res = pick_node(state, gizmo_scale.clone(), state.io.input_manager.mouse.point.pos.unwrap(), false, true);
        if let Some((_, pick_res)) = pick_res
        {
            let axis = pick_res.node.read().unwrap().name.clone();
            editor_state.selected_gizmo = match axis.as_str()
            {
                "x" => Some(GizmoTypeAndAxis::ScaleX),
                "y" => Some(GizmoTypeAndAxis::ScaleY),
                "z" => Some(GizmoTypeAndAxis::ScaleZ),
                "all" => Some(GizmoTypeAndAxis::ScaleUniform),
                _ => None
            };
        }

        editor_state.selected_object_gizmo_value = None;
    }

    let mut updated = false;

    // ********** move object **********
    if input_active && editor_state.selected_gizmo.is_some()
    {
        {
            let (scene, _, _) = editor_state.get_selected_node(state);
            let scene = scene.unwrap();
            apply_fly_camera_move_state(scene, false);
        }

        let mut gizmo_pos = Point3::<f32>::new(0.0, 0.0, 0.0);
        let gizmo_scale = gizmo_scale.read().unwrap();
        let transform_component = gizmo_scale.find_component::<Transformation>();

        if let Some(transform_component) = transform_component
        {
            component_downcast!(transform_component, Transformation);
            gizmo_pos = transform_component.get_data().position.into();
        }

        {
            let plane_origin = gizmo_pos;
            let mut plane_normal = None;

            let mut ray_last = None;
            let mut ray_now = None;

            {
                let selected_gizmo = editor_state.selected_gizmo.clone();

                let (scene, _, _) = editor_state.get_selected_node(state);
                for camera in &scene.unwrap().cameras
                {
                    if camera.enabled && camera.is_point_in_viewport(&pointer_pos) && camera.is_point_in_viewport(&pointer_pos_last)
                    {
                        ray_last = Some(camera.get_ray_from_viewport_coordinates(&pointer_pos_last));
                        ray_now = Some(camera.get_ray_from_viewport_coordinates(&pointer_pos));

                        let xy_plane = Vector3::new(0.0, 0.0, 1.0);
                        let xz_plane = Vector3::new(0.0, 1.0, 0.0);

                        plane_normal = match selected_gizmo
                        {
                            Some(GizmoTypeAndAxis::ScaleX) => Some(xy_plane),
                            Some(GizmoTypeAndAxis::ScaleY) => Some(xy_plane),
                            Some(GizmoTypeAndAxis::ScaleZ) => Some(xz_plane),
                            Some(GizmoTypeAndAxis::ScaleUniform) => Some(xy_plane),
                            _ => None,
                        };

                        break;
                    }
                }
            }

            if let (Some(plane_normal), Some(ray_last), Some(ray_now)) = (plane_normal, ray_last, ray_now)
            {
                let plane_normal = (world_rotation_only * Vector4::<f32>::new(plane_normal.x, plane_normal.y, plane_normal.z, 0.0)).xyz();

                let p0 = math::ray_plane_intersection(&ray_last, plane_normal, plane_origin);
                let p1 = math::ray_plane_intersection(&ray_now, plane_normal, plane_origin);

                if let (Some(p0), Some(p1)) = (p0, p1)
                {
                    let v0 = p0 - gizmo_pos;
                    let v1 = p1 - gizmo_pos;

                    let scale = v1.norm() / v0.norm();
                    let scale = scale.clamp(GIZMO_SCALE_MIN, GIZMO_SCALE_CLAMP);

                    let mut uniform_scale = editor_state.selected_gizmo == Some(GizmoTypeAndAxis::ScaleUniform);

                    let mut scale_vec = match editor_state.selected_gizmo
                    {
                        Some(GizmoTypeAndAxis::ScaleX) => Vector3::new(scale, 1.0, 1.0),
                        Some(GizmoTypeAndAxis::ScaleY) => Vector3::new(1.0, scale, 1.0),
                        Some(GizmoTypeAndAxis::ScaleZ) => Vector3::new(1.0, 1.0, scale),
                        Some(GizmoTypeAndAxis::ScaleUniform) => Vector3::new(scale, scale, scale),
                        _ => Vector3::new(1.0, 1.0, 1.0),
                    };

                    // uniform scale for holding shift
                    if state.io.input_manager.keyboard.is_holding_modifier(Modifier::LeftShift)
                    {
                        scale_vec = Vector3::new(scale, scale, scale);
                        uniform_scale = true;
                    }

                    if let Some(selected_object_gizmo_value) = editor_state.selected_object_gizmo_value.as_mut()
                    {
                        selected_object_gizmo_value.x *= scale_vec.x;
                        selected_object_gizmo_value.y *= scale_vec.y;
                        selected_object_gizmo_value.z *= scale_vec.z;
                    }
                    else
                    {
                        let edit_transformation = find_transform_component(editor_state, state);
                        component_downcast!(edit_transformation, Transformation);

                        let mut scale = scale_vec;
                        scale.x *= edit_transformation.get_data().scale.x;
                        scale.y *= edit_transformation.get_data().scale.y;
                        scale.z *= edit_transformation.get_data().scale.z;
                        editor_state.selected_object_gizmo_value = Some(scale);
                    }

                    let snapping = state.io.input_manager.keyboard.is_holding_modifier(Modifier::LeftCtrl) || state.io.input_manager.keyboard.is_holding_modifier(Modifier::LeftLogo);

                    // ********** without snap **********
                    if !snapping
                    {
                        let edit_transformation = find_transform_component(editor_state, state);
                        component_downcast_mut!(edit_transformation, Transformation);

                        let scale_vec = editor_state.selected_object_gizmo_value.unwrap();
                        edit_transformation.set_scale(scale_vec);
                    }

                    // ********** snap based scale **********
                    else if snapping
                    {
                        let mut scale_vec = editor_state.selected_object_gizmo_value.unwrap();

                        if uniform_scale
                        {
                            scale_vec.x = snap_to_grid(scale_vec.x, GIZMO_SCALE_STEP);
                            scale_vec.y = snap_to_grid(scale_vec.y, GIZMO_SCALE_STEP);
                            scale_vec.z = snap_to_grid(scale_vec.z, GIZMO_SCALE_STEP);
                        }
                        else if editor_state.selected_gizmo == Some(GizmoTypeAndAxis::ScaleX)
                        {
                            scale_vec.x = snap_to_grid(scale_vec.x, GIZMO_SCALE_STEP);
                        }
                        else if editor_state.selected_gizmo == Some(GizmoTypeAndAxis::ScaleY)
                        {
                            scale_vec.y = snap_to_grid(scale_vec.y, GIZMO_SCALE_STEP);
                        }
                        else if editor_state.selected_gizmo == Some(GizmoTypeAndAxis::ScaleZ)
                        {
                            scale_vec.z = snap_to_grid(scale_vec.z, GIZMO_SCALE_STEP);
                        }

                        let edit_transformation = find_transform_component(editor_state, state);
                        component_downcast_mut!(edit_transformation, Transformation);
                        edit_transformation.set_scale(scale_vec);
                    }

                    updated = true;
                }
            }
        }
    }

    updated
}

pub fn move_gizmos(editor_state: &mut EditorState, state: &mut State)
{
    let world_transform = get_world_transform_from_selected_node(editor_state, state);
    let world_rotatio_only = extract_rotation_as_euler_vec(&world_transform);

    let parent_transform = get_parent_world_transform_from_selected_node(editor_state, state);
    let parent_world_rotation_only = extract_rotation_as_euler_vec(&parent_transform);

    let (scene, _, _) = editor_state.get_selected_node(state);
    let scene = scene.unwrap();

    // get pos from transform
    let pos = world_transform.column(3).xyz();

    // calculate gizmo scaling
    let camera = scene.cameras.iter().find(|c| c.enabled).unwrap();
    let cam_pos = camera.get_data().eye_pos;
    let distance = distance(&pos.into(), &cam_pos);

    let scale = distance * GIZMO_SCALE_DISTANCE_FACTOR;

    // position gizmo
    let gizmo_translation = scene.find_node_by_name("gizmo_position");
    if let Some(gizmo_translation) = gizmo_translation
    {
        let gizmo_translation = gizmo_translation.write().unwrap();
        let transform_component = gizmo_translation.find_component::<Transformation>();

        if let Some(transform_component) = transform_component
        {
            component_downcast_mut!(transform_component, Transformation);
            let new_scale = Vector3::new(scale, scale, scale);

            if !math::approx_equal_vec(&transform_component.get_data().position, &pos)
            {
                transform_component.set_translation(pos);
            }
            if !math::approx_equal_vec(&transform_component.get_data().scale, &new_scale)
            {
                transform_component.set_scale(new_scale);
            }
        }
    }

    // rotation gizmo
    let gizmo_rotation = scene.find_node_by_name("gizmo_rotation");
    if let Some(gizmo_rotation) = gizmo_rotation
    {
        let gizmo_rotation = gizmo_rotation.write().unwrap();
        let transform_component = gizmo_rotation.find_component::<Transformation>();

        if let Some(transform_component) = transform_component
        {
            component_downcast_mut!(transform_component, Transformation);
            let new_scale = Vector3::new(scale, scale, scale);

            if !math::approx_equal_vec(&transform_component.get_data().position, &pos)
            {
                transform_component.set_translation(pos);
            }
            if !math::approx_equal_vec(&transform_component.get_data().rotation, &parent_world_rotation_only) {
                transform_component.set_rotation(parent_world_rotation_only);
            }
            if !math::approx_equal_vec(&transform_component.get_data().scale, &new_scale) {
                transform_component.set_scale(new_scale);
            }
        }
    }

    // scale gizmo
    let gizmo_scale = scene.find_node_by_name("gizmo_scale");
    if let Some(gizmo_scale) = gizmo_scale
    {
        let gizmo_scale = gizmo_scale.write().unwrap();
        let transform_component = gizmo_scale.find_component::<Transformation>();

        if let Some(transform_component) = transform_component
        {
            component_downcast_mut!(transform_component, Transformation);
            let new_scale = Vector3::new(scale, scale, scale);

            if !math::approx_equal_vec(&transform_component.get_data().position, &pos)
            {
                transform_component.set_translation(pos);
            }
            if !math::approx_equal_vec(&transform_component.get_data().rotation, &world_rotatio_only)
            {
                transform_component.set_rotation(world_rotatio_only);
            }
            if !math::approx_equal_vec(&transform_component.get_data().scale, &new_scale)
            {
                transform_component.set_scale(new_scale);
            }
        }
    }
}

pub fn hover_gizmos(pointer_pos: Point2<f32>, editor_state: &mut EditorState, state: &mut State, gizmo_transform_updated: bool)
{
    // if transform was updated (e.g. by dragging), keep the current highlight state
    if gizmo_transform_updated
    {
        return;
    }

    let gizmos = vec!
    [
        ("gizmo_position", editor_state.gizmo_position),
        ("gizmo_rotation", editor_state.gizmo_rotation),
        ("gizmo_scale", editor_state.gizmo_scale)
    ];

    let mut hovered_node_id = None;
    let mut min_dist = std::f32::MAX;

    // only pick if not doing action
    if !state.io.input_manager.is_main_pointer_action_active()
    {
        let selected_scene_id = editor_state.selected_scene_id;

        if let Some(selected_scene_id) = selected_scene_id
        {
            if let Some(scene) = state.find_scene_by_id(selected_scene_id)
            {
                for (gizmo_name, gizmo_enabled) in &gizmos
                {
                    if !gizmo_enabled { continue; }

                    if let Some(gizmo_node) = scene.find_node_by_name(gizmo_name)
                    {
                        if let Some((_, pick_res)) = pick_node(state, gizmo_node, pointer_pos, false, true)
                        {
                            if pick_res.time_of_impact < min_dist
                            {
                                min_dist = pick_res.time_of_impact;
                                hovered_node_id = Some(pick_res.node.read().unwrap().id);
                            }
                        }
                    }
                }
            }
        }
    }

    // determine if we need to update highlights
    if hovered_node_id != editor_state.highlighted_gizmo_id
    {
        // 1. clear everything (safest way to ensure old highlight is gone)
        let selected_scene_id = editor_state.selected_scene_id;
        if let Some(selected_scene_id) = selected_scene_id
        {
            if let Some(scene) = state.find_scene_by_id_mut(selected_scene_id)
            {
                for (gizmo_name, gizmo_enabled) in &gizmos
                {
                    if !gizmo_enabled { continue; }

                    if let Some(gizmo_node) = scene.find_node_by_name(gizmo_name)
                    {
                        gizmo_node.write().unwrap().set_highlighted(false);
                    }
                }

                // 2. highlight new one if it exists
                if let Some(node_id) = hovered_node_id
                {
                    if let Some(node) = scene.find_node_by_id(node_id)
                    {
                        node.write().unwrap().set_highlighted(true);
                    }
                }
            }
        }

        editor_state.highlighted_gizmo_id = hovered_node_id;
    }
}