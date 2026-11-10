#![allow(dead_code)]

//! What the editor saves of nodes, instances and components - and of loaded assets only the diff (overrides).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Arc, RwLock};

use nalgebra::{Vector3, Vector4};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::console_error;
use crate::state::scene::components::component::{Component, ComponentBox, ComponentItem, DeserializationContext};
use crate::state::scene::components::material::{Material, MaterialItem};
use crate::state::scene::components::transformation::Transformation;
use crate::state::scene::instance::{Instance, InstanceData};
use crate::state::scene::manager::id_manager;
use crate::state::scene::node::{InstanceItemArc, Node, NodeItem, NodeSettings};
use crate::state::scene::scene::Scene;
use crate::state::scene::utilities::origin::Origin;

// ******************** asset info ********************

// asset data on the root node of a loaded asset
pub struct AssetInfo
{
    pub materials: Vec<MaterialItem>, // own materials - not the ones reused from other assets
    pub baseline: Option<AssetBaseline>, // editor only: state after loading, overrides are the diff to it
    pub orphans: Vec<Value>, // overrides without asset node - saved again
}

pub struct AssetBaselineNode
{
    pub node_id: u32,
    pub parent_id: Option<u32>,
    pub name: String,
    pub state: Value,
}

pub struct AssetBaseline
{
    pub nodes: BTreeMap<String, AssetBaselineNode>, // by asset key - "" is the root
    pub materials: BTreeMap<String, Value>,
}

// ******************** states ********************

#[derive(Serialize, Deserialize, Clone, PartialEq)]
pub struct TransformState
{
    pub position: Vector3<f32>,
    pub rotation: Vector3<f32>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotation_quat: Option<Vector4<f32>>,

    pub scale: Vector3<f32>,

    #[serde(default = "default_true")]
    pub parent_inheritance: bool,
}

fn default_true() -> bool { true }

#[derive(Serialize, Deserialize)]
pub struct InstanceState
{
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub uuid: String,

    pub name: String,
    pub pickable: bool,
    pub data: InstanceData,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transform: Option<TransformState>,

    // scene components besides the transformation - serde of the runtime types
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub components: Vec<Value>,
}

// node fields are None for the asset root
#[derive(Serialize, Deserialize, Default)]
pub struct NodeState
{
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<NodeSettings>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<Vector3<f32>>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transform: Option<TransformState>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub materials: Option<Vec<String>>, // uuids

    // asset components and instances by asset key
    #[serde(default)]
    pub components: BTreeMap<String, Value>,

    #[serde(default)]
    pub instances: BTreeMap<String, Value>,
}

// ******************** json diff (RFC 7396 merge patch) ********************

// patch from base to current - None if equal
pub fn merge_diff(base: &Value, current: &Value) -> Option<Value>
{
    match (base, current)
    {
        (Value::Object(base), Value::Object(current)) =>
        {
            let mut patch = Map::new();

            for (key, value) in current
            {
                match base.get(key)
                {
                    Some(base_value) => if let Some(diff) = merge_diff(base_value, value) { patch.insert(key.clone(), diff); },
                    None => { patch.insert(key.clone(), value.clone()); },
                }
            }

            for key in base.keys().filter(|key| !current.contains_key(*key))
            {
                patch.insert(key.clone(), Value::Null);
            }

            if patch.is_empty() { None } else { Some(Value::Object(patch)) }
        },
        _ => if base == current { None } else { Some(current.clone()) },
    }
}

pub fn merge_patch(target: &mut Value, patch: &Value)
{
    let Value::Object(patch) = patch else { *target = patch.clone(); return; };

    if !target.is_object()
    {
        *target = Value::Object(Map::new());
    }

    let target = target.as_object_mut().unwrap();
    for (key, value) in patch
    {
        if value.is_null()
        {
            target.remove(key);
        }
        else
        {
            merge_patch(target.entry(key.clone()).or_insert(Value::Null), value);
        }
    }
}

// ******************** components ********************

