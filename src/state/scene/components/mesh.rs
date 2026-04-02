#![allow(dead_code)]

use egui::RichText;
use nalgebra::{Matrix4, Point3, Point4, Vector3};
use parry3d::{bounding_volume::{Aabb, BoundingSphere}, math::{Pose3, Vec3}, query::{Ray, RayCast}, shape::{FeatureId, TriMesh}};
use serde::{Deserialize, Serialize};

use crate::{component_impl_default, component_impl_no_cleanup_node, component_impl_no_update, component_impl_set_enabled, console_error, gui::helper::info_box::info_box_with_body, helper::{change_tracker::ChangeTracker, option_or_id::OptionOrId}, state::{helper::render_item::RenderItemOption, resources::mesh_resource::MeshResourceItem, scene::node::NodeItem}};
use crate::state::scene::exporter::serialization_helper;


use super::component::{Component, ComponentBase};

pub const JOINTS_LIMIT: usize = 4;
const DEFAULT_SKIN_BOUNDING_VOLUME_SCALE: f32 = 2.0; // the skinned mesh bbox is multiplied by this factor -> because a bbox for an animated mesh can not be correctly calculated - just simply is a large factor

#[derive(Serialize, Deserialize)]
pub struct MeshData
{
    #[serde(skip, default)]
    pub b_box_skin: Option<Aabb>,

    #[serde(skip, default)]
    pub b_sphere_skin: Option<BoundingSphere>,

    pub b_volume_skin_multiplier: f32,
}

impl Default for MeshData
{
    fn default() -> Self
    {
        Self
        {
            b_box_skin: None, // armature space
            b_sphere_skin: None, // armature space

            b_volume_skin_multiplier: DEFAULT_SKIN_BOUNDING_VOLUME_SCALE
        }
    }
}

impl MeshData
{
    pub fn clear(&mut self)
    {
        self.b_box_skin = None;
        self.b_sphere_skin = None;

        self.b_volume_skin_multiplier = DEFAULT_SKIN_BOUNDING_VOLUME_SCALE;
    }
}

#[derive(Serialize, Deserialize)]
pub struct Mesh
{
    base: ComponentBase,

    data: ChangeTracker<MeshData>,

    #[serde(serialize_with = "serialization_helper::serialize_mesh_resource", deserialize_with = "serialization_helper::deserialize_mesh_resource")]
    pub mesh_resource: OptionOrId<MeshResourceItem>,

    #[serde(skip)]
    pub morph_target_render_item: RenderItemOption,

    pub update_skin_bbox_on_animation: bool,
}

impl Mesh
{
    pub fn new(name: &str) -> Mesh
    {
        let mesh_data = MeshData
        {
            b_box_skin: None,
            b_sphere_skin: None,

            b_volume_skin_multiplier: DEFAULT_SKIN_BOUNDING_VOLUME_SCALE
        };

        let mesh = Mesh
        {
            base: ComponentBase::new(name.to_string(), "Mesh".to_string(), "◼".to_string()),
            data: ChangeTracker::new(mesh_data),

            mesh_resource: OptionOrId::None,

            morph_target_render_item: None,

            update_skin_bbox_on_animation: false
        };

        mesh
    }

    pub fn get_data(&self) -> &MeshData
    {
        &self.data.get_ref()
    }

    pub fn get_data_mut(&mut self) -> &mut ChangeTracker<MeshData>
    {
        &mut self.data
    }

