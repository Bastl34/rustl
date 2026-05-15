#![allow(dead_code)]

use std::{env, sync::{Arc, RwLock}};
use web_time::Instant;

use image::{ImageFormat, EncodableLayout};
use nalgebra::{Point2, Point3, Vector3};

use crate::{console_log, gui::editor::{editor_project::EditorProjectData, helper::apply_fly_camera_move_state, recent_projects::RecentProjectsData, settings::EditorSettings}, helper::{console_log::LogType, file::{get_extension, get_stem}, math::approx_equal}, rendering::{self, texture::Texture}, resources::resources::{exists, load_binary, read_files_recursive}, state::{helper::render_item::get_render_item, scene::{components::transformation::TransformationData, node::NodeItem, scene::Scene}, state::State}};

const THUMB_EXTENSION: &str = "png";
const THUMB_SUFFIX_NAME: &str = "_thumb.png";

const OBJECTS_DIR: &str = "objects/";
const SCENES_DIR: &str = "scenes/";

const LOCAL_OBJECTS_DIR: &str = "resourcesLocal/objects/";
const LOCAL_SCENES_DIR: &str = "resourcesLocal/scenes/";

const DEFAULT_GRID_SIZE: f32 = 0.25;
const DEFAULT_GRID_AMOUNT: u32 = 1500;

#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub enum GizmoTypeAndAxis
{
    TranslateX,
    TranslateY,
    TranslateZ,
    TranslateXY,
    TranslateXZ,
    TranslateYZ,
    RotateX,
    RotateY,
    RotateZ,
    ScaleX,
    ScaleY,
    ScaleZ,
    ScaleUniform,
}

#[derive(PartialEq, Eq, Debug)]
pub enum SettingsPanel
{
    Components,
    Material,
    Camera,
    Texture,
    Sound,
    SoundSource,
    Light,
    Scene,
    Object,
    General,
    Resources,
    MeshResource,
    Project
}

#[derive(PartialEq, Eq)]
pub enum SelectionType
{
    Object,
    Camera,
    Light,
    Material,
    Texture,
    Sound,
    SoundSource,
    MeshResource,
    None
}

#[derive(PartialEq, Eq)]
pub enum PickType
{
    Camera,
    Parent,
    AnimationCopy,
    Texture,
    None
}

#[derive(PartialEq, Eq)]
pub enum BottomPanel
{
    Assets,
    Console,
    Debug,
    None
}

#[derive(PartialEq, Eq)]
pub enum DebugPanel
{
    SceneDebugging,
    DepthPassImage,
    DepthBufferImage,
    HzbImage,
}

#[derive(PartialEq, Eq)]
pub enum AssetType
{
    Scene,
    Object,
    Texture,
    Material
}

#[derive(Clone, Copy)]
pub enum EditMode
{
    Movement(Point2::<f32>, bool, bool, bool),
    Rotate(Point2::<f32>, bool, bool, bool)
}

pub struct Asset
{
    pub name: String,
    pub path: String,
    pub preview: Option<String>,
    pub egui_preview: Option<egui::TextureHandle>,
}

pub struct DebugImages
{
    pub depth_pass_image: Option<egui::TextureHandle>,
    pub depth_buffer_image: Option<egui::TextureHandle>,
    pub hzb_image: Option<egui::TextureHandle>,
}

pub struct LoadingGuard(pub Arc<RwLock<bool>>);

impl Drop for LoadingGuard
{
    fn drop(&mut self)
    {
        *self.0.write().unwrap() = false;
    }
}

pub struct EditorState
{
    pub visible: bool,
    pub loading: Arc<RwLock<bool>>,
    pub loading_progress: Arc<RwLock<f32>>,

    pub try_mode: bool,
    pub selectable: bool,
    pub fly_camera: bool,

    pub quad_view: bool,

    pub left_panel_open: bool,
    pub right_panel_open: bool,
    pub bottom_panel_open: bool,

    pub recent_projects: RecentProjectsData,
    pub settings: EditorSettings,

