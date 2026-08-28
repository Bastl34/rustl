#![allow(dead_code)]

use std::{cell::RefCell, collections::{HashMap, VecDeque}, fmt, rc::Rc, sync::{Arc, RwLock}};
use web_time::Instant;

use nalgebra::Vector3;
use serde::{de::{MapAccess, Visitor}, ser::SerializeMap, Deserialize, Deserializer, Serialize, Serializer};

use crate::{component_downcast_mut, helper::{self, change_tracker::ChangeTracker, concurrency::{execution_queue::{ExecutionQueue, ExecutionQueueItem}, thread::spawn_thread}}, impl_arc_rwbox_map_serializer, input::input_manager::InputManager, output::audio_device::AudioDeviceItem, resources::resources::{load_binary, load_binary_async}, state::{resources::{mesh_resource::{MeshResource, MeshResourceItem}, sound_source::{SoundSource, SoundSourceItem}, texture::{Texture, TextureItem}}, scene::{components::{material::Material, mesh::Mesh, sound::Sound}, scene::Scene}}};

use super::scene::{camera_controller::camera_controller::CameraControllerBox, components::{component::{Component, ComponentItem}, material::TextureType}, loader::loader::load_texture, scene::SceneItem, scene_controller::scene_controller::SceneControllerBox};

pub type StateItem = Rc<RefCell<State>>;

pub const FPS_CHART_VALUES: usize = 100;
pub const DEFAULT_MAX_TEXTURE_RESOLUTION: u32 = 16384;
pub const DEFAULT_MAX_SUPPORTED_TEXTURE_RESOLUTION: u32 = 4096;

pub const DEFAULT_SHADOW_MAP_SIZE: u32 = 2048;
pub const DEFAULT_SHADOW_MAX_DISTANCE: f32 = 100.0;

pub const DEFAULT_SSAO_RADIUS: f32 = 0.5;
pub const DEFAULT_SSAO_BIAS: f32 = 0.025;
pub const DEFAULT_SSAO_STRENGTH: f32 = 1.0;

pub const DEFAULT_XRAY_ALPHA: f32 = 0.5;

pub const DEFAULT_FOG_COLOR: Vector3<f32> = Vector3::new(0.6, 0.7, 0.8);
pub const DEFAULT_FOG_DENSITY: f32 = 0.02;

pub const REFERENCE_UPDATE_FRAMES: f32 = 60.0;

pub const ENGINE_INTERNAL_TAG_PREFX: &str = "__internal_";
pub const ENGINE_INTERNAL_TAG: &str = "__internal_engine";

pub fn get_delta_t(frame_scale: f32) -> f32
{
    frame_scale / REFERENCE_UPDATE_FRAMES
}

#[derive(Serialize, Deserialize)]
pub struct Project
{
    pub name: String,
}

pub struct RenderingAdapterFeatures
{
    pub name: String,
    pub driver: String,
    pub driver_info: String,
    pub backend: String,

    pub storage_buffer_array_support: bool,
    pub wireframe_mode_support: bool,
    pub occlusion_culling_support: bool, // compute shaders + indirect draws (not available on WebGL)
    pub ssao_support: bool, // textureLoad on depth textures is not supported by naga's GLSL backend (WebGL/GL)
    pub max_msaa_samples: u32,
    pub max_texture_resolution: u32,
    pub max_supported_texture_resolution: u32
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresentModeSetting
{
    VSync,     // Fifo: vblank-paced, tear-free
    FastVSync, // Mailbox: tear-free, no CPU stall on acquire
    VSyncOff,  // Immediate: may tear, uncapped
}

#[derive(Serialize, Deserialize)]
pub struct Rendering
{
    pub clear_color: ChangeTracker<Vector3<f32>>,
    pub present_mode: ChangeTracker<PresentModeSetting>,

    pub fullscreen: ChangeTracker<bool>,
    pub msaa: ChangeTracker<u32>,

    pub shadow: ChangeTracker<bool>,
    pub shadow_map_resolution: ChangeTracker<u32>,
    pub shadow_max_distance: f32,

    pub ssao: bool,
    pub ssao_half_res: bool,
    pub ssao_radius: f32,
    pub ssao_bias: f32,
    pub ssao_strength: f32,