    pub fn calc_bounding_volume_skin(&mut self, joint_matrices: &Vec<Matrix4<f32>>)
    {
        if let Some(mesh_resource) = self.mesh_resource.as_ref()
        {
            let mesh_resource = mesh_resource.read().unwrap();
            let mesh_resource_data = mesh_resource.get_data();

            // transform by skin
            let vertices = mesh_resource_data.vertices.iter().enumerate().map(|(v_i, v)|
            {
                let pos = Point4::<f32>::new(v.x, v.y, v.z, 1.0);
                let mut transformed_pos = Point4::<f32>::new(0.0, 0.0, 0.0, 0.0);

                for i in 0..JOINTS_LIMIT
                {
                    let joints = mesh_resource_data.joints[v_i];
                    let weights = mesh_resource_data.weights[v_i];

                    let joint_transform = joint_matrices[joints[i] as usize];
                    let transformed = joint_transform * pos * weights[i];

                    transformed_pos.x += transformed.x;
                    transformed_pos.y += transformed.y;
                    transformed_pos.z += transformed.z;
                    transformed_pos.w += transformed.w;
                }

                transformed_pos.x /= transformed_pos.w;
                transformed_pos.y /= transformed_pos.w;
                transformed_pos.z /= transformed_pos.w;

                Vec3::new(transformed_pos.x, transformed_pos.y, transformed_pos.z)
            }).collect::<Vec<Vec3>>();

            let mesh = TriMesh::new(vertices.clone(), mesh_resource_data.indices.clone()).unwrap();

            let trans = Pose3::identity();

            let data = self.data.get_mut();
            data.b_box_skin = Some(mesh.aabb(&trans));
            data.b_sphere_skin = Some(mesh.bounding_sphere(&trans));
        }
        else
        {
            console_error!("can not find mesh resource");

            let data = self.data.get_mut();
            data.b_box_skin = None;
            data.b_sphere_skin = None;
        }
    }

    pub fn get_scaled_skin_bbox_or_default(&self) -> Aabb
    {
        if let Some(b_box_skin) = self.get_data().b_box_skin
        {
            let s = self.get_data().b_volume_skin_multiplier;
            let b_box_skin_scaled = b_box_skin.scaled(Vec3::new(s, s, s));

            return b_box_skin_scaled;
        }

        if let Some(mesh_resource) = self.mesh_resource.as_ref()
        {
            let mesh_resource = mesh_resource.read().unwrap();
            let data = mesh_resource.get_data();

            return data.b_box;
        }

        Aabb::new_invalid()
    }

    pub fn get_scaled_skin_bbox(&self) -> Option<Aabb>
    {
        let data = self.get_data();

        if let Some(b_box_skin) = data.b_box_skin
        {
            let s = data.b_volume_skin_multiplier;
            let b_box_skin_scaled = b_box_skin.scaled(Vec3::new(s, s, s));

            return Some(b_box_skin_scaled);
        }

        None
    }

    pub fn get_scaled_skin_bounding_sphere(&self) -> Option<BoundingSphere>
    {
        let data = self.get_data();

        if let Some(b_sphere_skin) = data.b_sphere_skin
        {
            let s = data.b_volume_skin_multiplier;
            let b_sphere_skin = BoundingSphere::new
            (
                b_sphere_skin.center(),
                b_sphere_skin.radius() * s
            );

            return Some(b_sphere_skin);
        }

        None
    }

    pub fn get_scaled_skin_bounding_sphere_or_default(&self) -> BoundingSphere
    {
        if let Some(b_sphere_skin) = self.get_scaled_skin_bounding_sphere()
        {
            return b_sphere_skin;
        }

        if let Some(mesh_resource) = self.mesh_resource.as_ref()
        {
            let mesh_resource = mesh_resource.read().unwrap();
            let data = mesh_resource.get_data();

            return data.b_sphere;
        }

        BoundingSphere::new(Vec3::new(0.0, 0.0, 0.0), 0.0)
    }

    pub fn get_height(&self) -> f32
    {
        let b_box = self.get_scaled_skin_bbox_or_default();
        (b_box.maxs.y - b_box.mins.y).abs()
    }

    pub fn get_width(&self) -> f32
    {
        let b_box = self.get_scaled_skin_bbox_or_default();
        (b_box.maxs.x - b_box.mins.x).abs()
    }

    pub fn get_depth(&self) -> f32
    {
        let b_box = self.get_scaled_skin_bbox_or_default();
        (b_box.maxs.z - b_box.mins.z).abs()
    }

    pub fn intersect_b_box(&self, ray_inverse: &Ray, solid: bool) -> Option<f32>
    {
        let b_box = self.get_scaled_skin_bbox_or_default();

        b_box.cast_local_ray(&ray_inverse, std::f32::MAX, solid)
    }

    pub fn intersect_b_sphere(&self, ray_inverse: &Ray, solid: bool) -> Option<f32>
    {
        let b_sphere = self.get_scaled_skin_bounding_sphere_or_default();

        b_sphere.cast_local_ray(&ray_inverse, std::f32::MAX, solid)
    }