    pub project_data: EditorProjectData,
    pub project_path: Option<String>,
    pub project_session_start: Instant,

    pub gizmo_position: bool,
    pub gizmo_rotation: bool,
    pub gizmo_scale: bool,

    pub use_highlight: bool,
    pub show_internal_entries: bool,

    pub pick_id: String,
    pub pick_mode: PickType,

    pub grid_size: f32,
    pub grid_amount: u32,
    pub grid_recreate: bool,

    pub edit_mode: Option<EditMode>,
    pub edit_moving: bool,
    pub drag_and_drop_grid_only: bool,

    pub bottom: BottomPanel,
    pub asset_type: AssetType,
    pub debug_panel: DebugPanel,
    pub log_type: LogType,

    pub settings_panel: SettingsPanel,

    pub hierarchy_filter: String,

    pub hierarchy_multi_select: Vec<u32>,
    pub hierarchy_last_click_id: Option<u32>,
    pub hierarchy_flat_nodes_order: Vec<u32>, // flat list of all node ids in the hierarchy, used for shift+click range selection
    pub hierarchy_rename_id: Option<(String, u32)>,
    pub hierarchy_rename_value: String,
    pub hierarchy_rename_click_id: Option<u32>,
    pub hierarchy_rename_click_time: Option<Instant>,

    pub component_filter: String,

    pub tag_input: String,

    pub selected_scene_id: Option<u32>,
    pub selected_type: SelectionType,
    pub selected_object: String,
    pub selected_object_position: Option<Vector3<f32>>,
    pub selected_gizmo: Option<GizmoTypeAndAxis>,
    pub selected_object_gizmo_value: Option<Vector3<f32>>,

    pub last_hover_object: Option<String>,
    pub last_hover_pointer_position: Option<Point3<f32>>,
    pub last_hover_check: Instant,

    pub copy_asset: Option<String>,
    pub copy_asset_transform: Option<TransformationData>,
    pub copy_node_id: Arc<RwLock<Option<u32>>>,
    pub copy_node_name: Option<String>,

    pub drag_id: Option<String>,

    pub dialog_add_component: bool,
    pub add_component_id: usize,
    pub add_component_name: String,

    pub dialog_add_camera_controller: bool,
    pub add_camera_controller_id: usize,

    pub dialog_add_scene_controller: bool,
    pub add_scene_controller_id: usize,
    pub add_scene_controller_post: bool,

    pub dialog_debug_image: bool,
    pub dialog_debug_image_id: Option<egui::TextureHandle>,

    pub dialog_settings: bool,

    pub dialog_help_shortcuts: bool,
    pub dialog_about: bool,
    pub dialog_splash: bool,

    pub asset_filter: String,
    pub reuse_materials_by_name: bool,
    pub assets_objects: Vec<Asset>,
    pub assets_scenes: Vec<Asset>,

    pub log_filter: String,
    pub log_auto_scroll: bool,

    pub debug_images: DebugImages,

    pub highlighted_gizmo_id: Option<u32>,

    pub open_scene_tabs: Vec<u32>,
}

