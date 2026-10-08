use std::{cell::RefCell, collections::HashMap};

use crate::{component_downcast_mut, helper::{change_tracker::ChangeTracker, option_or_id::OptionOrId}, state::
{
    resources::
    {
        mesh_resource::MeshResourceItem,
        texture::TextureItem,
    },
    scene::
    {
        camera::CameraItem,
        components::
        {
            material::{MaterialItem, TextureType, ALL_TEXTURE_TYPES},
            mesh::Mesh,
            material::Material,
        },
        light::LightItem,
        loader::loader::LoaderOptions,
        node::{Node, NodeItem},
    },
    state::State,
}};

#[derive(Clone, Default, Debug)]
pub struct SceneAddResult
{
    pub texture_ids: Vec<u32>,
    pub mesh_resource_ids: Vec<u32>,
    pub material_ids: Vec<u32>,
    pub camera_ids: Vec<u32>,
    pub light_ids: Vec<u32>,
    pub node_ids: Vec<u32>,
    pub root_node_ids: Vec<u32>,
}

pub struct AssetContainer
{
    pub root_nodes: Vec<NodeItem>,
    pub nodes: Vec<NodeItem>,
    pub textures: Vec<TextureItem>,
    pub materials: Vec<MaterialItem>,
    pub mesh_resources: Vec<MeshResourceItem>,
    pub cameras: Vec<CameraItem>,
    pub lights: Vec<LightItem>,

    pub loader_options: LoaderOptions
}

impl AssetContainer
{
    pub fn new(loader_options: LoaderOptions) -> Self
    {
        Self
        {
            root_nodes: vec![],
            nodes: vec![],
            textures: vec![],
            materials: vec![],
            mesh_resources: vec![],
            cameras: vec![],
            lights: vec![],

            loader_options,
        }
    }

    fn get_texture_by_hash(&self, hash: &str) -> Option<TextureItem>
    {
        for texture in &self.textures
        {
            if texture.read().unwrap().hash == hash
            {
                return Some(texture.clone());
            }
        }

        None
    }

    pub fn insert_texture_or_reuse(&mut self, texture: TextureItem) -> TextureItem
    {
        if let Some(existing) = self.get_texture_by_hash(&texture.read().unwrap().hash)
        {
            existing
        }
        else
        {
            self.textures.push(texture.clone());
            texture
        }
    }