    pub fn intersect(&self, ray: &Ray, ray_inverse: &Ray, trans: &Matrix4<f32>, trans_inverse: &Matrix4<f32>, solid: bool, smooth_shading: bool) -> Option<(f32, Vector3<f32>, u32)>
    {
        if let Some(mesh_resource) = self.mesh_resource.as_ref()
        {
            let mesh_resource = mesh_resource.read().unwrap();

            let data = mesh_resource.get_data();

            let res = data.mesh.cast_local_ray_and_get_normal(&ray_inverse, std::f32::MAX, solid);
            if let Some(res) = res
            {
                let mut face_id = 0;
                if let FeatureId::Face(i) = res.feature
                {
                    face_id = i;
                }

                let mut normal;

                // use normal based on loaded normal (not on computed normal by parry -- for smooth shading)
                if smooth_shading && data.normals.len() > 0 && data.normals_indices.len() > 0
                {
                    let hit = ray.origin + (ray.dir * res.time_of_impact);
                    normal = mesh_resource.get_normal(hit.into(), face_id, trans_inverse, &data.vertices);
                    normal = (trans * normal.to_homogeneous()).xyz().normalize();

                    if data.mesh.is_backface(res.feature)
                    {
                        normal = -normal;
                    }
                }
                else
                {
                    let res_normal: Vector3<f32> = res.normal.into();
                    normal = (trans * res_normal.to_homogeneous()).xyz().normalize();
                }

                let time_of_impact = res.time_of_impact * ray.dir.length();

                return Some((time_of_impact, normal, face_id))
                //return Some((res.time_of_impact, normal, face_id))
            }
        }
        None
    }

    pub fn intersect_morphed_and_skinned(&self, ray: &Ray, ray_inverse: &Ray, trans: &Matrix4<f32>, trans_inverse: &Matrix4<f32>, joint_matrices: &Vec<Matrix4<f32>>, morph_target_weights: &Vec<f32>, solid: bool, smooth_shading: bool) -> Option<(f32, Vector3<f32>, u32)>
    {
        if let Some(mesh_resource) = self.mesh_resource.as_ref()
        {
            let mesh_resource = mesh_resource.read().unwrap();

            if mesh_resource.get_data().joints.len() == 0 || mesh_resource.get_data().weights.len() == 0 || joint_matrices.len() == 0
            {
                return self.intersect(ray, ray_inverse, trans, trans_inverse, solid, smooth_shading);
            }

            let data = mesh_resource.get_data();

            // transform by skin
            let vertices = data.vertices.iter().enumerate().map(|(v_i, v)|
            {
                let mut pos = Point4::<f32>::new(v.x, v.y, v.z, 1.0);
                let mut skinned_pos = Point4::<f32>::new(0.0, 0.0, 0.0, 0.0);

                // morph targets
                for i in 0..morph_target_weights.len()
                {
                    let weight = morph_target_weights[i];

                    let morph_pos = data.morph_target_positions[i][v_i];
                    pos.x += morph_pos.x * weight;
                    pos.y += morph_pos.y * weight;
                    pos.z += morph_pos.z * weight;
                }

                // joints
                for i in 0..JOINTS_LIMIT
                {
                    let joints = data.joints[v_i];
                    let weights = data.weights[v_i];

                    let joint_transform = joint_matrices[joints[i] as usize];
                    let transformed = joint_transform * pos * weights[i];

                    skinned_pos.x += transformed.x;
                    skinned_pos.y += transformed.y;
                    skinned_pos.z += transformed.z;
                    skinned_pos.w += transformed.w;
                }

                skinned_pos.x /= skinned_pos.w;
                skinned_pos.y /= skinned_pos.w;
                skinned_pos.z /= skinned_pos.w;

                Point3::new(skinned_pos.x, skinned_pos.y, skinned_pos.z)
            }).collect::<Vec<Point3<f32>>>();

            let vertices_vec3 = vertices.iter().map(|p| Vec3::new(p.x, p.y, p.z)).collect::<Vec<Vec3>>();
            let mesh = TriMesh::new(vertices_vec3, data.indices.clone()).unwrap();

            // run intersection test
            let res = mesh.cast_local_ray_and_get_normal(&ray_inverse, std::f32::MAX, solid);
            if let Some(res) = res
            {
                let mut face_id = 0;
                if let FeatureId::Face(i) = res.feature
                {
                    face_id = i;
                }

                let mut normal;

                // use normal based on loaded normal (not on computed normal by parry -- for smooth shading)
                if smooth_shading && data.normals.len() > 0 && data.normals_indices.len() > 0
                {
                    let hit = ray.origin + (ray.dir * res.time_of_impact);
                    normal = mesh_resource.get_normal(hit.into(), face_id, trans_inverse, &vertices);
                    normal = (trans * normal.to_homogeneous()).xyz().normalize();

                    if mesh.is_backface(res.feature)
                    {
                        normal = -normal;
                    }
                }
                else
                {
                    let res_normal: Vector3<f32> = res.normal.into();
                    normal = (trans * res_normal.to_homogeneous()).xyz().normalize();
                }

                let time_of_impact = res.time_of_impact * ray.dir.length();

                return Some((time_of_impact, normal, face_id))
                //return Some((res.time_of_impact, normal, face_id))
            }
        }
        None
    }
}