    // distance based fog (world space)
    #[serde(default)]
    pub fog: bool,
    #[serde(default = "default_fog_color")]
    pub fog_color: Vector3<f32>,
    #[serde(default = "default_fog_density")]
    pub fog_density: f32,

    pub distance_sorting: bool,
    pub frustum_culling: bool,
    pub occlusion_culling: bool,
    pub create_mipmaps: bool,
    pub max_texture_resolution: Option<u32>,

    pub wireframe_mode: bool,

    // reverse z depth buffer (near = 1, far = 0): near-uniform depth precision, less z-fighting
    #[serde(default)]
    pub reverse_z: bool,

    // debug rendering of the culling bounding volumes (lines)
    #[serde(default)]
    pub draw_bounding_boxes: bool,
    #[serde(default)]
    pub draw_bounding_spheres: bool,

    pub xray_mode: bool,
    pub xray_alpha: f32,
}

fn default_fog_color() -> Vector3<f32> { DEFAULT_FOG_COLOR }
fn default_fog_density() -> f32 { DEFAULT_FOG_DENSITY }

pub struct SupportedFileTypes
{
    pub objects: Vec<String>,
    pub scenes: Vec<String>,
    pub textures: Vec<String>,
    pub materials: Vec<String>,
}

impl Default for SupportedFileTypes
{
    fn default() -> Self
    {
        Self
        {
             objects: vec![String::from("obj"), String::from("gltf"), String::from("glb")],
             scenes: vec![String::from("scene")],
             textures: vec![String::from("jpg"), String::from("jpeg"), String::from("png"), String::from("webp")],
             materials: vec![String::from("mat")],
        }
    }
}

pub struct Statistics
{
    pub draw_calls: u32,
    pub occlusion_culled_objects: u32, // objects culled by the gpu occlusion check (async readback - a few frames behind)
    pub frustum_culled_objects: u32,   // objects dropped by the cpu frustum culling
    pub fps_timer: Instant,
    pub last_time: u128,
    pub fps: u32,
    pub last_fps: u32,
    pub last_fps_1_percent_low: u32, //1% low
    pub fps_cpu_absolute: u32,
    pub fps_gpu_absolute: Option<u32>, // None if the adapter does not support timestamp queries
    pub fps_average_chart: VecDeque<u32>,
    pub fps_1_percent_low_chart: VecDeque<u32>,

    pub frame_update_time: u128, // micros
    pub frame_scale: f32,

    pub frame_time: f32, // in ms
    pub frame_times: VecDeque<f32>, // micros (only last second)

    pub engine_update_time: f32,
    pub engine_render_time: f32,

    pub app_update_time: f32,

    pub editor_update_time: f32,

    pub egui_update_time: f32,
    pub egui_render_time: f32,

    pub shadow_views: u32,
    pub shadow_draw_calls: u32,

    pub gpu_shadow_time: Option<f32>,
    pub gpu_depth_time: Option<f32>,
    pub gpu_ssao_time: Option<f32>,
    pub gpu_color_time: Option<f32>,
    pub gpu_hzb_time: Option<f32>,
    pub gpu_egui_time: Option<f32>,

    pub frame: u64,
}

pub struct InputOutput
{
    pub input_manager: InputManager,
    pub audio_device: AudioDeviceItem,
}

#[derive(Default)]
pub struct Resources
{
    pub textures: HashMap<String, TextureItem>,
    pub sound_sources: HashMap<String, SoundSourceItem>,
    pub mesh_resources: HashMap<String, MeshResourceItem>,
}

impl_arc_rwbox_map_serializer!(TexturesSerializer, String, Texture);
impl_arc_rwbox_map_serializer!(SoundSourcesSerializer, String, SoundSource);
impl_arc_rwbox_map_serializer!(MeshResourcesSerializer, String, MeshResource);

impl Serialize for Resources
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where S: Serializer
    {
        let mut map = serializer.serialize_map(None)?;

        map.serialize_entry("textures", &TexturesSerializer { map: &self.textures },)?;
        map.serialize_entry("sound_sources", &SoundSourcesSerializer { map: &self.sound_sources },)?;
        map.serialize_entry("mesh_resources", &MeshResourcesSerializer { map: &self.mesh_resources },)?;

        map.end()
    }
}

