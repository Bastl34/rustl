use serde::Serialize;
use serde_json::{to_value, Map, Serializer, Value};

use crate::state::scene::scene::Scene;
use crate::state::state::{State, ENGINE_INTERNAL_TAG_PREFX};
use crate::helper::file::write_string_to_tile;

fn insert_kv(obj: &mut Value, key: &str, val: Value)
{
    if let Value::Object(map) = obj
    {
        map.insert(key.to_string(), val);
    }
}

fn get_object_mut(value: &mut Value) -> Option<&mut Map<String, Value>>
{
    match value
    {
        Value::Object(map) => Some(map),
        _ => None,
    }
}

fn to_pretty_json_with_indent(value: &Value, indent: &[u8]) -> String
{
    let mut buf = Vec::new();
    let formatter = serde_json::ser::PrettyFormatter::with_indent(indent);
    let mut serializer = Serializer::with_formatter(&mut buf, formatter);
    value.serialize(&mut serializer).unwrap();
    String::from_utf8(buf).unwrap()
}

pub fn export(state: &State, path: &str) -> bool
{
    println!("exporting {} to json", path);

    let mut export = Value::Object(Map::new());

    if let Value::Object(ref mut export) = export
    {
        // metadata
        export.insert("project".to_string(), to_value(&state.project).unwrap());
        export.insert("rendering_settings".to_string(), to_value(&state.rendering).unwrap());
        export.insert("exporter_version".to_string(), Value::String("1.0.0".to_string()));

        //export.insert("scenes".to_string(), Value::Array(scenes));
        export.insert
        (
            "scenes".to_string(),
            Value::Array
            (
                state.scenes
                    .iter()
                    .map(|scene| serde_json::to_value(scene).unwrap())
                    .collect(),
            ),
        );
    }

    clean_internal_fields(&mut export);

    let output = to_pretty_json_with_indent(&export, b"    ");
    if write_string_to_tile(format!("{}.json", path).as_str(), output).is_ok()
    {
        return true;
    }

    false
}

fn clean_internal_fields(value: &mut Value)
{
    match value
    {
        Value::Object(map) =>
        {
            // remove internal stuff from "extras"
            if let Some(Value::Object(extras)) = map.get_mut("extras")
            {
                extras.retain(|k, _| !k.starts_with(ENGINE_INTERNAL_TAG_PREFX));
            }

            // remove internal nodes and entries
            if let Some(Value::Object(tags)) = map.get("tags")
            {
                let has_internal_tag = tags.keys().any(|k| k.starts_with(ENGINE_INTERNAL_TAG_PREFX));
                if has_internal_tag
                {
                    *value = Value::Null;
                    return;
                }
            }

            for (_k, v) in map.iter_mut()
            {
                clean_internal_fields(v);
            }
        }

        Value::Array(arr) =>
        {
            for item in arr.iter_mut()
            {
                clean_internal_fields(item);
            }

            // remove all null values
            arr.retain(|v| !v.is_null());
        }

        _ => {}
    }
}
