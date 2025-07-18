#![allow(dead_code)]

use std::sync::{RwLock, Arc};

use image::{ImageFormat, EncodableLayout};
use nalgebra::{Point2, Vector3};

use crate::{helper::{file::{get_extension, get_stem}, math::approx_equal}, rendering::egui::EGui, resources::resources::{exists, load_binary, read_files_recursive}, state::{gui::editor::helper::apply_fly_camera_move_state, scene::{components::transformation::TransformationData, node::NodeItem, scene::Scene}, state::State}};

const THUMB_EXTENSION: &str = "png";
const THUMB_SUFFIX_NAME: &str = "_thumb.png";

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
    MeshResource
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
    Debug,
    Console,
    None
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

pub struct EditorState
{
    pub visible: bool,
    pub loading: Arc<RwLock<bool>>,

    pub try_mode: bool,
    pub selectable: bool,
    pub fly_camera: bool,

    pub project_name: String,

    pub gizmo_position: bool,
    pub gizmo_rotation: bool,
    pub gizmo_scale: bool,

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

    pub settings: SettingsPanel,

    pub hierarchy_expand_all: bool,
    pub hierarchy_filter: String,
    pub show_internal_entries: bool,

    pub component_filter: String,

    pub tag_input: String,

    pub selected_scene_id: Option<u64>,
    pub selected_type: SelectionType,
    pub selected_object: String,
    pub selected_object_position: Option<Vector3<f32>>,
    pub selected_gizmo: Option<GizmoTypeAndAxis>,
    pub selected_object_gizmo_value: Option<Vector3<f32>>,

    pub copy_asset: Option<String>,
    pub copy_asset_transform: Option<TransformationData>,
    pub copy_node_id: Arc<RwLock<Option<u64>>>,

    pub drag_id: Option<String>,

    pub dialog_add_component: bool,
    pub add_component_id: usize,
    pub add_component_name: String,

    pub dialog_add_camera_controller: bool,
    pub add_camera_controller_id: usize,

    pub dialog_add_scene_controller: bool,
    pub add_scene_controller_id: usize,
    pub add_scene_controller_post: bool,

    pub asset_filter: String,
    pub reuse_materials_by_name: bool,
    pub objects: Vec<Asset>,
    pub scenes: Vec<Asset>,
}

impl EditorState
{
    pub fn new() -> EditorState
    {
        EditorState
        {
            visible: true,
            loading: Arc::new(RwLock::new(false)),

            try_mode: false,
            selectable: true,
            fly_camera: true,

            project_name: "test_project".to_string(),

            gizmo_position: true,
            gizmo_rotation: false,
            gizmo_scale: false,

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

            settings: SettingsPanel::General,

            hierarchy_expand_all: true,
            hierarchy_filter: String::new(),
            show_internal_entries: false,

            component_filter: String::new(),

            tag_input: String::new(),

            selected_scene_id: None,
            selected_type: SelectionType::None,
            selected_object: String::new(), // type_nodeID/elementID_instanceID
            selected_object_position: None,
            selected_gizmo: None,
            selected_object_gizmo_value: None,

            copy_asset: None,
            copy_asset_transform: None,
            copy_node_id: Arc::new(RwLock::new(None)),

            drag_id: None,

            dialog_add_component: false,
            add_component_id: 0,
            add_component_name: "Component".to_string(),

            dialog_add_camera_controller: false,
            add_camera_controller_id: 0,

            dialog_add_scene_controller: false,
            add_scene_controller_id: 0,
            add_scene_controller_post: false,

            asset_filter: "".to_string(),
            reuse_materials_by_name: false,
            objects: vec![],
            scenes: vec![],
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

    pub fn get_object_ids(&self) -> (Option<u64>, Option<u64>)
    {
        // no scene selected
        if self.selected_scene_id == None && self.selected_object.is_empty()
        {
            return (None, None);
        }

        let parts: Vec<&str> = self.selected_object.split('_').collect();

        let mut item_id: Option<u64> = None;
        let mut subitem_id: Option<u64> = None; // like instance id

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

    pub fn get_selected_node<'a>(&'a mut self, state: &'a mut State) -> (Option<&'a mut Box<Scene>>, Option<NodeItem>, Option<u64>)
    {
        let (node_id, instance_id) = self.get_object_ids();

        if self.selected_type != SelectionType::Object || self.selected_scene_id.is_none() || node_id.is_none()
        {
            return (None, None, None);
        }

        let scene_id: u64 = self.selected_scene_id.unwrap();
        let node_id: u64 = node_id.unwrap();

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

        let scene_id: u64 = self.selected_scene_id.unwrap();

        state.find_scene_by_id_mut(scene_id)
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

    pub fn set_selected_object(&mut self, scene: &mut Scene, node_id: u64, instance_id: Option<u64>, selection_type: SelectionType) -> bool
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

        // highlight
        if !already_selected
        {
            self.selected_object = id_string;
            self.selected_scene_id = Some(scene_id);
            self.selected_type = selection_type;

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

    pub fn load_asset_entries(&mut self, path: &str, state: &State, asset_type: AssetType, egui: &EGui)
    {
        let files = read_files_recursive(path);

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

                let handle = egui.ctx.load_texture(thumb_path.clone(), image, Default::default());

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
            self.scenes = assets;
        }
        else if asset_type == AssetType::Object
        {
            self.objects = assets;
        }
    }
}