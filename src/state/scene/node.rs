#![allow(dead_code)]

use std::{collections::HashMap, fmt, sync::{Arc, RwLock}};
use nalgebra::{Matrix4, Point3, Vector4};
use parry3d::bounding_volume::{Aabb, BoundingSphere};
use parry3d::bounding_volume::BoundingVolume; // Needed for BoundingSphere::merge
use regex::Regex;
use serde::{de::{self, MapAccess, Visitor}, ser::SerializeMap, Deserialize, Deserializer, Serialize, Serializer};

use crate::{component_downcast, component_downcast_mut, console_log, console_warning, helper::{asset_path_descriptor::AssetPathDesciptor, change_tracker::ChangeTracker, generic::match_by_include_exclude, math::extract_max_scale_from_transform, option_or_id::OptionOrId}, state::{helper::render_item::RenderItemOption, scene::{components::component::find_and_add_new_components, scene::Scene}, state::InputOutput}};

use super::{components::{alpha::Alpha, animation::Animation, component::{find_component, find_component_by_id, find_components, remove_component_by_id, remove_component_by_type, remove_components_by_ids, Component, ComponentItem}, joint::Joint, mesh::Mesh, morph_target::MorphTarget, transformation::Transformation}, instance::{Instance, InstanceItem}, manager::id_manager, utilities::{extras::Extras, tags::Tags}};

pub type NodeItem = Arc<RwLock<Box<Node>>>;
pub type InstanceItemArc = Arc<RwLock<InstanceItem>>;

const UPDATE_ALL_INSTANCES_THRESHOLD: u32 = 10; // if more than 10 instances got an update -> update all instances at once to save performance

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct NodeSettings
{
    pub visible: bool,
    pub locked: bool,
    pub pickable: bool,

    pub render_children_first: bool,
    pub alpha_index: i64, // this can be used to influence the alpha sorting (for transparent objects rendering)
    pub render_group_id: i64, // this can be used to influence the sorting (for rendering) -> higher number means later rendering

    pub depth_test: bool,
    pub depth_write: bool,

    pub pick_bbox_first: bool,

    pub frustum_culling: bool,
    pub occlusion_culling: bool,
}

pub struct NodeUpdateResult
{
    pub delete_nodes: Vec<u32>,
}

pub struct Node
{
    pub id: u32,
    pub uuid: String,

    pub source: Option<AssetPathDesciptor>,

    pub name: String,
    pub root_node: bool,

    pub settings: NodeSettings,

    pub parent: OptionOrId<NodeItem>,

    pub skin: Vec<OptionOrId<NodeItem>>,

    pub extras: Extras,
    pub tags: Tags,

    pub nodes: Vec<NodeItem>,
    pub instances: ChangeTracker<Vec<Arc<RwLock<InstanceItem>>>>,
    pub components: Vec<ComponentItem>,

    pub instance_render_item: RenderItemOption,
    pub skeleton_render_item: RenderItemOption,
    pub skeleton_morph_target_bind_group_render_item: RenderItemOption,
    // pub occlusion_render_item: RenderItemOption,

    delete_later_request: bool,
}

impl Serialize for Node
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where S: Serializer
    {
        let mut map = serializer.serialize_map(None)?;

        map.serialize_entry("uuid", &self.uuid)?;
        map.serialize_entry("name", &self.name)?;
        map.serialize_entry("root_node", &self.root_node)?;
        map.serialize_entry("settings", &self.settings)?;
        map.serialize_entry("extras", &self.extras)?;
        map.serialize_entry("tags", &self.tags)?;

        if let Some(parent) = self.parent.as_ref()
        {
            map.serialize_entry("parent", &parent.read().unwrap().uuid)?;
        }

        let skin: Vec<_> = self.skin.iter().filter_map(|skin_node|
        {
            if let OptionOrId::Some(node_item) = skin_node
            {
                Some(node_item.read().unwrap().uuid.clone())
            }
            else
            {
                None
            }
        }).collect();
        map.serialize_entry("skin", &skin)?;

        let instance_guards: Vec<_> = self.instances.get_ref().iter().map(|arc| arc.read().unwrap()).collect();
        let instance_refs: Vec<&Instance> = instance_guards.iter().map(|guard| guard.as_ref()).collect();
        map.serialize_entry("instances", &instance_refs)?;

        let node_guards: Vec<_> = self.nodes.iter().map(|arc| arc.read().unwrap()).collect();
        let node_refs: Vec<&Node> = node_guards.iter().map(|guard| guard.as_ref()).collect();
        map.serialize_entry("nodes", &node_refs)?;

        let components_filtered = self.components.iter().filter(|c| c.read().unwrap().get_base().export);
        let components_guards: Vec<_> = components_filtered.map(|arc| arc.read().unwrap()).collect();
        let components_refs: Vec<&dyn Component> = components_guards.iter().map(|guard| guard.as_ref()).collect();
        map.serialize_entry("components", &components_refs)?;

        if let Some(source) = &self.source
        {
            map.serialize_entry("source", source)?;
        }

        map.end()
    }
}

impl<'de> Deserialize<'de> for Node
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where D: Deserializer<'de>
    {
        struct NodeVisitor;

        impl<'de> Visitor<'de> for NodeVisitor
        {
            type Value = Node;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result
            {
                formatter.write_str("struct Node")
            }

            fn visit_map<V>(self, mut map: V) -> Result<Node, V::Error>
            where V: MapAccess<'de>
            {
                let mut node = Node::default();

                while let Some(key) = map.next_key::<String>()?
                {
                    match key.as_str()
                    {
                        "uuid" => node.uuid = map.next_value()?,
                        "name" => node.name = map.next_value()?,
                        "root_node" => node.root_node = map.next_value()?,
                        "parent" => node.parent = OptionOrId::from_id_or_none(map.next_value()?),
                        "settings" => node.settings = map.next_value()?,
                        "extras" => node.extras = map.next_value()?,
                        "tags" => node.tags = map.next_value()?,
                        "skin" => node.skin = OptionOrId::from_id_vec(&map.next_value()?),
                        "source" => node.source = Some(map.next_value()?),
                        "instances" =>
                        {
                            node.instances = ChangeTracker::new
                            (
                                map.next_value()
                                    .into_iter()
                                    .map(|inst| Arc::new(RwLock::new(Box::new(inst))))
                                    .collect()
                            )
                        }
                        "nodes" =>
                        {
                            node.nodes = map.next_value()
                            .into_iter()
                            .map(|node| Arc::new(RwLock::new(Box::new(node))))
                            .collect()
                        },
                        "components" =>
                        {
                            let components_vec: Vec<Box<dyn Component>> = map.next_value()?;
                            node.components = components_vec
                                .into_iter()
                                .map(|component| Arc::new(RwLock::new(component)))
                                .collect();
                        }
                        _ =>
                        {
                            // ignore
                            let _: de::IgnoredAny = map.next_value()?;
                        }
                    }
                }

                Ok(node)
            }
        }

        deserializer.deserialize_map(NodeVisitor)
    }
}

