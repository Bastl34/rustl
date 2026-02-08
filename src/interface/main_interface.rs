#![allow(dead_code)]

use std::cell::RefCell;
use std::mem::swap;
use std::rc::Rc;
use std::sync::{Arc, RwLock};
use web_time::{Instant, SystemTime, UNIX_EPOCH};
use std::{vec, cmp};

use gilrs::Gilrs;
use nalgebra::{Vector2, Point2};
use winit::dpi::PhysicalPosition;
use winit::event::ElementState;
use winit::keyboard::ModifiersKeyState;
use winit::window::{Window, Fullscreen, CursorGrabMode};

use crate::helper::concurrency::execution_queue::ExecutionQueue;
use crate::helper::platform::is_mac;
use crate::input::input_point::PointState;
use crate::input::keyboard::Modifier;
use crate::interface::winit::winit_map_mouse_button;
use crate::output::audio_device::AudioDevice;
use crate::state::scene::utilities::scene_utils::highlight_and_unhighlight_scene_meshes;
use crate::{console_debug, console_error, console_log, rendering};
use crate::rendering::egui::EGui;
use crate::rendering::scene::Scene;
use crate::state::gui::editor::editor::Editor;
use crate::rendering::wgpu::WGpu;
use crate::state::helper::render_item::get_render_item_mut;
use crate::state::state::{State, FPS_CHART_VALUES, REFERENCE_UPDATE_FRAMES};

use super::app::App;
use super::context::Context;
use super::gilrs::{gilrs_event, gilrs_initialize};
use super::winit::winit_map_key;

pub struct MainInterface
{
    pub context: Context,

    app: Option<Box<dyn App>>,

    gilrs: Option<Gilrs>,
    editor_gui: Editor,
}

impl MainInterface
{
    //pub async fn new(window: Arc<Window>, event_loop: &winit::event_loop::EventLoop<()>) -> Self
    pub async fn new(window: Arc<Window>) -> Self
    {
        let audio_device = AudioDevice::default();
        let state = State::new(Arc::new(RwLock::new(Box::new(audio_device))));
        let state = Rc::new(RefCell::new(state));

        let samlpes;
        let mut wgpu: WGpu;
        {
            let state = &mut *(state.borrow_mut());
            state.width = window.inner_size().width;
            state.height = window.inner_size().height;
            state.scale_factor = window.scale_factor() as f32;

            wgpu = WGpu::new(window.clone(), state).await;

            state.rendering.msaa.set(cmp::min(state.rendering.msaa.get_ref().clone(), state.rendering_adapter.max_msaa_samples));
            samlpes = *(state.rendering.msaa.get_ref());

            wgpu.create_msaa_texture(samlpes);
        }

        let egui = EGui::new(wgpu.device(), wgpu.surface_config(), window.clone());

        let editor_gui = Editor::new();

        let gilrs_res = Gilrs::new();
        let mut gilrs = None;
        if let Ok(gilrs_res) = gilrs_res
        {
            gilrs = Some(gilrs_res);
        }

        let mut interface = Self
        {
            context: Context
            {
                state,

                window_title: window.title().clone(),
                window_minimized: false,

                wgpu,
                window,
                egui
            },

            app: None,

            gilrs,
            editor_gui,
        };

        interface.scene_init();
        interface.init();

        interface
    }

    pub fn init(&mut self)
    {
        {
            let state = &mut *(self.context.state.borrow_mut());
            let samlpes = *(state.rendering.msaa.get_ref());

            // move out scenes from state to prevent using multiple mut borrows
            let mut scenes = vec![];
            swap(&mut state.scenes, &mut scenes);

            for scene in &mut scenes
            {
                let render_item = Scene::new(&mut self.context.wgpu, state, scene, samlpes);
                scene.render_item = Some(Box::new(render_item));
            }

            swap(&mut scenes, &mut state.scenes);

            // gamepad init
            if let Some(gilrs) = &mut self.gilrs
            {
                gilrs_initialize(state, gilrs);
            }

            // init editor
            self.editor_gui.init(state, &self.context.egui);
        }

        // create dummy app
        let dummy_app = super::app_dummy::AppDummy::new();
        self.app = Some(Box::new(dummy_app));

        // init app
        if let Some(app) = &mut self.app
        {
            app.init(&mut self.context);
        }
    }