impl<'de> Deserialize<'de> for Resources
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where D: Deserializer<'de>
    {
        struct ResourcesVisitor;

        impl<'de> Visitor<'de> for ResourcesVisitor
        {
            type Value = Resources;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result
            {
                formatter.write_str("struct Resources")
            }

            fn visit_map<V>(self, mut map: V) -> Result<Resources, V::Error>
            where V: MapAccess<'de>
            {
                let mut resources: Resources = Resources::default();

                while let Some(key) = map.next_key::<String>()?
                {
                    match key.as_str()
                    {
                       "textures" =>
                       {
                            let temp_map: HashMap<String, Texture> = map.next_value()?;
                            resources.textures = temp_map.into_iter().map(|(k, v)| (k, Arc::new(RwLock::new(Box::new(v))))).collect();
                        }
                        "sound_sources" =>
                        {
                            let temp_map: HashMap<String, SoundSource> = map.next_value()?;
                            resources.sound_sources = temp_map.into_iter().map(|(k, v)| (k, Arc::new(RwLock::new(Box::new(v))))).collect();
                        }
                        "mesh_resources" =>
                        {
                            let temp_map: HashMap<String, MeshResource> = map.next_value()?;
                            resources.mesh_resources = temp_map.into_iter().map(|(k, v)| (k, Arc::new(RwLock::new(Box::new(v))))).collect();
                        }
                        _ =>
                        {
                            // ignore
                            let _: serde::de::IgnoredAny = map.next_value()?;
                        }
                    }
                }

                Ok(resources)
            }
        }

        deserializer.deserialize_map(ResourcesVisitor)
    }
}

#[derive(Serialize, Deserialize)]
pub struct Debug
{
    pub save_image: bool,
    pub save_depth_pass_image: bool,
    pub save_depth_buffer_image: bool,
    pub save_hzb_image: bool,

    pub save_screenshot: bool,

    pub show_depth_pass_image: Option<u32>,
    pub show_depth_buffer_image: Option<u32>,
    pub show_hzb_image: Option<u32>,

    pub highlight_visible_occlusions: bool,
}

pub struct State
{
    pub project: Project,

    pub rendering_adapter: RenderingAdapterFeatures,
    pub rendering: Rendering,

    pub io: InputOutput,

    pub resources: Resources,

    pub main_thread_execution_queue: ExecutionQueueItem,

    pub running: bool,
    pub pause: bool,
    pub exit: bool,

    pub scenes: Vec<SceneItem>,

    pub oneshot_sounds: Vec<Sound>,

    pub registered_components: Vec<(String, bool, fn(&str) -> ComponentItem)>,
    pub registered_camera_controller: Vec<(String, fn() -> CameraControllerBox)>,
    pub registered_scene_controller: Vec<(String, fn() -> SceneControllerBox)>,

    pub supported_file_types: SupportedFileTypes,

    pub in_focus: bool,

    pub width: u32,
    pub height: u32,
    pub scale_factor: f32,

    pub debug: Debug,

    pub stats: Statistics,
}