impl Node
{
    pub fn new(name: &str) -> NodeItem
    {
        let mut node = Self::default();
        node.name = name.to_string();

        Arc::new(RwLock::new(Box::new(node)))
    }

    pub fn default() -> Self
    {
        Self
        {
            id: id_manager::get_next_node_id(),
            uuid: uuid::Uuid::new_v4().to_string(),

            source: None,

            name: "default".to_string(),
            root_node: false,

            settings: NodeSettings
            {
                visible: true,
                locked: false,
                pickable: true,

                render_children_first: false,
                alpha_index: 0,
                render_group_id: 0,

                depth_write: true,
                depth_test: true,

                pick_bbox_first: true,
                frustum_culling: true,
                occlusion_culling: true,
            },

            components: vec![],

            parent: OptionOrId::None,

            skin: vec![],

            extras: Extras::new(),
            tags: Tags::new(),

            nodes: vec![],
            instances: ChangeTracker::new(vec![]),

            instance_render_item: None,
            skeleton_render_item: None,
            skeleton_morph_target_bind_group_render_item: None,
            // occlusion_render_item: None,

            delete_later_request: false
        }
    }

    pub fn cleanup_cyclic_references(nodes: &Vec<NodeItem>)
    {
        // clear instances and // clear parents
        for child_node in nodes
        {
            // remove cyclic reference nodes in instances
            let mut node = child_node.write().unwrap();
            node.clear_instances();

            node.components.clear();

            // remove cyclic reference to parent
            node.parent = OptionOrId::None;
        }
    }

    pub fn remove_node_from_components(node: NodeItem, node_to_remove: NodeItem)
    {
        let mut components;
        {
            let node = node.read().unwrap();

            components = node.components.clone();

            for instance in node.instances.get_ref()
            {
                let instance = instance.read().unwrap();
                components.append(&mut instance.components.clone());
            }
        }

        for component in &mut components
        {
            component.write().unwrap().cleanup_node(node_to_remove.clone());
        }

        // child nodes
        let node = node.read().unwrap();
        for child in &node.nodes
        {
            Self::remove_node_from_components(child.clone(), node_to_remove.clone());
        }
    }

    pub fn delete_later(&mut self)
    {
        self.delete_later_request = true;
    }

    pub fn add_node(node: NodeItem, child_node: NodeItem)
    {
        {
            let mut node = node.write().unwrap();
            node.nodes.push(child_node.clone());
        }

        {
            let mut child_node = child_node.write().unwrap();
            child_node.parent = OptionOrId::Some(node.clone());
        }
    }

    pub fn add_node_front(node: NodeItem, child_node: NodeItem)
    {
        {
            let mut node = node.write().unwrap();
            node.nodes.insert(0, child_node.clone());
        }

        {
            let mut child_node = child_node.write().unwrap();
            child_node.parent = OptionOrId::Some(node.clone());
        }
    }

    pub fn move_to_front(&mut self, node: NodeItem)
    {
        let nodes_amount = self.nodes.len();
        if nodes_amount == 0
        {
            return;
        }

        self.nodes.retain(|child_node|
        {
            child_node.read().unwrap().id != node.read().unwrap().id
        });

        if self.nodes.len() == nodes_amount
        {
            return;
        }

        self.nodes.insert(0, node.clone());
    }

    pub fn move_to_back(&mut self, node: NodeItem)
    {
        let nodes_amount = self.nodes.len();
        if nodes_amount == 0
        {
            return;
        }

        self.nodes.retain(|child_node|
        {
            child_node.read().unwrap().id != node.read().unwrap().id
        });

        if self.nodes.len() == nodes_amount
        {
            return;
        }

        self.nodes.push(node.clone());
    }

    pub fn set_parent(node: NodeItem, new_parent: NodeItem)
    {
        // remove from old node list
        if let Some(old_parent) = node.read().unwrap().parent.as_ref()
        {
            let id = node.read().unwrap().id;

            let mut old_parent_write = old_parent.write().unwrap();
            old_parent_write.nodes.retain(|node|
            {
                node.read().unwrap().id != id
            });
        }

        // add to new node list
        new_parent.write().unwrap().nodes.push(node.clone());

        node.write().unwrap().parent = OptionOrId::Some(new_parent);

        node.write().unwrap().force_instances_update();
    }

    pub fn add_component(&mut self, component: ComponentItem)
    {
        self.components.push(component);
    }

    pub fn add_component_front(&mut self, component: ComponentItem)
    {
        self.components.insert(0, component);
    }

    pub fn find_component<T>(&self) -> Option<ComponentItem> where T: 'static
    {
        find_component::<T>(&self.components)
    }

    pub fn has_component<T>(&self) -> bool where T: 'static
    {
        find_component::<T>(&self.components).is_some()
    }

    pub fn find_component_by_id(&self, id: u32) -> Option<ComponentItem>
    {
        find_component_by_id(&self.components, id)
    }

    pub fn find_components<T: Component>(&self) -> Vec<ComponentItem> where T: 'static
    {
        find_components::<T>(&self.components)
    }

    pub fn remove_component_by_type<T>(&mut self) where T: 'static
    {
        if remove_component_by_type::<T>(&mut self.components)
        {
            self.force_instances_update();
        }
    }

    pub fn remove_component_by_id(&mut self, id: u32)
    {
        if remove_component_by_id(&mut self.components, id)
        {
            self.force_instances_update();
        }
    }

    pub fn remove_components_by_ids(&mut self, ids: &Vec<u32>)
    {
        if remove_components_by_ids(&mut self.components, &ids)
        {
            self.force_instances_update();
        }
    }

    pub fn move_component_up(&mut self, component: ComponentItem)
    {
        let index = self.components.iter().position(|c| Arc::ptr_eq(c, &component));

        if let Some(index) = index
        {
            if index > 0
            {
                self.components.swap(index, index - 1);
                self.force_instances_update();
            }
        }
    }

    pub fn move_component_down(&mut self, component: ComponentItem)
    {
        let index = self.components.iter().position(|c| Arc::ptr_eq(c, &component));

        if let Some(index) = index
        {
            if index < self.components.len() - 1
            {
                self.components.swap(index, index + 1);
                self.force_instances_update();
            }
        }
    }

    pub fn has_mesh(&self) -> bool
    {
        self.has_component::<Mesh>()
    }

    pub fn get_mesh(&self) -> Option<ComponentItem>
    {
        self.find_component::<Mesh>()
    }

    pub fn get_meshes(&self) -> Vec<ComponentItem>
    {
        self.find_components::<Mesh>()
    }

