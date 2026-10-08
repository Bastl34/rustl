#![allow(dead_code)]

use std::collections::HashMap;

use nalgebra::{Vector2, Vector3, Vector4};
use serde::{Deserialize, Serialize};

use super::origin::Origin;

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

// for the editor: plain values, lists (newline separated strings) on one line
impl std::fmt::Display for ExtraType
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result
    {
        match self
        {
            ExtraType::Bool(v) => write!(f, "{v}"),
            ExtraType::String(v) => write!(f, "\"{}\"", v.lines().collect::<Vec<_>>().join(", ")),
            ExtraType::Int32(v) => write!(f, "{v}"),
            ExtraType::Int64(v) => write!(f, "{v}"),
            ExtraType::UInt32(v) => write!(f, "{v}"),
            ExtraType::UInt64(v) => write!(f, "{v}"),
            ExtraType::USize(v) => write!(f, "{v}"),
            ExtraType::Float32(v) => write!(f, "{v}"),
            ExtraType::Float64(v) => write!(f, "{v}"),
            ExtraType::Vec2(v) => write!(f, "({}, {})", v.x, v.y),
            ExtraType::Vec3(v) => write!(f, "({}, {}, {})", v.x, v.y, v.z),
            ExtraType::Vec4(v) => write!(f, "({}, {}, {}, {})", v.x, v.y, v.z, v.w),
        }
    }
}

// value + origin per key
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Extras
{
    extras: HashMap<String, (ExtraType, Origin)>,
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
        Extras { extras: HashMap::new() }
    }

    pub fn contains(&self, key: &str) -> bool
    {
        self.extras.contains_key(key)
    }

    pub fn get<'a, T>(&'a self, key: &str) -> Option<&'a T>
    where
        T: 'static,
    {
        let (extra, _) = self.extras.get(key)?;
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

    pub fn get_mut<'a, T>(&'a mut self, key: &str) -> Option<&'a mut T>
    where
        T: 'static,
    {
        let (extra, _) = self.extras.get_mut(key)?;
        match extra
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
    }

    // a new key is runtime, an existing one keeps its origin - so code can change a scene value and it is saved
    pub fn insert<T>(&mut self, key: &str, value: T) where T: Into<ExtraType>,
    {
        let origin = self.origin(key).unwrap_or_default();
        self.extras.insert(key.to_string(), (value.into(), origin));
    }

    pub fn insert_with_origin<T>(&mut self, key: &str, value: T, origin: Origin) where T: Into<ExtraType>,
    {
        self.extras.insert(key.to_string(), (value.into(), origin));
    }

    pub fn origin(&self, key: &str) -> Option<Origin>
    {
        self.extras.get(key).map(|(_, origin)| *origin)
    }

    pub fn remove(&mut self, key: &str)
    {
        self.extras.remove(key);
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &ExtraType)>
    {
        self.extras.iter().map(|(key, (value, _))| (key, value))
    }

    pub fn iter_with_origin(&self) -> impl Iterator<Item = (&String, &ExtraType, Origin)>
    {
        self.extras.iter().map(|(key, (value, origin))| (key, value, *origin))
    }

    // a json value of a project file: bool, number, string or an array of 2 to 4 numbers - false if not supported
    pub fn insert_json(&mut self, key: &str, value: &serde_json::Value, origin: Origin) -> bool
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

        self.extras.insert(key.to_string(), (extra, origin));
        true
    }

    // the value as json for a project file, the counterpart of insert_json
    pub fn get_json(&self, key: &str) -> Option<serde_json::Value>
    {
        let (extra, _) = self.extras.get(key)?;
        Some(match extra
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
