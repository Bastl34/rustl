use std::sync::{LazyLock, Mutex};

static SCENE_ID:        LazyLock<Mutex<u32>> = LazyLock::new(|| Mutex::new(0));
static TEXTURE_ID:      LazyLock<Mutex<u32>> = LazyLock::new(|| Mutex::new(0));
static MESH_ID:         LazyLock<Mutex<u32>> = LazyLock::new(|| Mutex::new(0));
static SOUND_SOURCE_ID: LazyLock<Mutex<u32>> = LazyLock::new(|| Mutex::new(0));
static NODE_ID:         LazyLock<Mutex<u32>> = LazyLock::new(|| Mutex::new(0));
static INSTANCE_ID:     LazyLock<Mutex<u32>> = LazyLock::new(|| Mutex::new(0));
static CAMERA_ID:       LazyLock<Mutex<u32>> = LazyLock::new(|| Mutex::new(0));
static LIGHT_ID:        LazyLock<Mutex<u32>> = LazyLock::new(|| Mutex::new(0));
static COMPONENT_ID:    LazyLock<Mutex<u32>> = LazyLock::new(|| Mutex::new(0));

fn get_next_from(counter: &LazyLock<Mutex<u32>>) -> u32
{
    let mut id = counter.lock().unwrap();
    let current_id = *id;
    *id += 1;

    current_id
}

pub fn get_next_scene_id() -> u32        { get_next_from(&SCENE_ID) }
pub fn get_next_texture_id() -> u32      { get_next_from(&TEXTURE_ID) }
pub fn get_next_mesh_id() -> u32         { get_next_from(&MESH_ID) }
pub fn get_next_sound_source_id() -> u32 { get_next_from(&SOUND_SOURCE_ID) }
pub fn get_next_node_id() -> u32         { get_next_from(&NODE_ID) }
pub fn get_next_instance_id() -> u32     { get_next_from(&INSTANCE_ID) }
pub fn get_next_camera_id() -> u32       { get_next_from(&CAMERA_ID) }
pub fn get_next_light_id() -> u32        { get_next_from(&LIGHT_ID) }
pub fn get_next_component_id() -> u32    { get_next_from(&COMPONENT_ID) }