    pub fn get_meshes_with_mesh_resource(&self) -> Vec<ComponentItem>
    {
        let meshes = self.find_components::<Mesh>();

        meshes.iter().filter_map(|component|
        {
            let guard = component.read().ok()?;
            let mesh = guard.as_any().downcast_ref::<Mesh>()?;
            if mesh.mesh_resource.is_some()
            {
                Some(component.clone())
            }
            else
            {
                None
            }
        })
        .collect()
    }

    pub fn has_tag(&self, tag: &str) -> bool
    {
        self.tags.contains(tag)
    }

    pub fn get_world_bounding_info(&self, instance_id: Option<u32>, recursive: bool, predicate: Option<Arc<dyn Fn(NodeItem) -> bool + Send + Sync>>) -> Option<(Point3<f32>, Point3<f32>)>
    {
        let meshes = self.get_meshes();

        let mut min = Point3::<f32>::new(std::f32::MAX, std::f32::MAX, std::f32::MAX);
        let mut max = Point3::<f32>::new(std::f32::MIN, std::f32::MIN, std::f32::MIN);

        let mut found = false;

        // own meshes
        for instance in self.instances.get_ref()
        {
            let instance = instance.read().unwrap();

            // check for matching instance id
            if let Some(instance_id) = instance_id
            {
                if instance_id != instance.id
                {
                    continue;
                }
            }

            let transform = instance.calculate_transform();

            for mesh in &meshes
            {
                component_downcast!(mesh, Mesh);

                let mut bbox = None;
                if let Some(skin_bbox) = mesh.get_data().b_box_skin
                {
                    bbox = Some(skin_bbox);
                }
                else if let Some(mesh_resource) = mesh.mesh_resource.as_ref()
                {
                    bbox = Some(mesh_resource.read().unwrap().get_data().b_box);
                }

                if let Some(bbox) = bbox
                {
                    let transformed_min = transform * Vector4::<f32>::new(bbox.mins.x, bbox.mins.y, bbox.mins.z, 1.0);
                    let transformed_max = transform * Vector4::<f32>::new(bbox.maxs.x, bbox.maxs.y, bbox.maxs.z, 1.0);

                    // sometimes coordinates are flipped because of the transformation -> check for min and max points
                    min.x = min.x.min(transformed_min.x);
                    min.y = min.y.min(transformed_min.y);
                    min.z = min.z.min(transformed_min.z);

                    min.x = min.x.min(transformed_max.x);
                    min.y = min.y.min(transformed_max.y);
                    min.z = min.z.min(transformed_max.z);


                    max.x = max.x.max(transformed_min.x);
                    max.y = max.y.max(transformed_min.y);
                    max.z = max.z.max(transformed_min.z);

                    max.x = max.x.max(transformed_max.x);
                    max.y = max.y.max(transformed_max.y);
                    max.z = max.z.max(transformed_max.z);

                    found = true;
                }
            }
        }

        // meshes of child nodes
        if recursive
        {
            for node in &self.nodes
            {
                if let Some(predicate) = &predicate
                {
                    if !predicate(node.clone())
                    {
                        continue;
                    }
                }

                let node = node.read().unwrap();
                let child_min_max = node.get_world_bounding_info(instance_id, recursive, predicate.clone());

                if let Some(child_min_max) = child_min_max
                {
                    let (child_min, child_max) = child_min_max;

                    min.x = min.x.min(child_min.x);
                    min.y = min.y.min(child_min.y);
                    min.z = min.z.min(child_min.z);

                    max.x = max.x.max(child_max.x);
                    max.y = max.y.max(child_max.y);
                    max.z = max.z.max(child_max.z);

                    found = true;
                }
            }
        }

        if found
        {
            return Some((min, max));
        }

        None
    }

    pub fn get_world_bbox_center(&self, instance_id: Option<u32>, recursive: bool, predicate: Option<Arc<dyn Fn(NodeItem) -> bool + Send + Sync>>) -> Option<Point3<f32>>
    {
        let bounding_info = self.get_world_bounding_info(instance_id, recursive, predicate);

        if let Some(bounding_info) = bounding_info
        {
            let (min, max) = bounding_info;

            return Some(min + (max - min) / 2.0);
        }

        None
    }

    pub fn get_bounding_sphere_for_all_instances(&self, transformations: &Vec::<Matrix4::<f32>>) -> Option<(Point3<f32>, f32)>
    {
        if transformations.len() != self.instances.get_ref().len()
        {
            console_warning!("get_bounding_sphere_for_all_instances: transformations length does not match instances length - which is not supported");
            return None;
        }

        let meshes = self.get_meshes();

        let mut bounding_sphere_mesh: Option<BoundingSphere> = None;

        for mesh in &meshes
        {
            component_downcast!(mesh, Mesh);

            let mut sphere = None;
            if let Some(skin_sphere) = mesh.get_scaled_skin_bounding_sphere()
            {
                sphere = Some(skin_sphere);
            }
            else if let Some(mesh_resource) = mesh.mesh_resource.as_ref()
            {
                sphere = Some(mesh_resource.read().unwrap().get_data().b_sphere);
            }

            if let Some(sphere) = sphere
            {
                if let Some(bounding_sphere_mesh) = bounding_sphere_mesh.as_mut()
                {
                    bounding_sphere_mesh.merge(&sphere);
                }
                else
                {
                    bounding_sphere_mesh = Some(sphere);
                }
            }
        }

        let mut bounding_sphere_result: Option<BoundingSphere> = None;

        if let Some(bounding_sphere_mesh) = bounding_sphere_mesh
        {
            for (instance_id, _) in self.instances.get_ref().iter().enumerate()
            {
                let transform = transformations.get(instance_id).unwrap();

                let max_scale = extract_max_scale_from_transform(transform);

                let transformed_center = transform * Vector4::<f32>::new(bounding_sphere_mesh.center.x, bounding_sphere_mesh.center.y, bounding_sphere_mesh.center.z, 1.0);
                let transformed_center = Point3::new
                (
                    transformed_center.x / transformed_center.w,
                    transformed_center.y / transformed_center.w,
                    transformed_center.z / transformed_center.w,
                );

                let transformed_radius = bounding_sphere_mesh.radius * max_scale;

                // merge into final sphere
                let instance_sphere = BoundingSphere::new
                (
                    Point3::<f32>::new(transformed_center.x, transformed_center.y, transformed_center.z).into(),
                    transformed_radius
                );

                if let Some(bounding_sphere_all) = bounding_sphere_result.as_mut()
                {
                    bounding_sphere_all.merge(&instance_sphere);
                }
                else
                {
                    bounding_sphere_result = Some(instance_sphere);
                }
            }
        }

        if let Some(bounding_sphere_result) = bounding_sphere_result
        {
            return Some((bounding_sphere_result.center.into(), bounding_sphere_result.radius));
        }

        None
    }

