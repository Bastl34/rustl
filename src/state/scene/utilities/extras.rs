#![allow(dead_code)]

use std::collections::{hash_map::Iter, HashMap};

use nalgebra::{Vector2, Vector3, Vector4};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExtraType
{
    Bool(bool),
    String(String),
    Int32(i32),
    Int64(i64),
    UInt32(u32),
    UInt64(u64),
    USize(usize),
    Float32(f32),
    Float64(f64),
    Vec2(Vector2<f32>),
    Vec3(Vector3<f32>),
    Vec4(Vector4<f32>),
}

#[derive(Clone, Serialize, Deserialize, Default)]
#[serde(transparent)]
pub struct Extras
{
    pub extras: HashMap<String, ExtraType>,
}

impl From<bool> for ExtraType
{
    fn from(value: bool) -> Self
    {
        ExtraType::Bool(value)
    }
}

impl From<String> for ExtraType
{
    fn from(value: String) -> Self
    {
        ExtraType::String(value)
    }
}

impl From<i32> for ExtraType
{
    fn from(value: i32) -> Self
    {
        ExtraType::Int32(value)
    }
}

impl From<i64> for ExtraType
{
    fn from(value: i64) -> Self
    {
        ExtraType::Int64(value)
    }
}

impl From<u32> for ExtraType
{
    fn from(value: u32) -> Self
    {
        ExtraType::UInt32(value)
    }
}

impl From<u64> for ExtraType
{
    fn from(value: u64) -> Self
    {
        ExtraType::UInt64(value)
    }
}

impl From<usize> for ExtraType
{
    fn from(value: usize) -> Self
    {
        ExtraType::USize(value)
    }
}

impl From<f32> for ExtraType
{
    fn from(value: f32) -> Self
    {
        ExtraType::Float32(value)
    }
}

impl From<f64> for ExtraType
{
    fn from(value: f64) -> Self
    {
        ExtraType::Float64(value)
    }
}

impl From<Vector2<f32>> for ExtraType
{
    fn from(value: Vector2<f32>) -> Self
    {
        ExtraType::Vec2(value)
    }
}

impl From<Vector3<f32>> for ExtraType
{
    fn from(value: Vector3<f32>) -> Self
    {
        ExtraType::Vec3(value)
    }
}

impl From<Vector4<f32>> for ExtraType
{
    fn from(value: Vector4<f32>) -> Self
    {
        ExtraType::Vec4(value)
    }
}

impl Extras
{
    pub fn new() -> Extras
    {
        Extras
        {
            extras: HashMap::new()
        }
    }

    pub fn contains(&self, key: &str) -> bool
    {
        let key = key.to_string();
        self.extras.contains_key(&key)
    }