// saved as serde - not the transformation (own field) and not materials (by uuid)
pub fn is_component_value_saved(component: &dyn Component) -> bool
{
    let base = component.get_base();
    base.export && !base.delete_later_request && component.is_serializable()
        && component.as_any().downcast_ref::<Transformation>().is_none()
        && component.as_any().downcast_ref::<Material>().is_none()
}

pub fn component_value(component: &ComponentItem) -> Option<Value>
{
    let component = component.read().unwrap();

    match serde_json::to_value(&*component)
    {
        Ok(value) => Some(value),
        Err(e) => { console_error!("failed to serialize component '{}': {}", component.get_base().name, e); None },
    }
}

// scene components of a node or an instance
pub fn scene_component_values(components: &[ComponentItem]) -> Vec<Value>
{
    components.iter()
        .filter(|component| { let component = component.read().unwrap(); component.get_base().origin == Origin::Scene && is_component_value_saved(&**component) })
        .filter_map(component_value)
        .collect()
}

pub fn component_from_value(value: Value, context: &mut DeserializationContext) -> Option<ComponentItem>
{
    match serde_json::from_value::<ComponentBox>(value)
    {
        Ok(mut component) =>
        {
            component.get_base_mut().id = id_manager::get_next_component_id();
            component.run_after_deserialize(context);

            let component: ComponentItem = Arc::new(RwLock::new(component));
            context.components.push(component.clone());
            Some(component)
        },
        Err(e) => { console_error!("failed to parse component: {}", e); None },
    }
}

// replaces the component content in place - shared references stay shared
pub fn apply_component_value(item: &ComponentItem, value: Value, context: &mut DeserializationContext) -> bool
{
    let mut component = match serde_json::from_value::<ComponentBox>(value)
    {
        Ok(component) => component,
        Err(e) => { console_error!("failed to apply component '{}': {}", item.read().unwrap().get_base().name, e); return false; },
    };

    // before the lock - reads other components
    component.run_after_deserialize(context);

    let mut old = item.write().unwrap();
    {
        let old_base = old.get_base();
        let base = component.get_base_mut();
        base.id = old_base.id;
        base.origin = old_base.origin;
        base.asset_key = old_base.asset_key.clone();
        base.component_name = old_base.component_name.clone();
        base.icon = old_base.icon.clone();
    }

    component.keep_unsaved(&mut **old);
    *old = component;

    true
}

// ******************** transform ********************

pub fn transform_state(components: &[ComponentItem]) -> Option<TransformState>
{
    let transformation = components.iter().find(|component| component.read().unwrap().as_any().downcast_ref::<Transformation>().is_some())?;
    let transformation = transformation.read().unwrap();
    let data = transformation.as_any().downcast_ref::<Transformation>().unwrap().get_data();

    Some(TransformState
    {
        position: data.position,
        rotation: data.rotation,
        rotation_quat: data.rotation_quat,
        scale: data.scale,
        parent_inheritance: data.parent_inheritance,
    })
}

// sets the transformation - adds one if missing
pub fn apply_transform_state(components: &mut Vec<ComponentItem>, state: &TransformState)
{
    let existing = components.iter().find(|component| component.read().unwrap().as_any().downcast_ref::<Transformation>().is_some()).cloned();
    let item = existing.unwrap_or_else(||
    {
        let item: ComponentItem = Arc::new(RwLock::new(Box::new(Transformation::identity("Transform"))));
        components.push(item.clone());
        item
    });

    let mut component = item.write().unwrap();
    let transformation = component.as_any_mut().downcast_mut::<Transformation>().unwrap();
    {
        let data = transformation.get_data_mut().get_mut();
        data.position = state.position;
        data.rotation = state.rotation;
        data.rotation_quat = state.rotation_quat;
        data.scale = state.scale;
        data.parent_inheritance = state.parent_inheritance;
    }
    transformation.calc_transform();
}

// ******************** instances ********************

pub fn instance_state(instance: &Instance) -> InstanceState
{
    InstanceState
    {
        uuid: instance.uuid.clone(),
        name: instance.name.clone(),
        pickable: instance.pickable,
        data: instance.get_data().clone(),

        transform: transform_state(&instance.components),
        components: scene_component_values(&instance.components),
    }
}