impl State
{
    pub fn new(audio_device: AudioDeviceItem) -> State
    {
        let mut components: Vec<(String, bool, fn(&str) -> ComponentItem)> = vec![];

        components.push(("Alpha".to_string(), crate::state::scene::components::alpha::Alpha::instantiable(), |name| { Arc::new(RwLock::new(Box::new(crate::state::scene::components::alpha::Alpha::new(name, 1.0)))) }));
        components.push(("Material".to_string(), crate::state::scene::components::material::Material::instantiable(), |name| { Arc::new(RwLock::new(Box::new(crate::state::scene::components::material::Material::new(name)))) }));
        //components.push(("Mesh".to_string(), crate::state::scene::components::mesh::Mesh::instantiable(), |name| { Arc::new(RwLock::new(Box::new(crate::state::scene::components::mesh::Mesh::new_plane( name, x0, x1, x2, x3)))) }));
        components.push(("Transform".to_string(), crate::state::scene::components::transformation::Transformation::instantiable(), |name| { Arc::new(RwLock::new(Box::new(crate::state::scene::components::transformation::Transformation::identity(name)))) }));
        components.push(("Transform Animation".to_string(), crate::state::scene::components::transformation_animation::TransformationAnimation::instantiable(), |name| { Arc::new(RwLock::new(Box::new(crate::state::scene::components::transformation_animation::TransformationAnimation::new_empty(name)))) }));
        components.push(("Morph Target Animation".to_string(), crate::state::scene::components::morph_target_animation::MorphTargetAnimation::instantiable(), |name| { Arc::new(RwLock::new(Box::new(crate::state::scene::components::morph_target_animation::MorphTargetAnimation::new_empty(name)))) }));
        components.push(("Animation Blending".to_string(), crate::state::scene::components::animation_blending::AnimationBlending::instantiable(), |name| { Arc::new(RwLock::new(Box::new(crate::state::scene::components::animation_blending::AnimationBlending::new_empty(name)))) }));
        components.push(("Sound".to_string(), crate::state::scene::components::sound::Sound::instantiable(), |name| { Arc::new(RwLock::new(Box::new(crate::state::scene::components::sound::Sound::new_empty(name)))) }));
        components.push(("Delay".to_string(), crate::state::scene::components::delay::Delay::instantiable(), |name| { Arc::new(RwLock::new(Box::new(crate::state::scene::components::delay::Delay::new_empty(name)))) }));
        components.push(("Look At".to_string(), crate::state::scene::components::delay::Delay::instantiable(), |name| { Arc::new(RwLock::new(Box::new(crate::state::scene::components::look_at::LookAt::new_empty(name)))) }));

        let mut cam_controller: Vec<(String, fn() -> CameraControllerBox)> = vec![];
        cam_controller.push(("Fly Controller".to_string(), || { Box::new(crate::state::scene::camera_controller::fly_controller::FlyController::default()) }));
        cam_controller.push(("Pan Controller".to_string(), || { Box::new(crate::state::scene::camera_controller::pan_controller::PanController::default()) }));
        cam_controller.push(("Target Rotation Controller".to_string(), || { Box::new(crate::state::scene::camera_controller::target_rotation_controller::TargetRotationController::default()) }));
        cam_controller.push(("Path Controller".to_string(), || { Box::new(crate::state::scene::camera_controller::path_controller::PathController::default()) }));

        let mut scene_controller: Vec<(String, fn() -> SceneControllerBox)> = vec![];
        scene_controller.push(("Character Controller".to_string(), || { Box::new(crate::state::scene::scene_controller::char_controller::CharacterController::default()) }));

        Self
        {
            project: Project
            {
                name: "Uknown".to_string(),
            },
            rendering_adapter: RenderingAdapterFeatures
            {
                name: String::new(),
                driver: String::new(),
                driver_info: String::new(),
                backend: String::new(),
                storage_buffer_array_support: false,
                wireframe_mode_support: false,
                occlusion_culling_support: false,
                ssao_support: false,
                max_msaa_samples: 1,
                max_texture_resolution: DEFAULT_MAX_TEXTURE_RESOLUTION,
                max_supported_texture_resolution: DEFAULT_MAX_SUPPORTED_TEXTURE_RESOLUTION
            },

            rendering: Rendering
            {
                clear_color: ChangeTracker::new(Vector3::<f32>::new(0.0, 0.0, 0.0)),
                present_mode: ChangeTracker::new(PresentModeSetting::VSync),

                fullscreen: ChangeTracker::new(false),
                msaa: ChangeTracker::new(8),
                shadow: ChangeTracker::new(true),
                shadow_map_resolution: ChangeTracker::new(DEFAULT_SHADOW_MAP_SIZE),
                shadow_max_distance: DEFAULT_SHADOW_MAX_DISTANCE,

                ssao: true,
                ssao_half_res: false,
                ssao_radius: DEFAULT_SSAO_RADIUS,
                ssao_bias: DEFAULT_SSAO_BIAS,
                ssao_strength: DEFAULT_SSAO_STRENGTH,

                fog: false,
                fog_color: DEFAULT_FOG_COLOR,
                fog_density: DEFAULT_FOG_DENSITY,

                distance_sorting: true,
                frustum_culling: true,
                occlusion_culling: true,
                create_mipmaps: true,
                max_texture_resolution: None,

                wireframe_mode: false,
                reverse_z: false,

                draw_bounding_boxes: false,
                draw_bounding_spheres: false,

                xray_mode: false,
                xray_alpha: DEFAULT_XRAY_ALPHA,
            },

            io: InputOutput
            {
                input_manager: InputManager::new(),
                audio_device,
            },

            resources: Resources
            {
                textures: HashMap::new(),
                sound_sources: HashMap::new(),
                mesh_resources: HashMap::new(),
            },

            main_thread_execution_queue: Arc::new(RwLock::new(ExecutionQueue::new())),

            running: false,
            pause: false,
            exit: false,

            scenes: vec![],

            oneshot_sounds: vec![],

            registered_components: components,
            registered_camera_controller: cam_controller,
            registered_scene_controller: scene_controller,

            supported_file_types: SupportedFileTypes::default(),

            in_focus: true,

            width: 0,
            height: 0,
            scale_factor: 1.0,

            debug: Debug
            {
                save_image: false,
                save_depth_pass_image: false,
                save_depth_buffer_image: false,
                save_hzb_image: false,

                save_screenshot: false,

                show_depth_pass_image: None,
                show_depth_buffer_image: None,
                show_hzb_image: None,

                highlight_visible_occlusions: false,
            },

            stats: Statistics
            {
                draw_calls: 0,
                occlusion_culled_objects: 0,
                frustum_culled_objects: 0,
                fps_timer: Instant::now(),
                last_time: 0,
                fps: 0,
                last_fps: 0,
                last_fps_1_percent_low: 0,
                fps_cpu_absolute: 0,
                fps_gpu_absolute: None,
                fps_average_chart: VecDeque::from(vec![0; 100]),
                fps_1_percent_low_chart: VecDeque::from(vec![0; 100]),
                frame_times: VecDeque::from(vec![]),

                frame_update_time: 0,
                frame_scale: 0.0,

                frame_time: 0.0,

                engine_update_time: 0.0,
                engine_render_time: 0.0,

                app_update_time: 0.0,

                editor_update_time: 0.0,

                egui_update_time: 0.0,
                egui_render_time: 0.0,

                shadow_views: 0,
                shadow_draw_calls: 0,

                gpu_shadow_time: None,
                gpu_depth_time: None,
                gpu_ssao_time: None,
                gpu_color_time: None,
                gpu_hzb_time: None,
                gpu_egui_time: None,

                frame: 0,
            },
        }
    }