impl EditorState
{
    pub fn new() -> EditorState
    {
        let mut state = EditorState
        {
            visible: true,
            loading: Arc::new(RwLock::new(false)),
            loading_progress: Arc::new(RwLock::new(0.0)),

            try_mode: false,
            selectable: true,
            fly_camera: true,

            quad_view: false,

            left_panel_open: true,
            right_panel_open: true,
            bottom_panel_open: true,

            recent_projects: RecentProjectsData::new(),
            settings: EditorSettings::new(),

            project_data: EditorProjectData::default(),
            project_path: None,
            project_session_start: Instant::now(),

            gizmo_position: true,
            gizmo_rotation: false,
            gizmo_scale: false,

            use_highlight: true,
            show_internal_entries: false,

            pick_id: "".to_string(),
            pick_mode: PickType::None,

            grid_size: DEFAULT_GRID_SIZE,
            grid_amount: DEFAULT_GRID_AMOUNT,
            grid_recreate: false,

            edit_mode: None,
            edit_moving: false,
            drag_and_drop_grid_only: false,

            bottom: BottomPanel::Assets,
            asset_type: AssetType::Object,
            debug_panel: DebugPanel::SceneDebugging,
            log_type: LogType::All,

            settings_panel: SettingsPanel::General,

            hierarchy_filter: String::new(),

            hierarchy_multi_select: vec![],
            hierarchy_last_click_id: None,
            hierarchy_flat_nodes_order: vec![],
            hierarchy_rename_id: None,
            hierarchy_rename_value: String::new(),
            hierarchy_rename_click_id: None,
            hierarchy_rename_click_time: None,

            component_filter: String::new(),

            tag_input: String::new(),

            selected_scene_id: None,
            selected_type: SelectionType::None,
            selected_object: String::new(), // type_nodeID/elementID_instanceID
            selected_object_position: None,
            selected_gizmo: None,
            selected_object_gizmo_value: None,

            last_hover_object: None,
            last_hover_pointer_position: None,
            last_hover_check: Instant::now(),

            copy_asset: None,
            copy_asset_transform: None,
            copy_node_id: Arc::new(RwLock::new(None)),
            copy_node_name: None,

            drag_id: None,

            dialog_add_component: false,
            add_component_id: 0,
            add_component_name: "Component".to_string(),

            dialog_add_camera_controller: false,
            add_camera_controller_id: 0,

            dialog_add_scene_controller: false,
            add_scene_controller_id: 0,
            add_scene_controller_post: false,

            dialog_debug_image: false,
            dialog_debug_image_id: None,

            dialog_settings: false,

            dialog_help_shortcuts: false,
            dialog_about:false,
            dialog_splash: false,

            asset_filter: "".to_string(),
            reuse_materials_by_name: true,
            assets_objects: vec![],
            assets_scenes: vec![],

            log_filter: "".to_string(),
            log_auto_scroll: true,

            debug_images: DebugImages
            {
                depth_pass_image: None,
                depth_buffer_image: None,
                hzb_image: None,
            },
            highlighted_gizmo_id: None,
            open_scene_tabs: vec![],
        };

        state.reset_project();

        state
    }

    pub fn reset_project(&mut self)
    {
        self.project_data = EditorProjectData::default();
        self.project_path = None;
        self.project_session_start = Instant::now();
    }

    pub fn accumulate_editing_time(&mut self)
    {
        let elapsed = self.project_session_start.elapsed().as_secs();
        self.project_data.editing_time_secs += elapsed;
        self.project_session_start = Instant::now();
    }

