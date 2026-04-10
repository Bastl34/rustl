use std::sync::{Arc, RwLock};

use serde::{Deserialize, Deserializer, Serializer};

use crate::{helper::option_or_id::OptionOrId, state::{resources::{mesh_resource::MeshResourceItem, sound_source::SoundSourceItem, texture::TextureItem}, scene::{components::component::{Component, ComponentItem}, node::NodeItem}}};


pub fn default_true() -> bool { true }
pub fn is_true(v: &bool) -> bool { *v }
pub fn is_false(v: &bool) -> bool { !*v }


// ******************** node serialization ********************

pub fn serialize_node<S>(item: &OptionOrId<NodeItem>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match item
    {
        OptionOrId::Some(item) =>
        {
            let guard = item.read().map_err(serde::ser::Error::custom)?;
            serializer.serialize_str(&guard.uuid)
        }
        OptionOrId::Id(uuid) =>
        {
            serializer.serialize_str(uuid)
        }
        OptionOrId::None => serializer.serialize_none(),
    }
}


pub fn deserialize_node<'de, D>(deserializer: D) -> Result<OptionOrId<NodeItem>, D::Error>
where
    D: Deserializer<'de>,
{
    let uuid_opt = Option::<String>::deserialize(deserializer)?;

    if let Some(uuid) = uuid_opt
    {
        Ok(OptionOrId::from_id(uuid))
    }
    else
    {
        Ok(OptionOrId::None)
    }
}

// ******************** sound source serialization ********************

pub fn serialize_sound_source<S>(item: &OptionOrId<SoundSourceItem>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match item
    {
        OptionOrId::Some(item) =>
        {
            let guard = item.read().map_err(serde::ser::Error::custom)?;
            serializer.serialize_str(&guard.uuid)
        }
        OptionOrId::Id(uuid) =>
        {
            serializer.serialize_str(uuid)
        }
        OptionOrId::None => serializer.serialize_none(),
    }
}


pub fn deserialize_sound_source<'de, D>(deserializer: D) -> Result<OptionOrId<SoundSourceItem>, D::Error>
where
    D: Deserializer<'de>,
{
    let uuid_opt = Option::<String>::deserialize(deserializer)?;

    if let Some(uuid) = uuid_opt
    {
        Ok(OptionOrId::from_id(uuid))
    }
    else
    {
        Ok(OptionOrId::None)
    }
}

// ******************** mesh serialization ********************

pub fn serialize_mesh_resource<S>(item: &OptionOrId<MeshResourceItem>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match item
    {
        OptionOrId::Some(item) =>
        {
            let guard = item.read().map_err(serde::ser::Error::custom)?;
            serializer.serialize_str(&guard.uuid)
        }
        OptionOrId::Id(uuid) =>
        {
            serializer.serialize_str(uuid)
        }
        OptionOrId::None => serializer.serialize_none(),
    }
}


pub fn deserialize_mesh_resource<'de, D>(deserializer: D) -> Result<OptionOrId<MeshResourceItem>, D::Error>
where
    D: Deserializer<'de>,
{
    let uuid_opt = Option::<String>::deserialize(deserializer)?;

    if let Some(uuid) = uuid_opt
    {
        Ok(OptionOrId::from_id(uuid))
    }
    else
    {
        Ok(OptionOrId::None)
    }
}

// ******************** texture serialization ********************

pub fn serialize_texture<S>(item: &OptionOrId<TextureItem>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match item
    {
        OptionOrId::Some(item) =>
        {
            let guard = item.read().map_err(serde::ser::Error::custom)?;
            serializer.serialize_str(&guard.uuid)
        }
        OptionOrId::Id(uuid) =>
        {
            serializer.serialize_str(uuid)
        }
        OptionOrId::None => serializer.serialize_none(),
    }
}


pub fn deserialize_texture<'de, D>(deserializer: D) -> Result<OptionOrId<TextureItem>, D::Error>
where
    D: Deserializer<'de>,
{
    let uuid_opt = Option::<String>::deserialize(deserializer)?;

    if let Some(uuid) = uuid_opt
    {
        Ok(OptionOrId::from_id(uuid))
    }
    else
    {
        Ok(OptionOrId::None)
    }
}

// ******************** components serialization ********************

pub fn serialize_component<S>(item: &OptionOrId<ComponentItem>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match item
    {
        OptionOrId::Some(item) =>
        {
            let guard = item.read().map_err(serde::ser::Error::custom)?;
            serializer.serialize_str(&guard.get_base().uuid)
        }
        OptionOrId::Id(uuid) =>
        {
            serializer.serialize_str(uuid)
        }
        OptionOrId::None => serializer.serialize_none(),
    }
}


pub fn deserialize_component<'de, D>(deserializer: D) -> Result<OptionOrId<ComponentItem>, D::Error>
where
    D: Deserializer<'de>,
{
    let uuid_opt = Option::<String>::deserialize(deserializer)?;

    if let Some(uuid) = uuid_opt
    {
        Ok(OptionOrId::from_id(uuid))
    }
    else
    {
        Ok(OptionOrId::None)
    }
}

pub fn serialize_component_vec<S>(items: &Vec<ComponentItem>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    use serde::ser::SerializeSeq;

    let mut seq = serializer.serialize_seq(Some(items.len()))?;
    for item in items
    {
        if !item.read().unwrap().get_base().export
        {
            continue;
        }

        let guard = item.read().map_err(serde::ser::Error::custom)?;
        seq.serialize_element(&*guard)?;
    }
    seq.end()
}


pub fn deserialize_component_vec<'de, D>(deserializer: D) -> Result<Vec<ComponentItem>, D::Error>
where
    D: Deserializer<'de>,
{
    let raw_components: Vec<Box<dyn Component>> = Deserialize::deserialize(deserializer)?;
    Ok(raw_components
        .into_iter()
        .map(|c| Arc::new(RwLock::new(c)))
        .collect())
}

// ******************** arc rw hashmap serialization ********************

#[macro_export] macro_rules! impl_arc_rwbox_map_serializer
{
    ($name:ident, $key:ty, $value:ty) =>
    {
        pub struct $name<'a>
        {
            pub map: &'a HashMap<$key, Arc<RwLock<Box<$value>>>>,
        }

        impl<'a> Serialize for $name<'a>
        {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: serde::Serializer,
            {
                let mut ser_map = serializer.serialize_map(Some(self.map.len()))?;
                for (k, v) in self.map
                {
                    let guard = v.read().map_err(serde::ser::Error::custom)?;
                    ser_map.serialize_entry(k, &**guard)?;
                }
                ser_map.end()
            }
        }
    };
}