    pub async fn load_texture_or_reuse_async(&mut self, path: &str, extension: Option<String>, max_tex_res: u32) -> anyhow::Result<TextureItem>
    {
        let image_bytes = load_binary_async(path).await?;

        Ok(self.load_texture_byte_or_reuse(&image_bytes, path, extension, max_tex_res))
    }

    pub fn load_texture_or_reuse(&mut self, path: &str, extension: Option<String>, max_tex_res: u32) -> anyhow::Result<TextureItem>
    {
        let image_bytes = load_binary(path)?;

        Ok(self.load_texture_byte_or_reuse(&image_bytes, path, extension, max_tex_res))
    }

    pub fn load_texture_byte_or_reuse(&mut self, image_bytes: &Vec<u8>, name: &str, extension: Option<String>, max_tex_res: u32) -> TextureItem
    {
        let hash = helper::crypto::get_hash_from_byte_vec(&image_bytes);

        if self.resources.textures.contains_key(&hash)
        {
            println!("reusing texture {}", name);
            return self.resources.textures.get_mut(&hash).unwrap().clone();
        }

        let texture = Texture::new(name, &image_bytes, extension, max_tex_res);

        let arc = Arc::new(RwLock::new(Box::new(texture)));

        self.resources.textures.insert(hash, arc.clone());

        arc
    }