    pub fn update_debug_images(&mut self, state: &mut State, wgpu: &mut rendering::wgpu::WGpu, egui_context: &egui::Context)
    {
        // ******************** depth pass image ********************
        if state.debug.show_depth_pass_image.is_some()
        {
            let scene = self.get_debug_scene(state);

            if let Some(scene) = scene
            {
                if let Some(ref render_item_box) = scene.render_item
                {
                    let scene_render_scene = get_render_item::<rendering::scene::Scene>(render_item_box);

                    let image = scene_render_scene.depth_pass_buffer_texture.to_image(wgpu, None);
                    let size = [image.width() as usize, image.height() as usize];
                    let pixels = image.to_rgba8().into_raw();
                    let color = egui::ColorImage::from_rgba_unmultiplied(size, pixels.as_slice());

                    match self.debug_images.depth_pass_image.as_mut()
                    {
                        Some(handle) =>
                        {
                            handle.set(color, Default::default());
                        }
                        None =>
                        {
                            let tex = egui_context.load_texture("depth_pass_image", color, Default::default());
                            self.debug_images.depth_pass_image = Some(tex);
                        }
                    }
                }
            }
        }
        else
        {
            self.debug_images.depth_pass_image = None;
        }

        // ******************** depth buffer image ********************
        if state.debug.show_depth_buffer_image.is_some()
        {
            let scene = self.get_debug_scene(state);

            if let Some(scene) = scene
            {
                if let Some(ref render_item_box) = scene.render_item
                {
                    let scene_render_scene = get_render_item::<rendering::scene::Scene>(render_item_box);

                    let image = scene_render_scene.depth_buffer_texture.to_image(wgpu, None);
                    let size = [image.width() as usize, image.height() as usize];
                    let pixels = image.to_rgba8().into_raw();
                    let color = egui::ColorImage::from_rgba_unmultiplied(size, pixels.as_slice());

                    match self.debug_images.depth_buffer_image.as_mut()
                    {
                        Some(handle) =>
                        {
                            handle.set(color, Default::default());
                        }
                        None =>
                        {
                            let tex = egui_context.load_texture("depth_buffer_image", color, Default::default());
                            self.debug_images.depth_buffer_image = Some(tex);
                        }
                    }
                }
            }
        }
        else
        {
            self.debug_images.depth_buffer_image = None;
        }

        // ******************** hzb image ********************
        if let Some(show_hzb_image_mip) = state.debug.show_hzb_image
        {
            let scene = self.get_debug_scene(state);

            if let Some(scene) = scene
            {
                if let Some(cam) = scene.get_active_camera()
                {
                    if let Some(ref render_item_box) = cam.hzb_texture_render_item
                    {
                        let hzb_texture = get_render_item::<Texture>(render_item_box);

                        let image = hzb_texture.to_image(wgpu, Some(show_hzb_image_mip));
                        let size = [image.width() as usize, image.height() as usize];
                        let pixels = image.to_rgba8().into_raw();
                        let color = egui::ColorImage::from_rgba_unmultiplied(size, pixels.as_slice());

                        match self.debug_images.hzb_image.as_mut()
                        {
                            Some(handle) =>
                            {
                                handle.set(color, egui::TextureOptions::NEAREST);
                            }
                            None =>
                            {
                                let tex = egui_context.load_texture("hzb_image", color, egui::TextureOptions::NEAREST);
                                self.debug_images.hzb_image = Some(tex);
                            }
                        }
                    }
                }

            }
        }
        else
        {
            self.debug_images.hzb_image = None;
        }
    }

    pub fn set_grid_size(&mut self, size: f32)
    {
        if approx_equal(self.grid_size, size)
        {
            return;
        }

        let new_amount = (DEFAULT_GRID_SIZE / size) * DEFAULT_GRID_AMOUNT as f32;
        self.grid_size = size;
        self.grid_amount = new_amount.round() as u32;

        self.grid_recreate = true;
    }

    pub fn get_object_ids(&self) -> (Option<u32>, Option<u32>)
    {
        // no scene selected
        if self.selected_scene_id == None && self.selected_object.is_empty()
        {
            return (None, None);
        }

        let parts: Vec<&str> = self.selected_object.split('_').collect();

        let mut item_id: Option<u32> = None;
        let mut subitem_id: Option<u32> = None; // like instance id

        if parts.len() >= 2
        {
            item_id = Some(parts.get(1).unwrap().parse().unwrap());
        }

        if parts.len() >= 3
        {
            subitem_id = Some(parts.get(2).unwrap().parse().unwrap());
        }

        (item_id, subitem_id)
    }

    pub fn get_selected_node_id(&self) -> Option<u32>
    {
        let (node_id, _) = self.get_object_ids();

        node_id
    }

    pub fn get_debug_scene<'a>(&'a self, state: &'a mut State) -> Option<&'a mut Box<Scene>>
    {
        if let Some(scene_id) = self.selected_scene_id
        {
            state.find_scene_by_id_mut(scene_id)
        }
        else
        {
            let main_scene_id = state.scenes.iter().position(|s| s.active).or_else(|| if state.scenes.is_empty() { None } else { Some(0) });
            main_scene_id.and_then(|i| state.scenes.get_mut(i))
        }
    }