    pub fn window(&self) -> &Window
    {
        &self.context.window
    }

    pub fn resize(&mut self, dimensions: Option<winit::dpi::PhysicalSize<u32>>, scale_factor: Option<f64>)
    {
        let mut width;
        let mut height;

        if let Some(dimensions) = dimensions
        {
            width = dimensions.width;
            height = dimensions.height;
        }
        else
        {
            let size = self.context.window.inner_size();
            width = size.width;
            height = size.height;
        }

        self.context.window_minimized = width == 0 && height == 0;

        console_log!("resize {}x{} (minimized: {})", width, height, self.context.window_minimized);

        if width == 0 { width = 1; }
        if height == 0 { height = 1; }

        self.context.wgpu.resize(width, height);
        self.context.egui.resize(width, height, scale_factor);

        {
            let state = &mut *(self.context.state.borrow_mut());

            state.width = width;
            state.height = height;
            state.scale_factor = self.context.window.scale_factor() as f32;

            for scene in &mut state.scenes
            {
                scene.update_resolution(width, height);

                let mut render_item = scene.render_item.take();

                let render_scene = get_render_item_mut::<Scene>(render_item.as_mut().unwrap());
                render_scene.resize(&mut self.context.wgpu, scene);

                scene.render_item = render_item;
            }

            // reset input states
            state.io.input_manager.reset();
        }

        if let Some(app) = &mut self.app
        {
            app.resize(&mut self.context);
        }
    }

    pub fn scene_init(&mut self)
    {
        //init scene
        let state = &mut *(self.context.state.borrow_mut());

        let mut scene = crate::state::scene::scene::Scene::new("main scene");
        scene.add_defaults();
        scene.main = true;

        state.scenes.push(Box::new(scene));
    }

    pub fn app_update(&mut self)
    {
        if let Some(app) = &mut self.app
        {
            // update app
            app.update(&mut self.context);
        }
    }