    pub fn get<'a, T>(&'a self, key: &str) -> Option<&'a T>
    where
            T: 'static,
    {
        if let Some(extra) = self.extras.get(key)
        {
            match extra
            {
                ExtraType::Bool(value) => value as &dyn std::any::Any,
                ExtraType::String(value) => value as &dyn std::any::Any,
                ExtraType::Int32(value) => value as &dyn std::any::Any,
                ExtraType::Int64(value) => value as &dyn std::any::Any,
                ExtraType::UInt32(value) => value as &dyn std::any::Any,
                ExtraType::UInt64(value) => value as &dyn std::any::Any,
                ExtraType::USize(value) => value as &dyn std::any::Any,
                ExtraType::Float32(value) => value as &dyn std::any::Any,
                ExtraType::Float64(value) => value as &dyn std::any::Any,
                ExtraType::Vec2(value) => value as &dyn std::any::Any,
                ExtraType::Vec3(value) => value as &dyn std::any::Any,
                ExtraType::Vec4(value) => value as &dyn std::any::Any,
            }.downcast_ref::<T>()
        }
        else
        {
            None
        }
    }

    pub fn get_mut<'a, T>(&'a mut self, key: &str) -> Option<&'a mut T>
    where
        T: 'static,
    {
        if let Some(extra) = self.extras.get_mut(key)
        {
            if let Some(value) = match extra
            {
                ExtraType::Bool(value) => value as &mut dyn std::any::Any,
                ExtraType::String(value) => value as &mut dyn std::any::Any,
                ExtraType::Int32(value) => value as &mut dyn std::any::Any,
                ExtraType::Int64(value) => value as &mut dyn std::any::Any,
                ExtraType::UInt32(value) => value as &mut dyn std::any::Any,
                ExtraType::UInt64(value) => value as &mut dyn std::any::Any,
                ExtraType::USize(value) => value as &mut dyn std::any::Any,
                ExtraType::Float32(value) => value as &mut dyn std::any::Any,
                ExtraType::Float64(value) => value as &mut dyn std::any::Any,
                ExtraType::Vec2(value) => value as &mut dyn std::any::Any,
                ExtraType::Vec3(value) => value as &mut dyn std::any::Any,
                ExtraType::Vec4(value) => value as &mut dyn std::any::Any,
            }.downcast_mut::<T>()
            {
                Some(value)
            }
            else
            {
                None
            }
        }
        else
        {
            None
        }
    }

    pub fn insert<T>(&mut self, key: &str, value: T) where T: Into<ExtraType>,
    {
        let key = key.to_string();
        let extra_type = value.into();
        self.extras.insert(key, extra_type);
    }

    pub fn remove(&mut self, key: &str)
    {
        let key = key.to_string();
        self.extras.remove(&key);
    }

    pub fn iter(&self) -> Iter<'_, String, ExtraType>
    {
        self.extras.iter()
    }

    // a json value of a project file: bool, number, string or an array of 2 to 4 numbers - false if not supported
    pub fn insert_json(&mut self, key: &str, value: &serde_json::Value) -> bool
    {
        let numbers: Option<Vec<f32>> = value.as_array().map(|a| a.iter().filter_map(|v| v.as_f64()).map(|v| v as f32).collect());
        let extra = match value
        {
            serde_json::Value::Bool(v) => ExtraType::Bool(*v),
            serde_json::Value::String(v) => ExtraType::String(v.clone()),
            serde_json::Value::Number(n) if n.is_i64() => ExtraType::Int64(n.as_i64().unwrap()),
            serde_json::Value::Number(n) if n.is_u64() => ExtraType::UInt64(n.as_u64().unwrap()),
            serde_json::Value::Number(n) => ExtraType::Float64(n.as_f64().unwrap_or_default()),
            serde_json::Value::Array(a) => match numbers.as_deref()
            {
                Some([x, y]) if a.len() == 2 => ExtraType::Vec2(Vector2::new(*x, *y)),
                Some([x, y, z]) if a.len() == 3 => ExtraType::Vec3(Vector3::new(*x, *y, *z)),
                Some([x, y, z, w]) if a.len() == 4 => ExtraType::Vec4(Vector4::new(*x, *y, *z, *w)),
                _ => return false,
            },
            _ => return false,
        };

        self.extras.insert(key.to_string(), extra);
        true
    }

    // the value as json for a project file, the counterpart of insert_json
    pub fn get_json(&self, key: &str) -> Option<serde_json::Value>
    {
        Some(match self.extras.get(key)?
        {
            ExtraType::Bool(v) => serde_json::json!(v),
            ExtraType::String(v) => serde_json::json!(v),
            ExtraType::Int32(v) => serde_json::json!(v),
            ExtraType::Int64(v) => serde_json::json!(v),
            ExtraType::UInt32(v) => serde_json::json!(v),
            ExtraType::UInt64(v) => serde_json::json!(v),
            ExtraType::USize(v) => serde_json::json!(v),
            ExtraType::Float32(v) => serde_json::json!(v),
            ExtraType::Float64(v) => serde_json::json!(v),
            ExtraType::Vec2(v) => serde_json::json!([v.x, v.y]),
            ExtraType::Vec3(v) => serde_json::json!([v.x, v.y, v.z]),
            ExtraType::Vec4(v) => serde_json::json!([v.x, v.y, v.z, v.w]),
        })
    }
}