pub fn instance_value(instance: &InstanceItemArc) -> Option<Value>
{
    serde_json::to_value(instance_state(&instance.read().unwrap())).ok()
}

pub fn apply_instance_state(instance: &InstanceItemArc, state: InstanceState, context: &mut DeserializationContext)
{
    let components: Vec<ComponentItem> = state.components.into_iter().filter_map(|value| component_from_value(value, context)).collect();

    let mut instance = instance.write().unwrap();

    if !state.uuid.is_empty() && instance.origin != Origin::Asset
    {
        instance.uuid = state.uuid;
    }

    instance.name = state.name;
    instance.pickable = state.pickable;

    // keeps the runtime fields
    {
        let data = instance.get_data_mut().get_mut();
        let computed = std::mem::take(&mut data.computed);
        let highlight = data.highlight;

        *data = InstanceData { computed, highlight, ..state.data };
    }

    if let Some(transform) = &state.transform
    {
        apply_transform_state(&mut instance.components, transform);
    }

    // replaces all scene components
    instance.components.retain(|component| { let component = component.read().unwrap(); component.get_base().origin != Origin::Scene || !is_component_value_saved(&**component) });
    for component in components
    {
        component.write().unwrap().get_base_mut().origin = Origin::Scene;
        instance.add_component(component);
    }

    instance.set_force_update();
}

// adds a scene instance from a scene file value
pub fn add_instance_from_value(node: &NodeItem, value: Value, context: &mut DeserializationContext)
{
    let state = match serde_json::from_value::<InstanceState>(value)
    {
        Ok(state) => state,
        Err(e) => { console_error!("failed to parse instance of '{}': {}", node.read().unwrap().name, e); return; },
    };

    let mut instance = Instance::new(state.name.clone(), node.clone());
    instance.origin = Origin::Scene;

    let instance = node.write().unwrap().add_instance(Box::new(instance));
    apply_instance_state(&instance, state, context);
}

// ******************** nodes ********************

pub fn material_uuids(node: &Node) -> Vec<String>
{
    node.components.iter()
        .filter(|component| component.read().unwrap().as_any().downcast_ref::<Material>().is_some())
        .map(|component| component.read().unwrap().get_base().uuid.clone())
        .collect()
}

// node_fields: false for the asset root (only asset components and instances)
pub fn node_state(node: &Node, node_fields: bool) -> NodeState
{
    let components = node.components.iter()
        .filter(|component| { let component = component.read().unwrap(); component.get_base().origin == Origin::Asset && is_component_value_saved(&**component) })
        .filter_map(|component| Some((component.read().unwrap().get_base().asset_key.clone(), component_value(component)?)))
        .collect();

    let instances = node.instances.get_ref().iter()
        .filter(|instance| instance.read().unwrap().origin == Origin::Asset)
        .filter_map(|instance| Some((instance.read().unwrap().asset_key.clone(), instance_value(instance)?)))
        .collect();

    let mut state = NodeState { components, instances, ..Default::default() };

    if node_fields
    {
        state.name = Some(node.name.clone());
        state.settings = Some(node.settings.clone());
        state.color = node.color;
        state.transform = transform_state(&node.components);
        state.materials = Some(material_uuids(node));
    }

    state
}

pub fn node_state_value(node: &Node, node_fields: bool) -> Value
{
    serde_json::to_value(node_state(node, node_fields)).unwrap_or(Value::Null)
}

pub fn set_node_materials(node: &NodeItem, uuids: &[String], context: &DeserializationContext)
{
    let materials: Vec<MaterialItem> = uuids.iter().filter_map(|uuid|
    {
        let material = context.scene.materials.values().find(|material| material.read().unwrap().get_base().uuid == *uuid).cloned();
        if material.is_none()
        {
            console_error!("node '{}': material {} not found", node.read().unwrap().name, uuid);
        }
        material
    }).collect();

    let mut node = node.write().unwrap();
    node.remove_components_by_type::<Material>();
    for material in materials
    {
        node.add_component(material);
    }
}