    pub fn get_bounding_box_for_all_instances_from_cached_transform(&self) -> Option<(Point3<f32>, Point3<f32>)>
    {
        let meshes = self.get_meshes();

        let mut bounding_box_mesh: Option<Aabb> = None;

        for mesh in &meshes
        {
            component_downcast!(mesh, Mesh);

            let mut bounding_box = None;
            if let Some(skin_box) = mesh.get_scaled_skin_bbox()
            {
                bounding_box = Some(skin_box);
            }
            else if let Some(mesh_resource) = mesh.mesh_resource.as_ref()
            {
                bounding_box = Some(mesh_resource.read().unwrap().get_data().b_box);
            }

            if let Some(bounding_box) = bounding_box
            {
                if let Some(bounding_box_mesh) = bounding_box_mesh.as_mut()
                {
                    bounding_box_mesh.merge(&bounding_box);
                }
                else
                {
                    bounding_box_mesh = Some(bounding_box);
                }
            }
        }

        let mut bounding_box_result: Option<Aabb> = None;

        if let Some(bounding_box_mesh) = bounding_box_mesh
        {
            for instance in self.instances.get_ref()
            {
                let instance = instance.read().unwrap();
                let transform = instance.get_cached_world_transform();

                // hole die min und max punkte aus der Aabb
                let min = bounding_box_mesh.mins;
                let max = bounding_box_mesh.maxs;

                // berechne die 8 eckpunkte
                let corners =
                [
                    Point3::new(min.x, min.y, min.z),
                    Point3::new(min.x, min.y, max.z),
                    Point3::new(min.x, max.y, min.z),
                    Point3::new(min.x, max.y, max.z),
                    Point3::new(max.x, min.y, min.z),
                    Point3::new(max.x, min.y, max.z),
                    Point3::new(max.x, max.y, min.z),
                    Point3::new(max.x, max.y, max.z),
                ];

                // transformiere alle punkte
                let mut new_min = Point3::new(f32::MAX, f32::MAX, f32::MAX);
                let mut new_max = Point3::new(f32::MIN, f32::MIN, f32::MIN);

                for corner in &corners
                {
                    let v = transform * Vector4::new(corner.x, corner.y, corner.z, 1.0);
                    let p = Point3::new(v.x / v.w, v.y / v.w, v.z / v.w);

                    new_min.x = new_min.x.min(p.x);
                    new_min.y = new_min.y.min(p.y);
                    new_min.z = new_min.z.min(p.z);

                    new_max.x = new_max.x.max(p.x);
                    new_max.y = new_max.y.max(p.y);
                    new_max.z = new_max.z.max(p.z);
                }

                // neue transformierte box
                let instance_box = Aabb::new(new_min.into(), new_max.into());

                if let Some(bounding_box_all) = bounding_box_result.as_mut()
                {
                    bounding_box_all.merge(&instance_box);
                }
                else
                {
                    bounding_box_result = Some(instance_box);
                }
            }
        }

        if let Some(bounding_box_result) = bounding_box_result
        {
            return Some((bounding_box_result.mins.into(), bounding_box_result.maxs.into()));
        }

        None
    }

    pub fn is_name_matching_regex(&self, regex: &str) -> bool
    {
        let regex_item: Regex = Regex::new(regex).unwrap();

        regex_item.is_match(&self.name)
    }

    pub fn parent_amount(&self) -> u32
    {
        let mut parent_amount = 0;

        let mut parent = self.parent.clone();
        while parent.is_some()
        {
            parent_amount += 1;
            parent = parent.unwrap().read().unwrap().parent.clone();
        }

        parent_amount
    }

    pub fn has_parent(&self, parent_node: NodeItem) -> bool
    {
        let mut parent = self.parent.clone();
        while parent.is_some()
        {
            let parent_clone = parent.clone();

            if let OptionOrId::Some(parent) = parent
            {
                if parent.read().unwrap().id == parent_node.read().unwrap().id
                {
                    return true;
                }
            }

            parent = parent_clone.unwrap().read().unwrap().parent.clone();
        }

        false
    }

    pub fn has_parent_or_is_equal(&self, node: NodeItem) -> bool
    {
        if self.id == node.read().unwrap().id
        {
            return true;
        }

        self.has_parent(node)
    }

    pub fn has_parent_id(&self, parent_node_id: u32) -> bool
    {
        let mut parent = self.parent.clone();
        while parent.is_some()
        {
            let parent_clone = parent.clone();

            if let OptionOrId::Some(parent) = parent
            {
                if parent.read().unwrap().id == parent_node_id
                {
                    return true;
                }
            }

            parent = parent_clone.unwrap().read().unwrap().parent.clone();
        }

        false
    }

    pub fn has_parent_id_or_is_equal(&self, node_id: u32) -> bool
    {
        if self.id == node_id
        {
            return true;
        }

        self.has_parent_id(node_id)
    }

    pub fn is_locked(&self) -> bool
    {
        if self.settings.locked
        {
            return true;
        }

        let mut parent = self.parent.clone();
        while parent.is_some()
        {
            {
                let parent = parent.clone().unwrap();
                if parent.read().unwrap().settings.locked
                {
                    return true;
                }
            }

            parent = parent.unwrap().read().unwrap().parent.clone();
        }

        false
    }

    pub fn set_pickable(&mut self, pickable: bool)
    {
        self.settings.pickable = pickable;
        let all_childs = Scene::list_all_child_nodes(&self.nodes);
        for child_node in all_childs
        {
            let mut child_node = child_node.write().unwrap();
            child_node.settings.pickable = pickable;

            for instance in child_node.instances.get_ref()
            {
                let mut instance = instance.write().unwrap();
                instance.pickable = pickable;
            }
        }
    }

    pub fn set_highlighted(&mut self, highlight: bool)
    {
        let all_childs = Scene::list_all_child_nodes(&self.nodes);
        for child_node in all_childs
        {
            let child_node = child_node.write().unwrap();

            for instance in child_node.instances.get_ref()
            {
                let mut instance = instance.write().unwrap();
                if instance.get_data().highlight != highlight
                {
                    instance.get_data_mut().get_mut().highlight = highlight;
                }
            }
        }

        // self
        for instance in self.instances.get_ref()
        {
            let mut instance = instance.write().unwrap();
            if instance.get_data().highlight != highlight
            {
                instance.get_data_mut().get_mut().highlight = highlight;
            }
        }
    }

    pub fn has_changed_instance_data(&self) -> bool
    {
        for instance in self.instances.get_ref()
        {
            let instance = instance.read().unwrap();
            if instance.get_data_tracker().changed()
            {
                return true;
            }
        }

        false
    }

    pub fn has_changed_data(&self) -> bool
    {
        if self.has_changed_instance_data()
        {
            return true;
        }

        // check transformation
        let transformations = self.find_components::<Transformation>();
        for transformation in transformations
        {
            component_downcast!(transformation, Transformation);
            if transformation.get_data_tracker().changed()
            {
                return true;
            }
        }

        false
    }

