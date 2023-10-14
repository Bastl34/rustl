use nalgebra::Point2;

use crate::state::{scene::{scene::Scene, node::NodeItem}, state::State};

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
pub enum BottomPanel
{
    Assets,
    Debug,
    Console,
}

#[derive(Clone, Copy)]
pub enum EditMode
{
    Movement(Point2::<f32>, bool, bool, bool),
    Rotate(Point2::<f32>, bool, bool, bool)
}

pub struct EditorState
{
    pub visible: bool,
    pub try_out: bool,
    pub selectable: bool,
    pub fly_camera: bool,

    pub pick_mode: SelectionType,

    pub edit_mode: Option<EditMode>,

    pub bottom: BottomPanel,

    pub settings: SettingsPanel,

    pub hierarchy_expand_all: bool,
    pub hierarchy_filter: String,

    pub selected_scene_id: Option<u64>,
    pub selected_type: SelectionType,
    pub selected_object: String,

    pub dialog_add_component: bool,
    pub add_component_id: usize,
    pub add_component_name: String,
}

impl EditorState
{
    pub fn new() -> EditorState
    {
        EditorState
        {
            visible: true,
            try_out: false,
            selectable: true,
            fly_camera: true,

            pick_mode: SelectionType::None,

            edit_mode: None,

            bottom: BottomPanel::Assets,

            settings: SettingsPanel::Rendering,

            hierarchy_expand_all: true,
            hierarchy_filter: String::new(),

            selected_scene_id: None,
            selected_type: SelectionType::None,
            selected_object: String::new(), // type_nodeID/elementID_instanceID

            dialog_add_component: false,
            add_component_id: 0,
            add_component_name: "Component".to_string()
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
                    let node = node.read().unwrap();
                    for instance in node.instances.get_ref()
                    {
                        let mut instance = instance.borrow_mut();
                        let instance_data = instance.get_data_mut().get_mut();
                        instance_data.highlight = false;
                    }
                    /*
                    if let Some(deselect_instance_id) = deselect_instance_id
                    {
                        if let Some(instance) = node.read().unwrap().find_instance_by_id(deselect_instance_id)
                        {
                            let mut instance = instance.borrow_mut();
                            let instance_data = instance.get_data_mut().get_mut();
                            instance_data.highlight = false;
                        }
                    }
                    */
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
}