    pub fn update(&mut self)
    {
        if self.context.window_minimized && !self.app.as_ref().map_or(false, |a| a.allow_window_minimized_updates())
        {
            return;
        }

        // ******************** update states ********************
        {
            let state = &mut *(self.context.state.borrow_mut());
            if let Some(gilrs) = &mut self.gilrs
            {
                gilrs_event(state, gilrs, state.stats.frame);
            }
        }

        let frame_time = Instant::now();

        // ******************** update states ********************
        {
            let state = &mut *(self.context.state.borrow_mut());

            // vsync
            let (v_sync, vsync_changed) = state.rendering.v_sync.consume_clone();
            if vsync_changed
            {
                self.context.wgpu.set_vsync(v_sync);
            }

            // full screen
            let (fullscreen, fullscreen_changed) = state.rendering.fullscreen.consume_clone();
            if fullscreen_changed
            {
                let mut fullscreen_mode = None;
                if fullscreen { fullscreen_mode = Some(Fullscreen::Borderless(None)); }
                self.context.window.set_fullscreen(fullscreen_mode);
            }

            // fps
            let current_time = state.stats.fps_timer.elapsed().as_millis();
            state.stats.fps += 1;

            if current_time / 1000 > state.stats.last_time / 1000 && state.stats.frame_times.len() > 0
            {
                state.stats.last_time = state.stats.fps_timer.elapsed().as_millis();

                // average fps
                state.stats.last_fps = state.stats.fps;
                state.stats.fps_average_chart.push_back(state.stats.last_fps);

                // 1% low fps
                let mut sorted_times: Vec<f32> = state.stats.frame_times.iter().map(|x| *x).collect();
                sorted_times.sort_by(|a, b| b.partial_cmp(a).unwrap());

                let one_percent_index = (sorted_times.len() as f32 * 0.01).ceil() as usize;
                let one_percent_slowest_time = sorted_times.get(one_percent_index.saturating_sub(1)).copied().unwrap_or(*sorted_times.last().unwrap());

                state.stats.last_fps_1_percent_low = (1_000_000.0 / one_percent_slowest_time) as u32;
                state.stats.fps_1_percent_low_chart.push_back(state.stats.last_fps_1_percent_low);

                if state.stats.fps_average_chart.len() > FPS_CHART_VALUES
                {
                    state.stats.fps_average_chart.pop_front();
                    state.stats.fps_1_percent_low_chart.pop_front();
                }

                self.context.window.set_title(format!("{} | FPS: {} (1%L: {})", &self.context.window_title, state.stats.last_fps, state.stats.last_fps_1_percent_low).as_str());
                state.stats.fps = 0;
                state.stats.frame_times.clear();
            }

            // frame scale
            let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_micros();

            if state.stats.frame_update_time > 0 && now - state.stats.frame_update_time > 0
            {
                state.stats.frame_scale = REFERENCE_UPDATE_FRAMES / (1000000.0 / (now - state.stats.frame_update_time) as f32);
            }

            state.stats.frame_update_time = now;
        }

        // ******************** editor/ui update ********************
        {
            let now = Instant::now();
            let state = &mut *(self.context.state.borrow_mut());
            self.editor_gui.update(state, &mut self.context.wgpu, &self.context.egui.ctx);

            state.stats.editor_update_time = now.elapsed().as_micros() as f32 / 1000.0;
        }

        // ******************** build ui ********************
        if self.editor_gui.editor_state.visible
        {
            let now = Instant::now();
            let state = &mut *(self.context.state.borrow_mut());

            let gui_output = self.editor_gui.build_gui(state, &self.context.window, &mut self.context.egui);
            self.context.egui.output = Some(gui_output);

            //self.gui.request_repaint();
            state.stats.egui_update_time = now.elapsed().as_micros() as f32 / 1000.0;
        }

        // ******************** app update ********************

        if !self.context.state.borrow().pause
        {
            let now = Instant::now();
            self.app_update();

            let state = &mut *(self.context.state.borrow_mut());
            state.stats.app_update_time = now.elapsed().as_micros() as f32 / 1000.0;
        }

        // ******************** update main thread queue ********************
        {
            let state = &mut *(self.context.state.borrow_mut());
            let main_queue = state.main_thread_execution_queue.clone();
            ExecutionQueue::run_all(main_queue, state);
        }

        // ******************** update scene and rendering ********************
        if !self.context.state.borrow().pause
        {
            let engine_update_time = Instant::now();

            let state = &mut *(self.context.state.borrow_mut());

            // msaa
            let (msaa_samples, msaa_changed) = state.rendering.msaa.consume_clone();

            if msaa_changed
            {
                self.context.wgpu.create_msaa_texture(msaa_samples);
            }

            state.update(state.stats.frame_update_time, state.stats.frame_scale, state.stats.frame);

            rendering::state::update(&mut self.context.wgpu, state);

            // move out scenes from state to prevent using multiple mut borrows
            let mut scenes = vec![];
            swap(&mut state.scenes, &mut scenes);

            for scene in &mut scenes
            {
                let mut render_item = scene.render_item.take();

                let render_scene = get_render_item_mut::<Scene>(render_item.as_mut().unwrap());

                if msaa_changed
                {
                    render_scene.msaa_sample_size_update(&mut self.context.wgpu, scene, msaa_samples);
                }
                render_scene.update(&mut self.context.wgpu, state, scene);

                scene.render_item = render_item;
            }

            swap(&mut scenes, &mut state.scenes);

            state.stats.engine_update_time = engine_update_time.elapsed().as_micros() as f32 / 1000.0;
        }

        // ******************** render ********************
        let (output, view, msaa_view) = self.context.wgpu.start_render();
        let mut engine_encoder = self.context.wgpu.create_command_encoder();
        let mut egui_encoder = self.context.wgpu.create_command_encoder();
        {
            let state = &mut *(self.context.state.borrow_mut());

            // render scenes
            {
                let engine_render_time = Instant::now();

                state.stats.draw_calls = 0;

                for scene in &mut state.scenes
                {
                    if !scene.visible
                    {
                        continue;
                    }

                    let mut render_item = scene.render_item.take();

                    let render_scene = get_render_item_mut::<Scene>(render_item.as_mut().unwrap());
                    render_scene.distance_sorting = state.rendering.distance_sorting;
                    render_scene.frustum_culling = state.rendering.frustum_culling;
                    render_scene.occlusion_culling = state.rendering.occlusion_culling;

                    // render scene
                    let render_results =  render_scene.render(&mut self.context.wgpu, &view, &msaa_view, &mut engine_encoder, scene);

                    // update visibility info for cameras
                    for (cam_index, cam) in scene.cameras.iter_mut().enumerate()
                    {
                        cam.visible_nodes_last_frame = render_results[cam_index].objects_visible.clone();
                    }

                    // all draw calls
                    state.stats.draw_calls += render_results.iter().map(|r| r.draw_calls).sum::<u32>();

                    // debug highlight visible occlusions
                    if state.debug.highlight_visible_occlusions
                    {
                        let all_highlighted = render_results.iter().flat_map(|r| r.objects_visible.iter()).cloned().collect();
                        highlight_and_unhighlight_scene_meshes(scene, &all_highlighted);
                        console_debug!("Highlighted {} visible occluded objects", all_highlighted.len());
                    }

                    scene.render_item = render_item;
                }

                state.stats.engine_render_time = engine_render_time.elapsed().as_micros() as f32 / 1000.0;
            }

            // render egui
            if self.editor_gui.editor_state.visible
            {
                let now = Instant::now();
                self.context.egui.render(&mut self.context.wgpu, &view, &mut egui_encoder);

                state.stats.egui_render_time = now.elapsed().as_micros() as f32 / 1000.0;
            }
        }
        self.context.wgpu.submit_commands(vec![engine_encoder, egui_encoder]);
        self.context.wgpu.end_render(output);

        // ******************** screenshot ********************
        {
            let state = &mut *(self.context.state.borrow_mut());

            if state.debug.save_screenshot
            {
                let (buffer_dimensions, output_buffer, texture, view, msaa_view) = self.context.wgpu.start_screenshot_render();
                let mut encoder = self.context.wgpu.create_command_encoder();
                {
                    for scene in &mut state.scenes
                    {
                        let mut render_item = scene.render_item.take();

                        let render_scene = get_render_item_mut::<Scene>(render_item.as_mut().unwrap());
                        render_scene.distance_sorting = state.rendering.distance_sorting;
                        render_scene.frustum_culling = state.rendering.frustum_culling;
                        render_scene.occlusion_culling = state.rendering.occlusion_culling;
                        render_scene.render(&mut self.context.wgpu, &view, &msaa_view, &mut encoder, scene);

                        scene.render_item = render_item;
                    }

                    self.context.egui.render(&mut self.context.wgpu, &view, &mut encoder);
                }
                let img_data = self.context.wgpu.end_screenshot_render(buffer_dimensions, output_buffer, texture, encoder);

                img_data.save("data/screenshot.png").unwrap();
                state.debug.save_screenshot = false;
            }
        }

        // ******************** update inputs ********************
        {
            let state = &mut *(self.context.state.borrow_mut());
            state.io.input_manager.update();
        }

        // ******************** mouse visibility ********************
        {
            let state = &mut *(self.context.state.borrow_mut());
            let (visible, changed) = state.io.input_manager.mouse.visible.consume_borrow();
            if changed
            {
                self.context.window.set_cursor_visible(*visible);

                if *visible
                {
                    _ = self.context.window.set_cursor_grab(CursorGrabMode::None);
                }
                else
                {
                    self.context.window.set_cursor_grab(CursorGrabMode::Confined)
                    .or_else(|_e| self.context.window.set_cursor_grab(CursorGrabMode::Locked))
                    .unwrap_or_else(|e|
                    {
                        console_error!("Failed to grab position: {:?}", e);
                    });
                }
            }
        }

        // ******************** reset global change tracker ********************
        {
            let state = &mut *(self.context.state.borrow_mut());
            state.io.audio_device.write().unwrap().data.consume_change();
        }

        // ******************** frame time ********************
        {
            let state = &mut *(self.context.state.borrow_mut());
            state.stats.frame_time = frame_time.elapsed().as_micros() as f32 / 1000.0;
            state.stats.frame_times.push_back(frame_time.elapsed().as_micros() as f32);

            state.stats.fps_cpu_absolute = (1000.0 / (state.stats.engine_render_time + state.stats.engine_update_time)) as u32;

            // frame update
            state.stats.frame += 1;
        }
    }

