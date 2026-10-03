use std::sync::{Arc, RwLock};

use nalgebra::{Matrix4, Point3, Vector3, Vector4};
use serde::{Deserialize, Serialize};

use crate::{component_downcast_mut, helper::option_or_id::OptionOrId, state::{resources::mesh_resource::MeshResource, scene::{components::{material::{Material, MaterialItem}, mesh::Mesh, transformation::Transformation}, instance::Instance, node::{InstanceItemArc, Node, NodeItem}, scene::Scene}, state::ENGINE_INTERNAL_TAG}};

// below it a tire leaves no mark
const MIN_STRENGTH: f32 = 0.05;

// a jump of the contact point further than this (recover, teleport) starts a new mark instead of drawing a line
const MAX_GAP: f32 = 3.0;

// above the ground along its normal, so the marks do not flicker with it
const LIFT: f32 = 0.02;

// pieces made at once the first time, after that the amount doubles - every new instance count rebuilds buffers
const FIRST_BLOCK: usize = 256;

// size of a waiting piece - not 0, Transformation inverts its matrix
const COLLAPSED_SCALE: f32 = 0.0001;

fn default_true() -> bool { true }

#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
pub struct TireMarkSettings
{
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub opacity: f32, // at full slide

    // the width of each tire, measured by the auto setup - off: width for all of them
    #[serde(default = "default_true")]
    pub width_auto: bool,
    pub width: f32, // m

    pub segment_length: f32, // m of one piece of a mark
    pub max_pieces: usize, // the oldest pieces are reused beyond it
    pub color: Vector3<f32>,
}

impl Default for TireMarkSettings
{
    fn default() -> Self
    {
        Self { enabled: true, opacity: 0.6, width_auto: true, width: 0.22, segment_length: 0.3, max_pieces: 5000, color: Vector3::new(0.03, 0.03, 0.03) }
    }
}

// what a wheel does this frame
pub struct TireMarkInput
{
    pub ground: Option<(Vector3<f32>, Vector3<f32>)>, // contact point and normal, world space - None in the air
    pub strength: f32, // 0..1, how hard the tire slides
    pub width: f32, // m
}

// Rubber left on the ground where the tires slide - pieces of one quad mesh as instances of an internal node, made in blocks, reused in a ring.
#[derive(Default)]
pub struct TireMarks
{
    node: Option<NodeItem>,
    material: Option<MaterialItem>,
    pieces: Vec<InstanceItemArc>,
    used: usize, // pieces showing a mark, the rest wait collapsed
    next: usize, // the piece reused next once all exist
    last_points: Vec<Option<Vector3<f32>>>, // per wheel, where its current mark ends
}

impl TireMarks
{
    pub fn update(&mut self, scene: &mut Scene, settings: &TireMarkSettings, wheels: &[TireMarkInput])
    {
        if !settings.enabled
        {
            self.clear(scene);
            return;
        }

        self.last_points.resize(wheels.len(), None);

        for (index, wheel) in wheels.iter().enumerate()
        {
            let Some((point, normal)) = wheel.ground.filter(|_| wheel.strength >= MIN_STRENGTH) else
            {
                self.last_points[index] = None;
                continue;
            };

            let Some(last) = self.last_points[index] else
            {
                self.last_points[index] = Some(point);
                continue;
            };

            let distance = (point - last).norm();
            if distance > MAX_GAP
            {
                self.last_points[index] = Some(point);
            }
            else if distance >= settings.segment_length
            {
                let transform = piece_transform(last, point, normal, wheel.width);
                let color = Vector4::new(1.0, 1.0, 1.0, (wheel.strength * settings.opacity).clamp(0.0, 1.0));
                self.add_piece(scene, settings, transform, color);
                self.last_points[index] = Some(point);
            }
        }
    }

    fn add_piece(&mut self, scene: &mut Scene, settings: &TireMarkSettings, transform: Matrix4<f32>, color: Vector4<f32>)
    {
        let node = self.node(scene, settings);

        let max_pieces = settings.max_pieces.max(1);
        if self.used == self.pieces.len() && self.pieces.len() < max_pieces
        {
            self.grow(&node, max_pieces, &transform);
        }

        // a waiting piece, or once all are in use the oldest one moves here
        let index = if self.used < self.pieces.len()
        {
            self.used += 1;
            self.used - 1
        }
        else
        {
            self.next %= self.pieces.len();
            self.next += 1;
            self.next - 1
        };

        let mut piece = self.pieces[index].write().unwrap();

        if let Some(transformation) = piece.find_component::<Transformation>()
        {
            component_downcast_mut!(transformation, Transformation);
            transformation.set_local_transform(transform);
        }

        set_piece(&mut piece, transform, color);
    }