    pub fn load_sound_source_byte_or_reuse(&mut self, sound_bytes: &Vec<u8>, name: &str, extension: Option<String>) -> SoundSourceItem
    {
        let hash = helper::crypto::get_hash_from_byte_vec(&sound_bytes);

        if self.resources.sound_sources.contains_key(&hash)
        {
            println!("reusing sound source {}", name);
            return self.resources.sound_sources.get_mut(&hash).unwrap().clone();
        }

        let sound_source = SoundSource::new(name, self.io.audio_device.clone(), &sound_bytes, extension);

        let arc = Arc::new(RwLock::new(Box::new(sound_source)));

        self.resources.sound_sources.insert(hash, arc.clone());

        arc
    }

    pub fn insert_texture_or_reuse(&mut self, texture: TextureItem, name: &str) -> TextureItem
    {
        let hash = texture.read().unwrap().hash.clone();

        if self.resources.textures.contains_key(&hash)
        {
            println!("reusing texture {}", name);
            return self.resources.textures.get_mut(&hash).unwrap().clone();
        }


        self.resources.textures.insert(hash, texture.clone());

        texture
    }

    pub fn insert_mesh_resource_or_reuse(&mut self, mesh_resource: MeshResourceItem, name: &str) -> MeshResourceItem
    {
        let hash = mesh_resource.read().unwrap().hash.clone();

        if self.resources.mesh_resources.contains_key(&hash)
        {
            println!("reusing mesh resources {}", name);
            return self.resources.mesh_resources.get_mut(&hash).unwrap().clone();
        }

        self.resources.mesh_resources.insert(hash, mesh_resource.clone());

        mesh_resource
    }

    pub fn get_texture_by_id(&self, id: u32) -> Option<TextureItem>
    {
        for texture_arc in self.resources.textures.values()
        {
            let texture =  texture_arc.read().unwrap();
            if texture.id == id
            {
                return Some(texture_arc.clone());
            }
        }

        None
    }

    pub fn delete_texture_by_id(&mut self, id: u32) -> bool
    {
        for scene in &mut self.scenes
        {
            // remove texture from all materials
            for material in &mut scene.materials
            {
                let material = material.1;
                component_downcast_mut!(material, Material);
                material.remove_texture_by_id(id);
            }

            // remove texture from environment map
            if let Some(env_tex) = scene.get_data().environment_texture.as_ref()
            {
                if let Some(item) = env_tex.get()
                {
                    let same_id =
                    {
                        let texture_guard = item.read().unwrap();
                        texture_guard.id == id
                    };

                    if same_id
                    {
                        scene.get_data_mut().get_mut().environment_texture = None;
                    }
                }
            }
        }

        let len = self.resources.textures.len();
        self.resources.textures.retain(|_key, texture|
        {
            let texture = texture.read().unwrap();
            texture.id != id
        });

        self.resources.textures.len() != len
    }

    pub fn get_sound_source_by_id(&self, id: u32) -> Option<SoundSourceItem>
    {
        for sound_arc in self.resources.sound_sources.values()
        {
            let sound =  sound_arc.read().unwrap();
            if sound.id == id
            {
                return Some(sound_arc.clone());
            }
        }

        None
    }

    pub fn delete_sound_source_by_id(&mut self, id: u32) -> bool
    {
        // remove sound source from all node and instance components
        for scene in &mut self.scenes
        {
            let all_nodes = Scene::list_all_child_nodes(&scene.nodes);

            for node in all_nodes
            {
                let mut node = node.write().unwrap();

                node.components.retain(|component|
                {
                    let component = component.read().unwrap();

                    if let Some(sound) = component.as_any().downcast_ref::<Sound>()
                    {
                        if let Some(sound_source) = sound.sound_source.as_ref()
                        {
                            let sound_source = sound_source.read().unwrap();
                            if sound_source.id == id
                            {
                                return false;
                            }
                        }
                    }

                    true
                });

                for instance in node.instances.get_mut()
                {
                    let mut instance = instance.write().unwrap();

                    instance.components.retain(|component|
                    {
                        let component = component.read().unwrap();

                        if let Some(sound) = component.as_any().downcast_ref::<Sound>()
                        {
                            if let Some(sound_source) = sound.sound_source.as_ref()
                            {
                                let sound_source = sound_source.read().unwrap();
                                if sound_source.id == id
                                {
                                    return false;
                                }
                            }
                        }

                        true
                    });
                }
            }
        }

        // remove sound source
        let len = self.resources.sound_sources.len();
        self.resources.sound_sources.retain(|_key, sound|
        {
            let sound = sound.read().unwrap();
            sound.id != id
        });

        self.resources.sound_sources.len() != len
    }