    pub fn get_selected_node<'a>(&'a mut self, state: &'a mut State) -> (Option<&'a mut Box<Scene>>, Option<NodeItem>, Option<u32>)
    {
        let (node_id, instance_id) = self.get_object_ids();

        if self.selected_type != SelectionType::Object || self.selected_scene_id.is_none() || node_id.is_none()
        {
            return (None, None, None);
        }

        let scene_id: u32 = self.selected_scene_id.unwrap();
        let node_id: u32 = node_id.unwrap();

        let scene = state.find_scene_by_id_mut(scene_id);

        if scene.is_none()
        {
            return (None, None, None);
        }

        let scene = scene.unwrap();

        let node = scene.find_node_by_id(node_id);

        if node.is_none()
        {
            return (None, None, None);
        }

        let node = node.unwrap();

        (Some(scene), Some(node.clone()), instance_id)
    }

    pub fn get_selected_scene<'a>(&'a mut self, state: &'a mut State) -> Option<&'a mut Box<Scene>>
    {
        if  self.selected_scene_id.is_none()
        {
            return None;
        }

        let scene_id: u32 = self.selected_scene_id.unwrap();

        state.find_scene_by_id_mut(scene_id)
    }

    pub fn remove_highlight(&self, state: &mut State)
    {
        for scene in &mut state.scenes
        {
            for node in &scene.nodes
            {
                let mut all_nodes = vec![];
                all_nodes.push(node.clone());
                all_nodes.extend(Scene::list_all_child_nodes(&node.read().unwrap().nodes));

                for node in all_nodes
                {
                    let node = node.read().unwrap();
                    for instance in node.instances.get_ref()
                    {
                        let mut instance = instance.write().unwrap();
                        let instance_data = instance.get_data_mut().get_mut();
                        instance_data.highlight = false;
                    }
                }
            }
        }
    }

    pub fn apply_highlight(&mut self, state: &mut State)
    {
        let (scene, node, instance_id) = self.get_selected_node(state);

        if scene.is_none() || node.is_none()
        {
            return;
        }

        self.apply_highlight_for_node(&node.unwrap(), instance_id);
    }

    pub fn apply_highlight_for_node(&mut self, node: &NodeItem, instance_id: Option<u32>)
    {
        if let Some(instance_id) = instance_id
        {
            let node = node.read().unwrap();
            if let Some(instance) = node.find_instance_by_id(instance_id)
            {
                let mut instance = instance.write().unwrap();
                let instance_data = instance.get_data_mut().get_mut();
                instance_data.highlight = true;
            }
        }
        else
        {
            let mut all_nodes = vec![];
            all_nodes.push(node.clone());
            all_nodes.extend(Scene::list_all_child_nodes(&node.read().unwrap().nodes));

            for node in all_nodes
            {
                let node = node.read().unwrap();

                for instance in node.instances.get_ref()
                {
                    let mut instance = instance.write().unwrap();
                    let instance_data = instance.get_data_mut().get_mut();
                    instance_data.highlight = true;
                }
            }
        }
    }


    pub fn de_select_all_items(state: &mut State, predicate: Option<Arc<dyn Fn(NodeItem) -> bool + Send + Sync>>)
    {
        for scene in &mut state.scenes
        {
            // enable camera movement again
            apply_fly_camera_move_state(scene, true);

            for node in &scene.nodes
            {
                let mut all_nodes = vec![];
                all_nodes.push(node.clone());
                all_nodes.extend(Scene::list_all_child_nodes(&node.read().unwrap().nodes));

                for node in all_nodes
                {
                    if let Some(predicate) = &predicate
                    {
                        if !predicate(node.clone())
                        {
                            continue;
                        }
                    }

                    let node = node.read().unwrap();
                    for instance in node.instances.get_ref()
                    {
                        let mut instance = instance.write().unwrap();
                        let instance_data = instance.get_data_mut().get_mut();
                        instance_data.highlight = false;
                    }
                }
            }
        }
    }

    pub fn apply_highlight_for_node_ids(state: &mut State, node_ids: &Vec<u32>)
    {
        // first clear all existing highlights
        for scene in &mut state.scenes
        {
            for node in &scene.nodes
            {
                let mut all_nodes = vec![];
                all_nodes.push(node.clone());
                all_nodes.extend(Scene::list_all_child_nodes(&node.read().unwrap().nodes));

                for node in all_nodes
                {
                    let node = node.read().unwrap();
                    for instance in node.instances.get_ref()
                    {
                        if !instance.read().unwrap().get_data().highlight
                        {
                            continue;
                        }

                        let mut instance = instance.write().unwrap();
                        let instance_data = instance.get_data_mut().get_mut();
                        instance_data.highlight = false;
                    }
                }
            }
        }

        for scene in &state.scenes
        {
            for node_id in node_ids
            {
                if let Some(node) = scene.find_node_by_id(*node_id)
                {
                    let mut all_nodes = vec![];
                    all_nodes.push(node.clone());
                    all_nodes.extend(Scene::list_all_child_nodes(&node.read().unwrap().nodes));

                    for node in all_nodes
                    {
                        let node = node.read().unwrap();
                        for instance in node.instances.get_ref()
                        {
                            if instance.read().unwrap().get_data().highlight
                            {
                                continue;
                            }

                            let mut instance = instance.write().unwrap();
                            let instance_data = instance.get_data_mut().get_mut();
                            instance_data.highlight = true;
                        }
                    }
                }
            }
        }
    }

    pub fn de_select_current_item(&mut self, state: &mut State)
    {
        if self.selected_scene_id == None
        {
            return;
        }

        let scene_id = self.selected_scene_id.unwrap();

        for scene in &mut state.scenes
        {
            if scene_id != scene.id
            {
                continue;
            }

            let (node_id, _deselect_instance_id) = self.get_object_ids();
            if let Some(node_id) = node_id
            {
                if let Some(node) = scene.find_node_by_id(node_id)
                {
                    let mut all_nodes = vec![];
                    all_nodes.push(node.clone());
                    all_nodes.extend(Scene::list_all_child_nodes(&node.read().unwrap().nodes));

                    for node in all_nodes
                    {
                        let node = node.read().unwrap();
                        for instance in node.instances.get_ref()
                        {
                            let mut instance = instance.write().unwrap();
                            let instance_data = instance.get_data_mut().get_mut();
                            instance_data.highlight = false;
                        }
                    }
                }
            }

            // enable camera movement again
            apply_fly_camera_move_state(scene, true);
        }

        self.selected_object.clear();
        self.selected_scene_id = None;
        self.selected_type = SelectionType::None;
        self.selected_gizmo = None;
    }

    pub fn de_select_current_item_from_scene(&mut self, scene: &mut Scene)
    {
        if self.selected_scene_id == None
        {
            return;
        }

        let (node_id, _deselect_instance_id) = self.get_object_ids();
        if let Some(node_id) = node_id
        {
            if let Some(node) = scene.find_node_by_id(node_id)
            {
                let mut all_nodes = vec![];
                all_nodes.push(node.clone());
                all_nodes.extend(Scene::list_all_child_nodes(&node.read().unwrap().nodes));

                for node in all_nodes
                {
                    let node = node.read().unwrap();
                    for instance in node.instances.get_ref()
                    {
                        let mut instance = instance.write().unwrap();
                        let instance_data = instance.get_data_mut().get_mut();
                        instance_data.highlight = false;
                    }
                }
            }
        }

        self.selected_object.clear();
        self.selected_scene_id = None;
        self.selected_type = SelectionType::None;
        self.selected_gizmo = None;

        // enable camera movement again
        apply_fly_camera_move_state(scene, true);
    }

    pub fn set_selected_object(&mut self, scene: &mut Scene, node_id: u32, instance_id: Option<u32>, selection_type: SelectionType, highlight: bool) -> bool
    {
        let scene_id = scene.id;

        let node = scene.find_node_by_id(node_id);
        if node.is_none()
        {
            return false;
        }
        let node = node.unwrap();

        let id_string;
        {
            if let Some(instance_id) = instance_id
            {
                id_string = format!("objects_{}_{}", node_id, instance_id);
            }
            else
            {
                id_string = format!("objects_{}", node_id);
            }
        }

        let mut already_selected = false;

        if self.selected_object == id_string && self.selected_scene_id == Some(scene_id)
        {
            already_selected = true;
        }

        // de-select first
        self.de_select_current_item_from_scene(scene);

        if !already_selected
        {
            self.selected_object = id_string;
            self.selected_scene_id = Some(scene_id);
            self.selected_type = selection_type;

            if highlight
            {
                self.apply_highlight_for_node(&node, instance_id);
            }

            return true;
        }

        false
    }

    pub fn set_try_mode(&mut self, state: &mut State, try_out: bool)
    {
        self.try_mode = try_out;
        self.visible = !try_out;
        state.rendering.fullscreen.set(try_out);
        state.io.input_manager.mouse.visible.set(!try_out);

        if try_out
        {
            self.de_select_current_item(state);
        }
    }

    pub fn load_all_asset_entries(&mut self, state: &mut State, egui_context: &egui::Context)
    {
        // project
        self.load_asset_entries(SCENES_DIR, state, AssetType::Scene, egui_context, false);
        self.load_asset_entries(OBJECTS_DIR, state, AssetType::Object, egui_context, false);

        // local
        let local_objects_dir = env::current_dir().unwrap().join(LOCAL_OBJECTS_DIR);
        let local_objects_dir = local_objects_dir.to_string_lossy().to_string();

        let local_scenes_dir = env::current_dir().unwrap().join(LOCAL_SCENES_DIR);
        let local_scenes_dir = local_scenes_dir.to_string_lossy().to_string();

        self.load_asset_entries(local_objects_dir.as_str(), state, AssetType::Object, egui_context, true);
        self.load_asset_entries(local_scenes_dir.as_str(), state, AssetType::Scene, egui_context, true);
    }

    pub fn load_asset_entries(&mut self, path: &str, state: &State, asset_type: AssetType, egui_context: &egui::Context, append: bool)
    {
        let files = read_files_recursive(path);

        console_log!("loading assets ({}): {}", path, files.len());

        // filter supported file types
        let files: Vec<String> = files.iter().filter(|item|
        {
            let extension = get_extension(item.as_str());
            state.supported_file_types.objects.contains(&extension)
        }).map(|s| s.to_string()).collect();


        let mut assets = vec![];

        for file in &files
        {
            let extension = get_extension(file);
            let extension = format!(".{}", extension);

            let thumb_path = file.replace(extension.as_str(), THUMB_SUFFIX_NAME);

            let mut thumb = None;
            let mut egui_preview = None;

            if exists(thumb_path.as_str())
            {
                let image_bytes = load_binary(thumb_path.as_str()).unwrap();

                let format = ImageFormat::from_extension(THUMB_EXTENSION).unwrap();
                let image: image::DynamicImage = image::load_from_memory_with_format(image_bytes.as_slice(), format).unwrap();
                let image = image.to_rgba8();

                let image = egui::ColorImage::from_rgba_unmultiplied([image.width() as usize, image.height() as usize],image.as_bytes());

                let handle = egui_context.load_texture(thumb_path.clone(), image, Default::default());

                thumb = Some(thumb_path);
                egui_preview = Some(handle);
            }

            let asset = Asset
            {
                name: get_stem(file),
                path: file.to_string(),
                preview: thumb,
                egui_preview: egui_preview,
            };

            assets.push(asset);
        }

        if asset_type == AssetType::Scene
        {
            if append
            {
                self.assets_scenes.extend(assets);
            }
            else
            {
                self.assets_scenes = assets;
            }
        }
        else if asset_type == AssetType::Object
        {
            if append
            {
                self.assets_objects.extend(assets);
            }
            else
            {
                self.assets_objects = assets;
            }
        }
    }
}
