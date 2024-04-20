#![allow(dead_code)]

use std::collections::{hash_map::Iter, HashMap};

#[derive(Debug)]
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
    Float64(f64)
}

pub struct NodeExtras
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

impl NodeExtras
{
    pub fn new() -> NodeExtras
    {
        NodeExtras
        {
            extras: HashMap::new()
        }
    }

    pub fn contains(&self, key: String) -> bool
    {
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

    pub fn insert<T>(&mut self, key: String, value: T) where T: Into<ExtraType>,
    {
        let extra_type = value.into();
        self.extras.insert(key, extra_type);
    }

    pub fn iter(&self) -> Iter<'_, String, ExtraType>
    {
        self.extras.iter()
    }
}