// applies a node state to an asset node or the asset root
pub fn apply_node_state(node_item: &NodeItem, state: NodeState, context: &mut DeserializationContext)
{
    {
        let mut node = node_item.write().unwrap();

        if let Some(name) = state.name
        {
            node.name = name;
            node.color = state.color;
        }

        if let Some(settings) = state.settings { node.settings = settings; }

        if let Some(transform) = &state.transform
        {
            apply_transform_state(&mut node.components, transform);
        }
    }

    if let Some(materials) = &state.materials
    {
        if *materials != material_uuids(&node_item.read().unwrap())
        {
            set_node_materials(node_item, materials, context);
        }
    }

    // asset components: replaces changed ones, removes missing ones
    let components: Vec<ComponentItem> = node_item.read().unwrap().components.iter()
        .filter(|component| { let component = component.read().unwrap(); component.get_base().origin == Origin::Asset && is_component_value_saved(&**component) })
        .cloned()
        .collect();

    for component in components
    {
        let key = component.read().unwrap().get_base().asset_key.clone();
        match state.components.get(&key)
        {
            Some(value) => if component_value(&component).as_ref() != Some(value) { apply_component_value(&component, value.clone(), context); },
            None => { let id = component.read().unwrap().id(); node_item.write().unwrap().remove_component_by_id(id); },
        }
    }

    // asset instances
    let instances: Vec<InstanceItemArc> = node_item.read().unwrap().instances.get_ref().iter().filter(|instance| instance.read().unwrap().origin == Origin::Asset).cloned().collect();
    for instance in instances
    {
        let key = instance.read().unwrap().asset_key.clone();
        match state.instances.get(&key).map(|value| serde_json::from_value::<InstanceState>(value.clone()))
        {
            Some(Ok(instance_state)) => apply_instance_state(&instance, instance_state, context),
            Some(Err(e)) => { console_error!("failed to apply instance {}: {}", key, e); },
            None => { let id = instance.read().unwrap().id; node_item.write().unwrap().delete_instance_by_id(id); },
        }
    }

    node_item.write().unwrap().force_instances_update();
}

// ******************** asset identity ********************

pub fn derived_uuid(parent_uuid: &str, key: &str) -> String
{
    uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_OID, format!("{}/{}", parent_uuid, key).as_bytes()).to_string()
}

// asset nodes below the root - stops at non asset nodes
pub fn asset_nodes(root: &Node) -> Vec<NodeItem>
{
    let mut result = vec![];
    let mut stack: Vec<NodeItem> = root.nodes.iter().rev().cloned().collect();

    while let Some(node) = stack.pop()
    {
        let read = node.read().unwrap();
        if read.origin != Origin::Asset
        {
            continue;
        }

        stack.extend(read.nodes.iter().rev().cloned());
        drop(read);
        result.push(node);
    }

    result
}

fn mark_components(components: &[ComponentItem])
{
    let mut counts: HashMap<String, usize> = HashMap::new();
    for component in components
    {
        let mut component = component.write().unwrap();

        // materials are marked via the own materials list
        if component.as_any().downcast_ref::<Material>().is_some()
        {
            continue;
        }

        let base = component.get_base_mut();
        if base.origin != Origin::Runtime
        {
            continue;
        }

        let count = counts.entry(base.component_name.clone()).or_insert(0);
        base.origin = Origin::Asset;
        base.asset_key = format!("{}{}", base.component_name, count);
        *count += 1;
    }
}

// marks everything below the root as asset with a unique key
pub fn mark_asset(root: &NodeItem, materials: Vec<MaterialItem>)
{
    for material in &materials
    {
        material.write().unwrap().get_base_mut().origin = Origin::Asset;
    }

    let nodes = Scene::list_all_child_nodes(&root.read().unwrap().nodes);

    let mut keys = HashSet::new();
    for (index, node) in nodes.iter().enumerate()
    {
        let mut node = node.write().unwrap();
        node.origin = Origin::Asset;

        // glTF scenes can share nodes - makes the key unique
        if node.asset_key.is_empty() || keys.contains(&node.asset_key)
        {
            node.asset_key = format!("{}_{}", node.asset_key, index);
        }
        keys.insert(node.asset_key.clone());
    }

    for node in nodes.iter().chain(std::iter::once(root))
    {
        let node = node.read().unwrap();
        mark_components(&node.components);

        for (index, instance) in node.instances.get_ref().iter().enumerate()
        {
            let mut instance = instance.write().unwrap();
            instance.origin = Origin::Asset;
            instance.asset_key = format!("i{}", index);
        }
    }

    root.write().unwrap().asset = Some(Box::new(AssetInfo { materials, baseline: None, orphans: vec![] }));
}