    fn get_transform(&self) -> (Matrix4<f32>, bool)
    {
        let transform_component = self.find_component::<Transformation>();
        let joint_component = self.find_component::<Joint>();

        if let Some(joint_component) = joint_component
        {
            component_downcast!(joint_component, Joint);

            if joint_component.get_base().is_enabled
            {
                let mut parent_transform = Matrix4::<f32>::identity();

                if let Some(parent) = self.parent.as_ref()
                {
                    parent_transform = parent.read().unwrap().get_full_joint_transform(None, true);
                }

                let joint_transform = joint_component.get_joint_transform(&parent_transform);

                return
                (
                    joint_transform,
                    true
                );
            }
        }
        else if let Some(transform_component) = transform_component
        {
            component_downcast!(transform_component, Transformation);

            if transform_component.get_base().is_enabled
            {
                return
                (
                    transform_component.get_transform().clone(),
                    transform_component.has_parent_inheritance()
                );
            }
        }

        (
            Matrix4::<f32>::identity(),
            true
        )
    }

    pub fn get_full_transform(&self) -> Matrix4<f32>
    {
        let (node_transform, node_parent_inheritance) = self.get_transform();
        let mut parent_trans = Matrix4::<f32>::identity();

        if let Some(parent_node) = self.parent.as_ref()
        {
            let parent_node = parent_node.read().unwrap();
            parent_trans = parent_node.get_full_transform();
        }

        if node_parent_inheritance
        {
            parent_trans * node_transform
        }
        else
        {
            node_transform
        }
    }

    /*
    pub fn get_full_transform_inverse(&self) -> Matrix4<f32>
    {
        let (node_transform, node_parent_inheritance) = self.get_transform();
        let mut parent_inverse_trans = Matrix4::<f32>::identity();

        if let Some(parent_node) = &self.parent
        {
            let parent_node = parent_node.read().unwrap();
            parent_inverse_trans = parent_node.get_full_transform_inverse();
        }

        if node_parent_inheritance
        {
            node_transform.try_inverse().unwrap() * parent_inverse_trans
        }
        else
        {
            node_transform.try_inverse().unwrap()
        }
    }
    */

    pub fn get_full_transform_inverse(&self) -> Matrix4<f32>
    {
        let full_transform = self.get_full_transform();

        full_transform.try_inverse().unwrap()
    }

    pub fn transform_vec_global_to_local(&self, vec: &Vector4<f32>) -> Vector4<f32>
    {
        let trans_inverse = self.get_full_transform_inverse();

        trans_inverse * vec
    }

    pub fn transform_vec_local_to_global(&self, vec: &Vector4<f32>) -> Vector4<f32>
    {
        let trans = self.get_full_transform();

        trans * vec
    }

    pub fn transform_vec_from_node_to_local(&self, vec: &Vector4<f32>, node: NodeItem) -> Vector4<f32>
    {
        let node = node.read().unwrap();
        let global_vec = node.transform_vec_local_to_global(vec);

        self.transform_vec_global_to_local(&global_vec)
    }

    pub fn get_full_joint_transform(&self, transform_cache: Option<&HashMap<u32, Matrix4::<f32>>>, animated: bool) -> Matrix4<f32>
    {
        let joint_component = self.find_component::<Joint>();

        if let Some(joint_component) = joint_component
        {
            component_downcast!(joint_component, Joint);

            let mut parent_transform = Matrix4::<f32>::identity();

            if let Some(parent) = self.parent.as_ref()
            {
                let mut from_cache = false;
                if let Some(transform_cache) = transform_cache
                {
                    let parent_transform_from_cache = transform_cache.get(&parent.read().unwrap().id);
                    if let Some(parent_transform_from_cache) = parent_transform_from_cache
                    {
                        parent_transform = *parent_transform_from_cache;
                        from_cache = true;
                    }
                }

                if !from_cache
                {
                    parent_transform = parent.read().unwrap().get_full_joint_transform(transform_cache, animated);
                }
            }

            // animated transformation or just skinned transformation
            let local_animation_transform;
            if animated
            {
                local_animation_transform = joint_component.get_joint_transform(&parent_transform);
            }
            else
            {
                local_animation_transform = joint_component.get_local_transform();
            }

            return parent_transform * local_animation_transform;
        }

        Matrix4::<f32>::identity()
    }

    pub fn get_joint_transform_vec(&self, animated: bool) -> Option<Vec<Matrix4<f32>>>
    {
        if self.skin.len() == 0
        {
            return None;
        }

        // store transforms in a cache -> no complete parent traversal needed for each joint
        let mut transform_cache: HashMap<u32, Matrix4::<f32>> = HashMap::new();

        let mut joints = vec![];
        for joint in &self.skin
        {
            let mut transform = Matrix4::<f32>::identity();

            if let OptionOrId::Some(joint) = joint
            {
                transform = joint.read().unwrap().get_full_joint_transform(Some(&transform_cache), animated);
                transform_cache.insert(joint.read().unwrap().id, transform);

                // inverse bind transform
                let joint_component = joint.read().unwrap().find_component::<Joint>();
                if let Some(joint_component) = joint_component
                {
                    component_downcast!(joint_component, Joint);
                    transform = transform * joint_component.get_inverse_bind_transform();
                }
            }
            else
            {
                console_warning!("Node {} has an empty skin joint with id which is not supported!", &self.name);
            }

            joints.push(transform);
        }

        Some(joints)
    }

    pub fn has_morph_target_weights(&self) -> bool
    {
        self.has_component::<MorphTarget>()
    }

    pub fn get_morph_target_weights_vec(&self) -> Option<Vec<f32>>
    {
        let morph_components = self.find_components::<MorphTarget>();

        if morph_components.len() == 0
        {
            return None;
        }

        let mut morph_target_weights: Vec<(u32, f32)> = vec![];

        for morph_target in morph_components
        {
            component_downcast!(morph_target, MorphTarget);
            let morph_data = morph_target.get_data();
            morph_target_weights.push((morph_data.target_id, morph_data.weight));
        }

        morph_target_weights.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());

        let morph_targets: Vec<f32> = morph_target_weights.iter().map(|morph_target| morph_target.1).collect();