#[typetag::serde]
impl Component for Mesh
{
    component_impl_default!();
    component_impl_no_update!();
    component_impl_set_enabled!();
    component_impl_no_cleanup_node!();

    fn run_after_deserialize(&mut self, context: &mut crate::state::scene::components::component::DeserializationContext)
    {
        if let Some(mesh_res_uuid) = self.mesh_resource.id()
        {
            let mesh_resource_found = context.mesh_resources.iter().find(|mesh_res| mesh_res.read().unwrap().uuid == mesh_res_uuid);
            if let Some(mesh_resource_found) = mesh_resource_found
            {
                self.mesh_resource = OptionOrId::Some(mesh_resource_found.clone());
            }
        }
    }

    fn instantiable() -> bool
    {
        false
    }

    fn duplicatable(&self) -> bool
    {
        false
    }

    fn duplicate(&self) -> Option<crate::state::scene::components::component::ComponentItem>
    {
        None
    }

    fn ui(&mut self, ui: &mut egui::Ui, _node: Option<NodeItem>)
    {
        if let Some(mesh_resource) = self.mesh_resource.as_ref()
        {
            mesh_resource.read().unwrap().ui_info(ui);

            if let Some(b_box_skin) = self.get_data().b_box_skin.as_ref()
            {
                ui.label(format!(" ⚫ bbox skin min: [{:.3}, {:.3}, {:.3}]", b_box_skin.mins.x, b_box_skin.mins.z, b_box_skin.mins.z));
                ui.label(format!(" ⚫ bbox skin max: [{:.3}, {:.3}, {:.3}]", b_box_skin.maxs.x, b_box_skin.maxs.z, b_box_skin.maxs.z));
            }

            if let Some(b_sphere_skin) = self.get_data().b_sphere_skin.as_ref()
            {
                ui.label(format!(" ⚫ b sphere skin: [{:.3}, {:.3}, {:.3}] radius: {:.3}", b_sphere_skin.center.x, b_sphere_skin.center.y, b_sphere_skin.center.z, b_sphere_skin.radius));
            }
        }

        ui.separator();

        ui.checkbox(&mut self.update_skin_bbox_on_animation, "update skin bbox on animation change");

        if self.get_data().b_box_skin.is_some()
        {
            info_box_with_body(ui, |ui|
            {
                ui.label(RichText::new("Skined Mesh BBox Factor").strong());
                ui.label("This is used to be able to check ray intersections more performant.");
                ui.label("Its based on the Skinned mesh with out animation multiplied by this factor.");
            });

            let mut changed = false;
            let mut b_box_skin_multiplier;
            {
                b_box_skin_multiplier = self.get_data().b_volume_skin_multiplier;
            }

            ui.horizontal(|ui|
            {
                ui.label("Factor: ");
                changed = ui.add(egui::Slider::new(&mut b_box_skin_multiplier, 1.0..=100.0).fixed_decimals(2)).changed() || changed;
            });

            if changed
            {
                let data = self.get_data_mut().get_mut();
                data.b_volume_skin_multiplier = b_box_skin_multiplier;
            }
        }
    }
}