    // The waiting pieces are invisible and practically without size - collapsed onto the current mark, so the bounding sphere does not reach the origin.
    fn grow(&mut self, node: &NodeItem, max_pieces: usize, at: &Matrix4<f32>)
    {
        let amount = self.pieces.len().max(FIRST_BLOCK).min(max_pieces - self.pieces.len());
        let collapsed = Matrix4::new_translation(&Vector3::new(at[(0, 3)], at[(1, 3)], at[(2, 3)])) * Matrix4::new_scaling(COLLAPSED_SCALE);

        for _ in 0..amount
        {
            let mut instance = Instance::new_with_transform("tire mark".to_string(), node.clone(), Transformation::new_transformation_only("Transform", collapsed));
            instance.pickable = false;
            instance.get_data_mut().get_mut().collision = false;
            set_piece(&mut instance, collapsed, Vector4::zeros());

            self.pieces.push(node.write().unwrap().add_instance(Box::new(instance)));
        }
    }

    // the internal node carrying the pieces - made on the first mark, again if the scene dropped it
    fn node(&mut self, scene: &mut Scene, settings: &TireMarkSettings) -> NodeItem
    {
        if let Some(node) = self.node.as_ref()
        {
            let id = node.read().unwrap().id;
            if scene.find_node_by_id(id).is_some()
            {
                return node.clone();
            }

            self.pieces.clear();
            self.used = 0;
            self.next = 0;
        }

        // a piece is 1 x 1: x across the mark, z along it, y the ground normal
        let mesh_resource = MeshResource::new_plane("tire mark", Point3::new(-0.5, 0.0, 1.0), Point3::new(0.5, 0.0, 1.0), Point3::new(0.5, 0.0, 0.0), Point3::new(-0.5, 0.0, 0.0));
        let mut mesh = Mesh::new("tire mark");
        mesh.mesh_resource = OptionOrId::Some(Arc::new(RwLock::new(Box::new(mesh_resource))));

        let mut material = Material::new("tire marks");
        {
            let data = material.get_data_mut().get_mut();
            data.base_color = settings.color;
            data.specular_color = Vector3::zeros();
            data.alpha = 0.99; // transparent pass - the pieces carry their own opacity
            data.cast_shadow = false;
            data.allow_xray = false;
        }
        let material: MaterialItem = Arc::new(RwLock::new(Box::new(material)));
        scene.add_material(&material);

        let node = Node::new("Tire Marks");
        {
            let mut node = node.write().unwrap();
            node.add_component(Arc::new(RwLock::new(Box::new(mesh))));
            node.add_component(material.clone());
            node.tags.insert(ENGINE_INTERNAL_TAG);
            node.set_skip_instance_update(true); // set_piece writes where each piece is drawn

            let settings = &mut node.settings;
            settings.transient = true;
            settings.pickable = false;
            settings.collision = false;
            settings.camera_collision = false;
            settings.depth_write = false;
            settings.occlusion_culling = false; // one box over the whole track never hides, but costs every piece each frame
        }

        scene.add_node(node.clone());

        self.node = Some(node.clone());
        self.material = Some(material);
        node
    }

    // the colour of the marks already on the ground follows the setting
    pub fn apply_color(&self, color: Vector3<f32>)
    {
        if let Some(material) = self.material.as_ref()
        {
            component_downcast_mut!(material, Material);
            if material.get_data().base_color != color
            {
                material.get_data_mut().get_mut().base_color = color;
            }
        }
    }

    // the marks go - back in the editor, the vehicle removed
    pub fn clear(&mut self, scene: &mut Scene)
    {
        if let Some(node) = self.node.take()
        {
            let id = node.read().unwrap().id;
            scene.delete_node_by_id(id, true, false, true, false);
        }

        self.reset();
    }

    // without the scene at hand: the node deletes itself with the next update
    pub fn clear_later(&mut self)
    {
        if let Some(node) = self.node.take()
        {
            node.write().unwrap().delete_later();
        }

        self.reset();
    }

    fn reset(&mut self)
    {
        self.material = None;
        self.pieces.clear();
        self.used = 0;
        self.next = 0;
        self.last_points.clear();
    }
}

// The node update that computes where an instance is drawn already ran this frame - without this the piece shows at the origin for a frame.
fn set_piece(instance: &mut Instance, transform: Matrix4<f32>, color: Vector4<f32>)
{
    let data = instance.get_data_mut().get_mut();
    data.color = color;
    data.computed.world_matrix = transform;
    data.computed.alpha = color.w;
}

// the quad from one mark point to the next, lying on the ground
fn piece_transform(from: Vector3<f32>, to: Vector3<f32>, normal: Vector3<f32>, width: f32) -> Matrix4<f32>
{
    let normal = normal.try_normalize(1e-6).unwrap_or_else(Vector3::y);

    let along = to - from;
    let along = along - normal * along.dot(&normal);
    let length = along.norm().max(1e-4);
    let forward = along / length;
    let side = normal.cross(&forward);

    let origin = from + normal * LIFT;

    let mut transform = Matrix4::identity();
    transform.fixed_view_mut::<3, 1>(0, 0).copy_from(&(side * width));
    transform.fixed_view_mut::<3, 1>(0, 1).copy_from(&normal);
    transform.fixed_view_mut::<3, 1>(0, 2).copy_from(&(forward * length));
    transform.fixed_view_mut::<3, 1>(0, 3).copy_from(&origin);
    transform
}