fn assign_component_uuids(node_uuid: &str, components: &[ComponentItem])
{
    for component in components
    {
        let mut component = component.write().unwrap();

        // materials get their uuid via the own materials list
        if component.as_any().downcast_ref::<Material>().is_some()
        {
            continue;
        }

        let base = component.get_base_mut();
        if base.origin == Origin::Asset && !base.asset_key.is_empty()
        {
            base.uuid = derived_uuid(node_uuid, &base.asset_key);
        }
    }
}

// derives the uuids of the asset objects from the root uuid
pub fn assign_asset_uuids(root: &NodeItem)
{
    let root_read = root.read().unwrap();
    let root_uuid = root_read.uuid.clone();

    assign_component_uuids(&root_uuid, &root_read.components);

    if let Some(asset) = &root_read.asset
    {
        for material in &asset.materials
        {
            let mut material = material.write().unwrap();
            let base = material.get_base_mut();
            if !base.asset_key.is_empty()
            {
                base.uuid = derived_uuid(&root_uuid, &base.asset_key);
            }
        }
    }

    for node in asset_nodes(&root_read)
    {
        let mut node = node.write().unwrap();
        node.uuid = derived_uuid(&root_uuid, &node.asset_key);

        assign_component_uuids(&node.uuid, &node.components);

        let node_uuid = node.uuid.clone();
        for instance in node.instances.get_ref()
        {
            let mut instance = instance.write().unwrap();
            if instance.origin == Origin::Asset
            {
                instance.uuid = derived_uuid(&node_uuid, &instance.asset_key);
            }
        }
    }
}

// editor only - stores the state the overrides are diffed against
pub fn capture_asset_baseline(root: &NodeItem)
{
    let mut nodes = BTreeMap::new();
    let mut materials = BTreeMap::new();

    {
        let root_read = root.read().unwrap();
        nodes.insert(String::new(), AssetBaselineNode { node_id: root_read.id, parent_id: None, name: root_read.name.clone(), state: node_state_value(&root_read, false) });

        for node in asset_nodes(&root_read)
        {
            let node = node.read().unwrap();
            let parent_id = node.parent.as_ref().map(|parent| parent.read().unwrap().id);
            nodes.insert(node.asset_key.clone(), AssetBaselineNode { node_id: node.id, parent_id, name: node.name.clone(), state: node_state_value(&node, true) });
        }

        if let Some(asset) = &root_read.asset
        {
            for material in &asset.materials
            {
                if let Some(value) = component_value(material)
                {
                    materials.insert(material.read().unwrap().get_base().asset_key.clone(), value);
                }
            }
        }
    }

    if let Some(asset) = root.write().unwrap().asset.as_mut()
    {
        asset.baseline = Some(AssetBaseline { nodes, materials });
    }
}

// finds the asset node by key and name - fallback by name only if the name is unique in the asset
pub fn find_asset_node(root: &NodeItem, key: &str, name: &str) -> Option<NodeItem>
{
    if key.is_empty()
    {
        return Some(root.clone());
    }

    let nodes = asset_nodes(&root.read().unwrap());

    if let Some(node) = nodes.iter().find(|node| { let node = node.read().unwrap(); node.asset_key == key && node.name == name })
    {
        return Some(node.clone());
    }

    let mut same_name = nodes.iter().filter(|node| node.read().unwrap().name == name);
    match (same_name.next(), same_name.next())
    {
        (Some(node), None) => Some(node.clone()),
        _ => None,
    }
}