    pub fn get_mesh_resource_by_id(&self, id: u32) -> Option<MeshResourceItem>
    {
        for mesh_arc in self.resources.mesh_resources.values()
        {
            let mesh =  mesh_arc.read().unwrap();
            if mesh.id == id
            {
                return Some(mesh_arc.clone());
            }
        }

        None
    }

    pub fn delete_mesh_resource_by_id(&mut self, id: u32) -> bool
    {
        // remove mesh resource from all node components
        for scene in &mut self.scenes
        {
            let all_nodes = Scene::list_all_child_nodes(&scene.nodes);

            for node in all_nodes
            {
                let mut node = node.write().unwrap();

                node.components.retain(|component|
                {
                    let component = component.read().unwrap();

                    if let Some(mesh) = component.as_any().downcast_ref::<Mesh>()
                    {
                        if let Some(mesh_resource) = mesh.mesh_resource.as_ref()
                        {
                            let mesh_resource = mesh_resource.read().unwrap();
                            if mesh_resource.id == id
                            {
                                return false;
                            }
                        }
                    }

                    true
                });
            }
        }

        // remove mesh resource
        let len = self.resources.mesh_resources.len();
        self.resources.mesh_resources.retain(|_key, mesh|
        {
            let mesh = mesh.read().unwrap();
            mesh.id != id
        });

        self.resources.mesh_resources.len() != len
    }

    pub fn load_scene_env_map(&mut self, path: &str, scene_id: u32)
    {
        let path = path.to_string().clone();

        //load default env texture
        let main_queue = self.main_thread_execution_queue.clone();
        let max_res = self.max_texture_resolution();
        spawn_thread(move ||
        {
            load_texture(path.as_str(), main_queue.clone(), Some(TextureType::Environment), Some(scene_id), None, true, max_res);
        });
    }

    pub fn add_scene(&mut self, name: &str) -> &mut SceneItem
    {
        let scenes_amount = self.scenes.len();

        let mut scene = crate::state::scene::scene::Scene::new(name);

        if scenes_amount == 0
        {
            scene.active = true;
        }

        scene.add_defaults();
        self.scenes.push(Box::new(scene));

        self.scenes.last_mut().unwrap()
    }

    pub fn get_active_scene(&self) -> Option<&SceneItem>
    {
        for scene in &self.scenes
        {
            if scene.active
            {
                return Some(&scene);
            }
        }

        None
    }

    pub fn get_active_scene_mut(&mut self) -> Option<&mut SceneItem>
    {
        for scene in &mut self.scenes
        {
            if scene.active
            {
                return Some(scene);
            }
        }

        None
    }

    pub fn get_active_scene_id(&self) -> Option<u32>
    {
        let scene = self.get_active_scene();
        if let Some(scene) = scene
        {
            return Some(scene.id);
        }
        None
    }

    pub fn set_active_scene(&mut self, id: u32)
    {
        for scene in &mut self.scenes
        {
            if scene.id == id
            {
                scene.active = true;
            }
            else
            {
                scene.active = false;
            }
        }
    }

    pub fn find_scene_by_id(&self, id: u32) -> Option<&SceneItem>
    {
        for scene in &self.scenes
        {
            if scene.id == id
            {
                return Some(&scene);
            }
        }

        None
    }

    pub fn find_scene_by_id_mut(&mut self, id: u32) -> Option<&mut SceneItem>
    {
        for scene in &mut self.scenes
        {
            if scene.id == id
            {
                return Some(scene);
            }
        }

        None
    }

    pub fn delete_scene_by_id(&mut self, id: u32, clear_resouces: bool) -> bool
    {
        let mut was_active = false;
        {
            for scene in &mut self.scenes
            {
                if scene.id == id
                {
                    if clear_resouces
                    {
                        scene.clear(true, true);
                    }

                    if scene.active
                    {
                        was_active = true;
                    }
                }
            }
        }

        let len = self.scenes.len();
        self.scenes.retain(|scene| scene.id != id);
        let success = self.scenes.len() != len;

        // mark another scene as active if the deleted one was active
        if was_active && self.scenes.len() > 0 && self.get_active_scene().is_none()
        {
            self.scenes[0].active = true;
        }

        success
    }