        Some(morph_targets)
    }

    pub fn find_child_node_by_id(&self, id: u32) -> Option<NodeItem>
    {
        for node in &self.nodes
        {
            if node.read().unwrap().id == id
            {
                return Some(node.clone());
            }

            // check child nodes
            let result: Option<Arc<RwLock<Box<Node>>>> = node.read().unwrap().find_child_node_by_id(id);
            if result.is_some()
            {
                return result;
            }
        }

        None
    }

    pub fn find_child_node_by_name(&self, name: &str) -> Option<NodeItem>
    {
        for node in &self.nodes
        {
            if node.read().unwrap().name == name
            {
                return Some(node.clone());
            }

            // check child nodes
            let result = node.read().unwrap().find_child_node_by_name(name);
            if result.is_some()
            {
                return result;
            }
        }
        None
    }

    pub fn find_child_node_by_regex(&self, regex: &str) -> Option<NodeItem>
    {
        let regex_item: Regex = Regex::new(regex).unwrap();

        for node in &self.nodes
        {
            if regex_item.is_match(&node.read().unwrap().name)
            {
                return Some(node.clone());
            }

            // check child nodes
            let result = node.read().unwrap().find_child_node_by_regex(regex);
            if result.is_some()
            {
                return result;
            }
        }
        None
    }

    pub fn find_node_by_id(nodes: &Vec<NodeItem>, id: u32) -> Option<NodeItem>
    {
        for node in nodes
        {
            if node.read().unwrap().id == id
            {
                return Some(node.clone());
            }

            // check child nodes
            let result = Node::find_node_by_id(&node.read().unwrap().nodes, id);
            if result.is_some()
            {
                return result;
            }
        }

        None
    }

    pub fn find_node_by_name(nodes: &Vec<NodeItem>, name: &str) -> Option<NodeItem>
    {
        for node in nodes
        {
            if node.read().unwrap().name == name
            {
                return Some(node.clone());
            }

            // check child nodes
            let result = Node::find_node_by_name(&node.read().unwrap().nodes, name);
            if result.is_some()
            {
                return result;
            }
        }

        None
    }

    pub fn find_mesh_node_by_name(nodes: &Vec<NodeItem>, name: &str) -> Option<NodeItem>
    {
        for node in nodes
        {
            if node.read().unwrap().name == name && node.read().unwrap().has_component::<Mesh>()
            {
                return Some(node.clone());
            }

            // check child nodes
            let result = Node::find_node_by_name(&node.read().unwrap().nodes, name);
            if result.is_some()
            {
                return result;
            }
        }

        None
    }

    pub fn find_mesh_node_by_ids(nodes: &Vec<NodeItem>, ids: &Vec<u32>) -> Option<NodeItem>
    {
        for node in nodes
        {
            if ids.contains(&node.read().unwrap().id) && node.read().unwrap().has_component::<Mesh>()
            {
                return Some(node.clone());
            }

            // check child nodes
            let result = Node::find_mesh_node_by_ids(&node.read().unwrap().nodes, ids);
            if result.is_some()
            {
                return result;
            }
        }

        None
    }

    // find the node which has animations
    pub fn find_animation_node(node: NodeItem) -> Option<NodeItem>
    {
        let node_read = node.read().unwrap();
        if node_read.has_component::<Animation>()
        {
            return Some(node.clone());
        }

        let all_nodes = Scene::list_all_child_nodes(&node_read.nodes);
        for child_node in all_nodes
        {
            let child_node_read = child_node.read().unwrap();
            if child_node_read.has_component::<Animation>()
            {
                return Some(child_node.clone());
            }
        }

        None
    }

    // find animation by name and return first animation if there is no name set
    pub fn find_animation_by_name(&self, name: &str) -> Option<ComponentItem>
    {
        let name = name.to_string();

        // first check on the item itself
        let animations = self.find_components::<Animation>();

        for animation in animations
        {
            let componen_name = animation.read().unwrap().get_base().name.clone();

            if componen_name == name || name == ""
            {
                return Some(animation.clone());
            }
        }

        // second check on nodes
        let all_nodes = Scene::list_all_child_nodes(&self.nodes);
        for node in all_nodes
        {
            let node = node.read().unwrap();
            let animations = node.find_components::<Animation>();

            for animation in animations
            {
                let componen_name = animation.read().unwrap().get_base().name.clone();

                if componen_name == name || name == ""
                {
                    return Some(animation.clone());
                }
            }
        }

        None
    }

    pub fn find_animation_by_regex(&self, regex: &str) -> Option<ComponentItem>
    {
        let regex = Regex::new(regex).unwrap();

        // first check on the item itself
        let animations = self.find_components::<Animation>();

        for animation in animations
        {
            let componen_name = animation.read().unwrap().get_base().name.clone().to_lowercase();

            if regex.is_match(&componen_name)
            {
                return Some(animation.clone());
            }
        }

        // second check on nodes
        let all_nodes = Scene::list_all_child_nodes(&self.nodes);
        for node in all_nodes
        {
            let node = node.read().unwrap();
            let animations = node.find_components::<Animation>();

            for animation in animations
            {
                let componen_name = animation.read().unwrap().get_base().name.clone().to_lowercase();

                if regex.is_match(&componen_name)
                {
                    return Some(animation.clone());
                }
            }
        }

        None
    }

    pub fn find_animations_by_regex(&self, regex: &str) -> Vec<ComponentItem>
    {
        let regex = Regex::new(regex).unwrap();

        let mut animations_found: Vec<ComponentItem> = vec![];

        // first check on the item itself
        let animations = self.find_components::<Animation>();

        for animation in animations
        {
            let componen_name = animation.read().unwrap().get_base().name.clone().to_lowercase();

            if regex.is_match(&componen_name)
            {
                animations_found.push(animation.clone());
            }
        }

        // second check on nodes
        let all_nodes = Scene::list_all_child_nodes(&self.nodes);
        for node in all_nodes
        {
            let node = node.read().unwrap();
            let animations = node.find_components::<Animation>();

            for animation in animations
            {
                let componen_name = animation.read().unwrap().get_base().name.clone().to_lowercase();

                if regex.is_match(&componen_name)
                {
                    animations_found.push(animation.clone());
                }
            }
        }

        animations_found
    }

    pub fn find_animation_by_include_exclude(&self, include: &Vec<String>, exclude: &Vec<String>) -> Option<ComponentItem>
    {
        // first check on the item itself
        let animations = self.find_components::<Animation>();

        for animation in animations
        {
            let componen_name = animation.read().unwrap().get_base().name.clone().to_lowercase();

            if match_by_include_exclude(&componen_name, include, exclude)
            {
                return Some(animation.clone());
            }
        }

        // second check on nodes
        let all_nodes = Scene::list_all_child_nodes(&self.nodes);
        for node in all_nodes
        {
            let node = node.read().unwrap();
            let animations = node.find_components::<Animation>();

            for animation in animations
            {
                let componen_name = animation.read().unwrap().get_base().name.clone().to_lowercase();

                if match_by_include_exclude(&componen_name, include, exclude)
                {
                    return Some(animation.clone());
                }
            }
        }

        None
    }

    pub fn start_all_animations(&self)
    {
        // first check on the item itself
        let animations = self.find_components::<Animation>();

        for animation in animations
        {
            component_downcast_mut!(animation, Animation);
            animation.start();
        }

        // second check on nodes
        let all_nodes = Scene::list_all_child_nodes(&self.nodes);
        for node in all_nodes
        {
            let node = node.read().unwrap();
            let animations = node.find_components::<Animation>();

            for animation in animations
            {
                component_downcast_mut!(animation, Animation);
                animation.start();
            }
        }
    }

    pub fn stop_all_animations(&self)
    {
        // first check on the item itself
        let animations = self.find_components::<Animation>();

        for animation in animations
        {
            component_downcast_mut!(animation, Animation);
            animation.stop();
        }

        // second check on nodes
        let all_nodes = Scene::list_all_child_nodes(&self.nodes);
        for node in all_nodes
        {
            let node = node.read().unwrap();
            let animations = node.find_components::<Animation>();

            for animation in animations
            {
                component_downcast_mut!(animation, Animation);
                animation.stop();
            }
        }
    }

    pub fn get_all_animations(&self) -> Vec<ComponentItem>
    {
        // first check on the item itself
        let mut animations = self.find_components::<Animation>();

        // second check on nodes
        let all_nodes = Scene::list_all_child_nodes(&self.nodes);
        for node in all_nodes
        {
            let node = node.read().unwrap();
            let child_animations = node.find_components::<Animation>();

            animations.extend(child_animations);
        }

        animations
    }

    pub fn start_first_animation(&self)
    {
        let animations = self.get_all_animations();

        if let Some(first) = animations.first()
        {
            component_downcast_mut!(first, Animation);
            first.start();
        }
    }

    pub fn start_animation(&self, name: &str)
    {
        if let Some(animation) = self.find_animation_by_name(name)
        {
            component_downcast_mut!(animation, Animation);
            animation.start();
        }
    }

    pub fn re_target_animations_to_child_nodes(&mut self) -> bool
    {
        let all_animations = self.get_all_animations();

        let mut all_animations_retarteted = true;

        for animation in all_animations
        {
            component_downcast_mut!(animation, Animation);
            for channel in &mut animation.channels
            {
                if channel.target.is_none()
                {
                    console_warning!("target not set for channel ");
                    continue;
                }
                let target = channel.target.as_ref().unwrap();

                let target_name = target.read().unwrap().name.clone();
                let target_node_candidate = self.find_child_node_by_name(target_name.as_str());

                if let Some(target_node_candidate) = target_node_candidate
                {
                    channel.target = OptionOrId::Some(target_node_candidate.clone());
                }
                else
                {
                    all_animations_retarteted = false;
                    console_warning!("not target found for {}", target_name);
                }
            }
        }

        all_animations_retarteted
    }

    pub fn get_alpha(&self) -> (f32, bool)
    {
        let alpha_components = self.find_components::<Alpha>();

        let mut alpha = 1.0;
        let mut inheritance = true;

        for alpha_component in alpha_components
        {
            component_downcast!(alpha_component, Alpha);

            if alpha_component.get_base().is_enabled
            {
                if !alpha_component.has_alpha_inheritance()
                {
                    inheritance = false;
                }

                alpha *= alpha_component.get_alpha();
            }
        }

        (alpha, inheritance)
    }

    pub fn get_full_alpha(node: NodeItem) -> f32
    {
        let node = node.read().unwrap();

        let (node_alpha, node_parent_inheritance) = node.get_alpha();
        let mut parent_alpha = 1.0;

        if let Some(parent_node) = node.parent.as_ref()
        {
            parent_alpha = Self::get_full_alpha(parent_node.clone());
        }

        if node_parent_inheritance
        {
            parent_alpha * node_alpha
        }
        else
        {
            node_alpha
        }
    }

    pub fn is_empty(&self) -> bool
    {
        let has_meshes = self.get_mesh().is_some();

        if has_meshes
        {
            return false;
        }
        else if !has_meshes && self.nodes.len() == 0
        {
            return true;
        }

        let mut is_not_empty = false;
        for node in &self.nodes
        {
            let node = node.read().unwrap();
            is_not_empty = is_not_empty || !node.is_empty();
        }

        !is_not_empty
    }

    pub fn create_default_instance(&mut self, self_node_item: NodeItem) -> Arc<RwLock<InstanceItem>>
    {
        let mut instance = Instance::new
        (
            "instance".to_string(),
            self_node_item
        );

        instance.is_default = true;
        let instance_arc = self.add_instance(Box::new(instance));

        instance_arc
    }

    pub fn add_instance(&mut self, instance: InstanceItem) -> Arc<RwLock<InstanceItem>>
    {
        let instance = Arc::new(RwLock::new(instance));
        self.instances.get_mut().push(instance.clone());

        instance
    }

    pub fn update(node: NodeItem, io: &mut InputOutput, time: u128, frame_scale: f32, frame: u64) -> NodeUpdateResult
    {
        // ***** copy all components *****
        let mut all_components;
        {
            let node = node.write().unwrap();
            all_components = node.components.clone();
        }

        let mut delete_components = vec![];

        for (component_id, component) in all_components.clone().iter_mut().enumerate()
        {
            if component.read().unwrap().get_base().delete_later_request
            {
                delete_components.push(component.read().unwrap().id());
            }

            {
                if !component.read().unwrap().is_enabled()
                {
                    continue;
                }
            }

            // remove the component itself  for the component update
            // otherwise this can cause read/write issues (its opened as write and it maybe is requested as read in a loop)
            {
                let mut node = node.write().unwrap();
                node.components = all_components.clone();
                node.components.remove(component_id);
            }

            // component update
            {
                let mut component_write = component.write().unwrap();
                component_write.update(node.clone(), io, time, frame_scale, frame);
            }

            // after each update, check if new components were added during the update --> add
            let maybe_new_components = &node.read().unwrap().components;
            find_and_add_new_components(&mut all_components, maybe_new_components);
        }

        // ***** reassign components *****
        {
            let mut node = node.write().unwrap();
            node.components = all_components;
        }

        // ***** delete components *****
        {
            let mut node = node.write().unwrap();
            node.remove_components_by_ids(&delete_components);
        }

        // ***** update instances *****
        {
            let mut updates = 0;
            {
                let node_read = node.read().unwrap();
                for instance in node_read.instances.get_ref()
                {
                    if Instance::update(&instance, io, time, frame_scale, frame)
                    {
                        updates += 1;
                    }
                }
            }

            // if more than UPDATE_ALL_INSTANCES_THRESHOLD instances got an update -> update all instances at once to save performance
            if updates >= UPDATE_ALL_INSTANCES_THRESHOLD
            {
                let mut node = node.write().unwrap();
                node.instances.force_change();
            }
        }

        // check for delete later
        let mut delete_nodes = vec![];
        {
            let node = node.read().unwrap();
            if node.delete_later_request
            {
                delete_nodes.push(node.id);
            }
        }

        // ***** update childs *****
        let node_read = node.read().unwrap();
        for child_node in &node_read.nodes
        {
            let mut update_result = Self::update(child_node.clone(), io, time, frame_scale, frame);

            if update_result.delete_nodes.len() > 0
            {
                delete_nodes.append(&mut update_result.delete_nodes);
            }
        }

        NodeUpdateResult { delete_nodes:  delete_nodes}
    }

    pub fn merge_mesh(&mut self, node: &NodeItem) -> bool
    {
        let merge_read = node.read().unwrap();
        let merge_mesh = merge_read.find_component::<Mesh>();
        let current_mesh = self.find_component::<Mesh>();

        if current_mesh.is_none() || merge_mesh.is_none()
        {
            console_warning!("can not merge node -> can not merge empty mesh");
            return false;
        }

        let merge_mesh = merge_mesh.unwrap();
        let current_mesh = current_mesh.unwrap();

        component_downcast!(merge_mesh, Mesh);
        component_downcast_mut!(current_mesh, Mesh);

        if current_mesh.mesh_resource.is_some() && merge_mesh.mesh_resource.is_some()
        {
            let current_mesh_res = current_mesh.mesh_resource.as_ref().unwrap();
            let mut current_mesh_res = current_mesh_res.write().unwrap();

            current_mesh_res.merge(merge_mesh.mesh_resource.as_ref().unwrap().read().unwrap().as_ref().get_data());
            current_mesh_res.calc_hash();

            return true;
        }

        false
    }

    pub fn merge_instances(&mut self) -> bool
    {
        let meshes = self.get_meshes();

        if meshes.len() == 0
        {
            return false;
        }

        if self.instances.get_ref().len() == 0
        {
            return false;
        }

        // get all transformations
        let mut transformations = vec![];

        let instances = self.instances.get_ref();
        for instance in instances
        {
            let instance = instance.read().unwrap();

            let mut matrix = Matrix4::<f32>::identity();

            let transform_component = instance.find_component::<Transformation>();

            if let Some(transform_component) = transform_component
            {
                component_downcast_mut!(transform_component, Transformation);

                // force update
                transform_component.calc_transform();
                matrix = transform_component.get_transform().clone();
            }

            transformations.push(matrix);
        }

        // apply all transformations
        for mesh in meshes
        {
            component_downcast_mut!(mesh, Mesh);

            if let Some(mesh_resource) = mesh.mesh_resource.as_ref()
            {
                let mut mesh_resource = mesh_resource.write().unwrap();
                mesh_resource.merge_by_transformations(&transformations);
                mesh_resource.calc_hash();
            }
        }

        // clear and create new single instance
        let node;
        {
            let first_instance = self.instances.get_ref().first().unwrap();
            let first_instance = first_instance.read().unwrap();

            node = first_instance.node.clone();
        }

        self.clear_instances();

        if let Some(node) = node.as_ref()
        {
            self.create_default_instance(node.clone());
        }
        else
        {
            console_warning!("merge_instances: Node has no parent node to create default instance");
        }

        true
    }

    pub fn force_instances_update(&mut self)
    {
        for instance in self.instances.get_ref()
        {
            let mut instance = instance.write().unwrap();
            instance.set_force_update();
        }

        let all_nodes = Scene::list_all_child_nodes(&self.nodes);

        for node in all_nodes
        {
            let node = node.read().unwrap();
            for instance in node.instances.get_ref()
            {
                let mut instance = instance.write().unwrap();
                instance.set_force_update();
            }
        }
    }

    pub fn find_instance_by_id(&self, id: u32) -> Option<&InstanceItemArc>
    {
        for instance in self.instances.get_ref()
        {
            if instance.read().unwrap().id == id
            {
                return Some(instance);
            }
        }

        None
    }

    pub fn delete_instance_by_id(&mut self, id: u32) -> bool
    {
        let len = self.instances.get_ref().len();
        self.instances.get_mut().retain(|instance|
        {
            instance.read().unwrap().id != id
        });

        self.instances.get_ref().len() != len
    }

    pub fn clear_instances(&mut self)
    {
        self.instances.get_mut().clear();
    }

    pub fn delete_child_node_by_id(&mut self, id: u32) -> bool
    {
        {
            let node = Node::find_node_by_id(&self.nodes, id);
            if let Some(node_arc) = node
            {
                let mut all_nodes;
                {
                    let node = node_arc.read().unwrap();
                    all_nodes = Scene::list_all_child_nodes(&node.nodes);
                    all_nodes.push(node_arc.clone());
                }
                Node::cleanup_cyclic_references(&all_nodes);
            }
        }

        let len = self.nodes.len();
        self.nodes.retain(|node|
        {
            node.read().unwrap().id != id
        });

        if self.nodes.len() != len
        {
            return true;
        }

        // if not found -> check children
        for node in &self.nodes
        {
            let deleted = node.write().unwrap().delete_child_node_by_id(id);

            if deleted
            {
                return true;
            }
        }

        false
    }

    pub fn find_root_node(node: NodeItem) -> Option<NodeItem>
    {
        if node.read().unwrap().root_node
        {
            return Some(node);
        }

        if let Some(parent) = node.read().unwrap().parent.as_ref()
        {
            return Self::find_root_node(parent.clone());
        }

        None
    }

    pub fn get_transform_between_root_joint_and_root_node(node: NodeItem) -> Matrix4<f32>
    {
        // get the transform between a joint_root and a root_node
        // these transforms are normally not treated in AdditiveComponentAbsolute joint animations
        let mut transform = Matrix4::identity();

        let mut start_multiply = false;

        let mut current_node = node.clone();

        loop
        {
            let is_root_node = current_node.read().unwrap().root_node;
            if is_root_node
            {
                break;
            }

            let mut is_joint = false;
            if let Some(joint_component) = current_node.read().unwrap().find_component::<Joint>()
            {
                component_downcast!(joint_component, Joint);
                if joint_component.get_data().root_joint
                {
                    start_multiply = true;
                    is_joint = true;
                }
            }

            if start_multiply && !is_root_node && !is_joint
            {
                let (node_transform, node_parent_inheritance) = current_node.read().unwrap().get_transform();

                if node_parent_inheritance
                {
                    //transform = transform * node_transform
                    transform = node_transform * transform;
                }
                else
                {
                    transform = node_transform;
                }
            }

            if let Some(parent) = current_node.clone().read().unwrap().parent.as_ref()
            {
                current_node = parent.clone();
            }
            else
            {
                break;
            }
        }

        transform
    }

    pub fn print(&self, level: usize)
    {
        let spaces = " ".repeat(level * 2);
        console_log!("{} - (NODE) id={} name={} components={}, instances={}", spaces, self.id, self.name, self.components.len(), self.instances.get_ref().len());

        for node in &self.nodes
        {
            node.read().unwrap().print(level + 1);
        }
    }
}