    pub fn apply_to_scene(&mut self, state: &mut State, scene_id: u32) -> SceneAddResult
    {
        let mut result = SceneAddResult::default();

        // *** textures — build remap for reused ones ***
        let mut texture_remap: HashMap<u32, TextureItem> = HashMap::new();
        for texture in &self.textures
        {
            let old_id = texture.read().unwrap().id;
            let tex = state.insert_texture_or_reuse(texture.clone(), "");
            let new_id = tex.read().unwrap().id;
            if old_id != new_id
            {
                texture_remap.insert(old_id, tex.clone());
            }
            result.texture_ids.push(new_id);
        }

        // *** mesh resources — build remap for reused ones ***
        let mut mesh_remap: HashMap<u32, MeshResourceItem> = HashMap::new();
        for mesh_resource in &self.mesh_resources
        {
            let old_id = mesh_resource.read().unwrap().id;
            let mr = state.insert_mesh_resource_or_reuse(mesh_resource.clone(), "");
            let new_id = mr.read().unwrap().id;
            if old_id != new_id
            {
                mesh_remap.insert(old_id, mr.clone());
            }
            result.mesh_resource_ids.push(new_id);
        }

        let Some(scene) = state.find_scene_by_id_mut(scene_id) else { return result; };

        // *** materials — id-based dedup + name-based reuse ***
        let mut material_remap: HashMap<u32, MaterialItem> = HashMap::new();
        for material in &self.materials
        {
            let old_id = material.read().unwrap().get_base().id;

            // name-based reuse
            if self.loader_options.reuse_materials
            {
                let name = material.read().unwrap().get_base().name.clone();
                if let Some(existing) = scene.get_material_by_name(&name)
                {
                    let existing_id = existing.read().unwrap().get_base().id;
                    if existing_id != old_id
                    {
                        material_remap.insert(old_id, existing.clone());
                        result.material_ids.push(existing_id);
                        continue;
                    }
                }
            }

            if scene.get_material_by_id(old_id).is_none()
            {
                scene.add_material(material);
            }
            result.material_ids.push(old_id);
        }

        // *** apply remaps to all nodes ***
        if !texture_remap.is_empty() || !mesh_remap.is_empty() || !material_remap.is_empty()
        {
            for node in &self.nodes
            {
                Self::remap_node(node, &texture_remap, &mesh_remap, &material_remap);
            }
        }

        // *** cameras ***
        for camera in self.cameras.drain(..)
        {
            result.camera_ids.push(camera.id);
            scene.cameras.push(camera);
        }

        // *** lights ***
        for light in self.lights.drain(..)
        {
            result.light_ids.push(light.id);
            scene.lights.get_mut().push(RefCell::new(ChangeTracker::new(light)));
        }

        // *** nodes ***
        for root_node in &self.root_nodes
        {
            if self.loader_options.hide_root_nodes
            {
                root_node.write().unwrap().settings.visible = false;
            }

            let root_id = root_node.read().unwrap().id;
            result.root_node_ids.push(root_id);

            if let Some(parent_id) = self.loader_options.parent_node_id
            {
                if let Some(parent_node) = scene.find_node_by_id(parent_id)
                {
                    Node::add_node(parent_node.clone(), root_node.clone());
                }
            }
            else
            {
                scene.add_node(root_node.clone());
            }
        }

        for node in &self.nodes
        {
            result.node_ids.push(node.read().unwrap().id);
        }

        result
    }

    fn remap_node(node: &NodeItem, texture_remap: &HashMap<u32, TextureItem>, mesh_remap: &HashMap<u32, MeshResourceItem>, material_remap: &HashMap<u32, MaterialItem>)
    {
        let mut node = node.write().unwrap();

        // remap textures in material components
        if !texture_remap.is_empty()
        {
            for mat_item in node.find_components::<Material>()
            {
                component_downcast_mut!(mat_item, Material);

                let types_to_remap: Vec<(TextureType, TextureItem)> = ALL_TEXTURE_TYPES.iter().filter_map(|&tex_type|
                {
                    let tex_state = mat_item.get_texture_by_type(tex_type)?;
                    let tex = tex_state.get()?.clone();
                    let old_id = tex.read().unwrap().id;
                    texture_remap.get(&old_id).map(|new_tex| (tex_type, new_tex.clone()))
                }).collect();

                for (tex_type, new_tex) in types_to_remap
                {
                    mat_item.set_texture(new_tex, tex_type);
                }
            }
        }

        // remap mesh resources in mesh components
        if !mesh_remap.is_empty()
        {
            for mesh_item in node.find_components::<Mesh>()
            {
                component_downcast_mut!(mesh_item, Mesh);

                if let Some(mr) = mesh_item.mesh_resource.as_ref()
                {
                    let mr: &MeshResourceItem = mr;
                    let old_id = mr.read().unwrap().id;
                    if let Some(new_mr) = mesh_remap.get(&old_id)
                    {
                        mesh_item.mesh_resource = OptionOrId::<MeshResourceItem>::Some(new_mr.clone());
                    }
                }
            }
        }

        // remap material components
        if !material_remap.is_empty()
        {
            let mat_ids_to_replace: Vec<(u32, MaterialItem)> = node.find_components::<Material>().into_iter().filter_map(|mat_item|
            {
                let old_id = mat_item.read().unwrap().get_base().id;
                material_remap.get(&old_id).map(|new_mat| (old_id, new_mat.clone()))
            }).collect();

            for (old_id, new_mat) in mat_ids_to_replace
            {
                node.remove_component_by_id(old_id);
                node.add_component(new_mat);
            }
        }
    }
}
