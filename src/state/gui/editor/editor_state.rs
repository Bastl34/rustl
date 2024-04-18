use std::{sync::{RwLock, Arc}, fmt::format};

use image::{ImageFormat, EncodableLayout};
use nalgebra::Point2;

use crate::{state::{scene::{scene::Scene, node::NodeItem}, state::State}, resources::resources::{read_files_recursive, exists, load_binary}, helper::file::{get_extension, get_stem}, rendering::egui::EGui};

const THUMB_EXTENSION: &str = "png";
const THUMB_SUFFIX_NAME: &str = "_thumb.png";

#[derive(PartialEq, Eq)]
pub enum SettingsPanel
{
    Components,
    Material,
    Camera,
    Texture,
    Light,
    Scene,
    Object,
    Rendering
}

#[derive(PartialEq, Eq)]
pub enum SelectionType
{
    Object,
    Camera,
    Light,
    Material,
    Texture,
    None
}

#[derive(PartialEq, Eq)]
pub enum PickType
{
    Camera,
    Parent,
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

    pub try_out: bool,
    pub selectable: bool,
    pub fly_camera: bool,

    pub pick_mode: PickType,

    pub edit_mode: Option<EditMode>,

    pub bottom: BottomPanel,
    pub asset_type: AssetType,

    pub settings: SettingsPanel,

    pub hierarchy_expand_all: bool,
    pub hierarchy_filter: String,

    pub selected_scene_id: Option<u64>,
    pub selected_type: SelectionType,
    pub selected_object: String,

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

            try_out: false,
            selectable: true,
            fly_camera: true,

            pick_mode: PickType::None,

            edit_mode: None,

            bottom: BottomPanel::Assets,
            asset_type: AssetType::Object,

            settings: SettingsPanel::Rendering,

            hierarchy_expand_all: true,
            hierarchy_filter: String::new(),

            selected_scene_id: None,
            selected_type: SelectionType::None,
            selected_object: String::new(), // type_nodeID/elementID_instanceID

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
    pub fn get_object_ids(&self) -> (Option<u64>, Option<u64>)
    {
        // no scene selected
        if self.selected_scene_id == None || self.selected_object.is_empty()
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
        }

        self.selected_object.clear();
        self.selected_scene_id = None;
        self.selected_type = SelectionType::None;
    }

    pub fn set_try_out(&mut self, state: &mut State, try_out: bool)
    {
        self.try_out = try_out;
        self.visible = !try_out;
        state.rendering.fullscreen.set(try_out);
        state.input_manager.mouse.visible.set(!try_out);

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