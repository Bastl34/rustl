#![allow(dead_code)]

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
pub struct AssetPathDesciptor
{
    pub origin_path: String,
    pub inner_path: String,
    pub variation: String,
}

impl AssetPathDesciptor
{
    pub fn new(origin_path: String, inner_path: String, variation: String) -> Self
    {
        AssetPathDesciptor
        {
            origin_path,
            inner_path,
            variation,
        }
    }

    pub fn new_from_path(origin_path: String) -> Self
    {
        AssetPathDesciptor
        {
            origin_path,
            inner_path: "".to_string(),
            variation: "".to_string(),
        }
    }

    pub fn get_full_descriptor(&self) -> String
    {
        let mut str = self.origin_path.to_string();

        if !self.inner_path.is_empty()
        {
            str.push_str(&format!(" # {}", self.inner_path));
        }

        if !self.variation.is_empty()
        {
            str.push_str(&format!(" # {}", self.variation));
        }

        str
    }
}