    pub fn exit(&mut self)
    {
        if let Some(app) = &mut self.app
        {
            app.exit(&mut self.context);
        }
    }

    pub fn check_exit(&mut self) -> bool
    {
        self.context.state.borrow().exit
    }

    pub fn request_exit(&mut self) -> bool
    {
        if let Some(app) = &mut self.app
        {
            return app.request_exit(&mut self.context);
        }

        true
    }

    pub fn window_input(&mut self, event: &winit::event::WindowEvent)
    {
        if self.editor_gui.editor_state.visible && self.context.egui.on_event(event, self.context.window.clone())
        {
            return;
        }
        else
        {
            let global_state = &mut *(self.context.state.borrow_mut());
            //let main_queue = global_state.main_thread_execution_queue.clone();

            match event
            {
                winit::event::WindowEvent::KeyboardInput { device_id: _, event, is_synthetic: _ } =>
                {
                    let key = winit_map_key(&event.logical_key, event.location);

                    if event.state == ElementState::Pressed
                    {
                        global_state.io.input_manager.keyboard.set_key(key, true, global_state.stats.frame);
                    }
                    else
                    {
                        global_state.io.input_manager.keyboard.set_key(key, false, global_state.stats.frame);
                    }
                },
                winit::event::WindowEvent::ModifiersChanged(modifiers_state) =>
                {
                    // TODO: Check if windows/linux is able to catch left/right difference
                    if is_mac()
                    {
                        global_state.io.input_manager.keyboard.set_modifier(Modifier::LeftAlt, modifiers_state.lalt_state() == ModifiersKeyState::Pressed, global_state.stats.frame);
                        global_state.io.input_manager.keyboard.set_modifier(Modifier::RightAlt, modifiers_state.ralt_state() == ModifiersKeyState::Pressed, global_state.stats.frame);

                        global_state.io.input_manager.keyboard.set_modifier(Modifier::LeftCtrl, modifiers_state.lcontrol_state() == ModifiersKeyState::Pressed, global_state.stats.frame);
                        global_state.io.input_manager.keyboard.set_modifier(Modifier::RightCtrl, modifiers_state.rcontrol_state() == ModifiersKeyState::Pressed, global_state.stats.frame);

                        global_state.io.input_manager.keyboard.set_modifier(Modifier::LeftLogo, modifiers_state.lsuper_state() == ModifiersKeyState::Pressed, global_state.stats.frame);
                        global_state.io.input_manager.keyboard.set_modifier(Modifier::RightLogo, modifiers_state.rsuper_state() == ModifiersKeyState::Pressed, global_state.stats.frame);

                        global_state.io.input_manager.keyboard.set_modifier(Modifier::LeftShift, modifiers_state.lshift_state() == ModifiersKeyState::Pressed, global_state.stats.frame);
                        global_state.io.input_manager.keyboard.set_modifier(Modifier::RightShift, modifiers_state.rshift_state() == ModifiersKeyState::Pressed, global_state.stats.frame);
                    }
                    else
                    {
                        global_state.io.input_manager.keyboard.set_modifier(Modifier::LeftAlt, modifiers_state.state().alt_key(), global_state.stats.frame);
                        global_state.io.input_manager.keyboard.set_modifier(Modifier::RightAlt, modifiers_state.state().alt_key(), global_state.stats.frame);

                        global_state.io.input_manager.keyboard.set_modifier(Modifier::LeftCtrl, modifiers_state.state().control_key(), global_state.stats.frame);
                        global_state.io.input_manager.keyboard.set_modifier(Modifier::RightCtrl, modifiers_state.state().control_key(), global_state.stats.frame);

                        global_state.io.input_manager.keyboard.set_modifier(Modifier::LeftLogo, modifiers_state.state().super_key(), global_state.stats.frame);
                        global_state.io.input_manager.keyboard.set_modifier(Modifier::RightLogo, modifiers_state.state().super_key(), global_state.stats.frame);

                        global_state.io.input_manager.keyboard.set_modifier(Modifier::LeftShift, modifiers_state.state().shift_key(), global_state.stats.frame);
                        global_state.io.input_manager.keyboard.set_modifier(Modifier::RightShift, modifiers_state.state().shift_key(), global_state.stats.frame);
                    }
                },
                winit::event::WindowEvent::MouseInput { device_id: _, state, button, .. } =>
                {
                    let pressed;
                    match state
                    {
                        ElementState::Pressed => pressed = true,
                        ElementState::Released => pressed = false,
                    }

                    let button = winit_map_mouse_button(button);

                    global_state.io.input_manager.mouse.set_button(button, pressed, global_state.stats.frame);
                },
                winit::event::WindowEvent::MouseWheel { device_id: _, delta, phase: _, ..} =>
                {
                    match delta
                    {
                        winit::event::MouseScrollDelta::LineDelta(x, y) =>
                        {
                            global_state.io.input_manager.mouse.set_wheel_delta_x(*x);
                            global_state.io.input_manager.mouse.set_wheel_delta_y(*y);
                        },
                        winit::event::MouseScrollDelta::PixelDelta(delta) =>
                        {
                            global_state.io.input_manager.mouse.set_wheel_delta_y(delta.x as f32);
                            global_state.io.input_manager.mouse.set_wheel_delta_y(delta.y as f32);
                        },
                    }
                },
                winit::event::WindowEvent::CursorMoved { device_id: _, position, ..} =>
                {
                    let mut pos = Point2::<f32>::new(position.x as f32, position.y as f32);

                    // invert pos (because x=0, y=0 is bottom left and "normal" window is top left)
                    pos.y = global_state.height as f32 - pos.y;

                    global_state.io.input_manager.mouse.set_pos(pos, global_state.stats.frame, global_state.width, global_state.height);
                },
                winit::event::WindowEvent::Touch(touch) =>
                {
                    let mut pos = Point2::<f32>::new(touch.location.x as f32, touch.location.y as f32);

                    // invert pos (because x=0, y=0 is bottom left and "normal" window is top left)
                    pos.y = global_state.height as f32 - pos.y;

                    let mut force = None;
                    if let Some(touch_force) = touch.force
                    {
                        force = Some(touch_force.normalized() as f32);
                    }

                    let state = match touch.phase
                    {
                        winit::event::TouchPhase::Started => PointState::Down,
                        winit::event::TouchPhase::Moved => PointState::Move,
                        winit::event::TouchPhase::Ended => PointState::Up,
                        winit::event::TouchPhase::Cancelled => PointState::Up,
                    };

                    global_state.io.input_manager.touch.set(touch.id, pos, state, global_state.stats.frame, force);
                },
                winit::event::WindowEvent::Focused(focus) =>
                {
                    global_state.in_focus = *focus;
                    global_state.io.input_manager.reset();
                },
                winit::event::WindowEvent::DroppedFile(path) =>
                {
                    if let Some(path) = path.to_str()
                    {
                        self.editor_gui.apply_external_asset_drag(global_state, path.to_string());
                        self.context.window.request_redraw();
                    }
                },
                _ => {}
            }
        }
    }

    pub fn device_input(&mut self, event: &winit::event::DeviceEvent)
    {
        let global_state = &mut *(self.context.state.borrow_mut());

        match event
        {
            winit::event::DeviceEvent::MouseMotion { delta } =>
            {
                let velocity = Vector2::<f32>::new(delta.0 as f32, -delta.1 as f32);
                global_state.io.input_manager.mouse.set_raw_velocity(velocity, global_state.stats.frame);
            },
            _ => {}
        }
    }

    pub fn update_done(&mut self)
    {
        let global_state = &mut *(self.context.state.borrow_mut());

        // center mouse (needed on windows)
        if !*global_state.io.input_manager.mouse.visible.get_ref() && !is_mac()
        {
            let window_size = self.context.window.inner_size();
            let center = PhysicalPosition::new(window_size.width as f64 / 2.0, window_size.height as f64 / 2.0);

            self.context.window.set_cursor_position(center).unwrap_or_else(|e|
            {
                console_error!("Failed to set mouse position: {:?}", e);
            });
        }
    }
}