    pub fn delete_all_scenes(&mut self, clear_resouces: bool) -> bool
    {
        if clear_resouces
        {
            for scene in &mut self.scenes
            {
                scene.clear(true, true);
            }
        }

        let len = self.scenes.len();
        self.scenes.clear();

        len > 0 && self.scenes.is_empty()
    }

    pub fn max_texture_resolution(&self) -> u32
    {
        if let Some(max_tex_resolution) = self.rendering.max_texture_resolution
        {
            return max_tex_resolution;
        }

        self.rendering_adapter.max_texture_resolution
    }

    pub fn update(&mut self, time: u128, time_delta: f32, frame: u64)
    {
        // ********** update scenes **********
        for scene in &mut self.scenes
        {
            if !scene.active
            {
                continue;
            }

            scene.update(&mut self.io, time, time_delta, frame);
        }

        // ********** textures **********
        // check for delete later textures
        // see: delete_node_by_id
        self.resources.textures.retain(|_key, texture|
        {
            if texture.read().unwrap().delete_later_request
            {
                return false;
            }
            true
        });

        // ********** mesh resources **********
        // check for delete later mesh resources
        // see: delete_node_by_id
        self.resources.mesh_resources.retain(|_key, mesh_resource|
        {
            if mesh_resource.read().unwrap().delete_later_request
            {
                return false;
            }
            true
        });

        // update hash
        let mut mesh_resource_key_updates = vec![];
        for (key, mesh_resource) in &self.resources.mesh_resources
        {
            let new_hash = mesh_resource.read().unwrap().hash.clone();
            if key != &new_hash
            {
                mesh_resource_key_updates.push(key.clone());
            }
        }

        for old_mesh_resource_key in mesh_resource_key_updates
        {
            if let Some(value) = self.resources.mesh_resources.remove(&old_mesh_resource_key)
            {
                let new_hash = value.read().unwrap().hash.clone();
                self.resources.mesh_resources.insert(new_hash, value);
            }
        }

        // ********** sound sources **********
        // check for delete later sound sources
        // see: delete_node_by_id
        self.resources.sound_sources.retain(|_key, sound_source|
        {
            if sound_source.read().unwrap().delete_later_request
            {
                return false;
            }
            true
        });

        // ********** fire-and-forget sounds **********
        let mut finished_sound_sources: HashMap<u32, SoundSourceItem> = HashMap::new();
        self.oneshot_sounds.retain(|sound|
        {
            if sound.stopped()
            {
                if let Some(sound_source) = sound.sound_source.as_ref()
                {
                    finished_sound_sources.insert(sound_source.read().unwrap().id, sound_source.clone());
                }
                return false;
            }
            true
        });

        for (_, sound_source) in finished_sound_sources
        {
            if Arc::strong_count(&sound_source) == 2
            {
                sound_source.write().unwrap().delete_later();
            }
        }
    }

    pub fn clear(&mut self)
    {
        self.resources.textures.clear();
        self.oneshot_sounds.clear();
    }

    pub fn print(&self)
    {
        println!("");
        println!("ADAPTER:");
        println!(" - adapter: {}", self.rendering_adapter.name);
        println!(" - driver: {}", self.rendering_adapter.driver);
        println!(" - driver info: {}", self.rendering_adapter.driver_info);
        println!(" - backend: {}", self.rendering_adapter.backend);
        println!(" - storage_buffer_array_support: {}", self.rendering_adapter.storage_buffer_array_support);
        println!(" - occlusion_culling_support: {}", self.rendering_adapter.occlusion_culling_support);
        println!(" - ssao_support: {}", self.rendering_adapter.ssao_support);
        println!(" - max msaa_samples: {}", self.rendering_adapter.max_msaa_samples);

        println!("");

        println!("SCENES:");

        // print scenes
        for scene in &self.scenes
        {
            scene.print();
        }
    }
}