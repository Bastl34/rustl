#![allow(dead_code)]

use egui::RichText;
use nalgebra::{Matrix4, Point3, Point4, Vector3};
use parry3d::{bounding_volume::{Aabb, BoundingSphere}, math::{Pose3, Vec3}, query::{Ray, RayCast}, shape::{FeatureId, TriMesh}};
use serde::{Deserialize, Serialize};

use crate::{component_impl_default, component_impl_no_cleanup_node, component_impl_no_update, component_impl_set_enabled, console_error, gui::helper::info_box::info_box_with_body, helper::{change_tracker::ChangeTracker, option_or_id::OptionOrId}, state::{helper::render_item::RenderItemOption, resources::mesh_resource::MeshResourceItem, scene::node::NodeItem}};
use crate::state::scene::exporter::serialization_helper;


use super::component::{Component, ComponentBase};

pub const JOINTS_LIMIT: usize = 4;

// safety margin on the skinned bounding volume
// the joint boxes cover skinning and morph targets exactly - but only for morph weights within [0, 1]
// animated weights are deliberately not clamped: they are authored data, and cubic spline interpolation
// overshoots past 1 on its own even when every keyframe stays in range
// so going beyond that range is the normal case, not an exception - this margin is what covers it
// raise it per mesh for rigs that drive morph weights far past 1
const DEFAULT_SKIN_BOUNDING_VOLUME_SCALE: f32 = 1.1;

#[derive(Serialize, Deserialize)]
pub struct MeshData
{
    #[serde(skip, default)]
    pub b_box_skin: Option<Aabb>,

    #[serde(skip, default)]
    pub b_sphere_skin: Option<BoundingSphere>,

    // one entry per joint: the bind space bounding box of every vertex this joint influences
    // the animated bounding volume is the union of these boxes, each transformed by its own joint
    #[serde(skip, default)]
    pub skin_joint_bounds: Vec<Option<(Point3<f32>, Point3<f32>)>>,

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

            skin_joint_bounds: vec![],

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

        self.skin_joint_bounds.clear();

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

            skin_joint_bounds: vec![],

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

    // precomputed once per mesh: the bind space bounding box of every vertex a joint influences
    // the animated bounding volume is the union of these boxes transformed by their own joint
    // -> the per frame cost is O(joints) instead of O(vertices)
    pub fn calc_skin_joint_bounds(&mut self)
    {
        let mut bounds: Vec<Option<(Point3<f32>, Point3<f32>)>> = vec![];

        if let Some(mesh_resource) = self.mesh_resource.as_ref()
        {
            let mesh_resource = mesh_resource.read().unwrap();
            let data = mesh_resource.get_data();

            if data.joints.len() >= data.vertices.len() && data.weights.len() >= data.vertices.len()
            {
                for (v_i, v) in data.vertices.iter().enumerate()
                {
                    // morph targets move a vertex on their own - the joints know nothing about them
                    // every target weight can reach 1 independently, so the reachable range of a vertex is
                    // the sum of all negative offsets up to the sum of all positive ones
                    let mut morph_min = Vector3::<f32>::zeros();
                    let mut morph_max = Vector3::<f32>::zeros();

                    for morph_target in &data.morph_target_positions
                    {
                        if let Some(offset) = morph_target.get(v_i)
                        {
                            morph_min += Vector3::new(offset.x.min(0.0), offset.y.min(0.0), offset.z.min(0.0));
                            morph_max += Vector3::new(offset.x.max(0.0), offset.y.max(0.0), offset.z.max(0.0));
                        }
                    }

                    let v_min = Point3::new(v.x + morph_min.x, v.y + morph_min.y, v.z + morph_min.z);
                    let v_max = Point3::new(v.x + morph_max.x, v.y + morph_max.y, v.z + morph_max.z);

                    for i in 0..JOINTS_LIMIT
                    {
                        if data.weights[v_i][i] <= 0.0
                        {
                            continue;
                        }

                        let joint_id = data.joints[v_i][i] as usize;

                        // joint ids show up in arbitrary order, so grow the vec until this id fits
                        // (None = a joint that has no vertices at all)
                        while bounds.len() <= joint_id
                        {
                            bounds.push(None);
                        }

                        bounds[joint_id] = match bounds[joint_id]
                        {
                            None => Some((v_min, v_max)),
                            Some((min, max)) => Some
                            ((
                                Point3::new(min.x.min(v_min.x), min.y.min(v_min.y), min.z.min(v_min.z)),
                                Point3::new(max.x.max(v_max.x), max.y.max(v_max.y), max.z.max(v_max.z))
                            ))
                        };
                    }
                }
            }
        }

        self.data.get_mut().skin_joint_bounds = bounds;
    }

    // rebuilds the skinned bounding volume: every per joint box is transformed by its joint and all
    // 8 corners are merged - O(joints) instead of O(vertices), cheap enough to run on every pose change
    // returns whether the volume actually changed
    pub fn update_skin_bounding_volume_from_joints(&mut self, joint_matrices: &Vec<Matrix4<f32>>) -> bool
    {
        let mut min = Vector3::<f32>::new(f32::MAX, f32::MAX, f32::MAX);
        let mut max = Vector3::<f32>::new(-f32::MAX, -f32::MAX, -f32::MAX);
        let mut found = false;

        for (joint_id, joint_bounds) in self.get_data().skin_joint_bounds.iter().enumerate()
        {
            if joint_id >= joint_matrices.len()
            {
                continue;
            }

            let (joint_min, joint_max) = match joint_bounds
            {
                Some(joint_bounds) => joint_bounds,
                None => continue
            };

            let joint_matrix = &joint_matrices[joint_id];

            // transforming the corners also covers rotation and scale of the joint
            for corner in 0..8
            {
                let x = if corner & 1 == 0 { joint_min.x } else { joint_max.x };
                let y = if corner & 2 == 0 { joint_min.y } else { joint_max.y };
                let z = if corner & 4 == 0 { joint_min.z } else { joint_max.z };

                let p = joint_matrix.transform_point(&Point3::new(x, y, z));

                min = Vector3::new(min.x.min(p.x), min.y.min(p.y), min.z.min(p.z));
                max = Vector3::new(max.x.max(p.x), max.y.max(p.y), max.z.max(p.z));
            }

            found = true;
        }

        if !found
        {
            return false;
        }

        let b_box = Aabb::new(Vec3::new(min.x, min.y, min.z), Vec3::new(max.x, max.y, max.z));

        // nothing moved -> do not mark the scene as changed
        if let Some(old_b_box) = self.get_data().b_box_skin
        {
            if old_b_box.mins == b_box.mins && old_b_box.maxs == b_box.maxs
            {
                return false;
            }
        }

        let sphere_center = Vec3::new((min.x + max.x) * 0.5, (min.y + max.y) * 0.5, (min.z + max.z) * 0.5);
        let sphere_radius = ((max - min) * 0.5).norm();

        let data = self.data.get_mut();
        data.b_box_skin = Some(b_box);
        data.b_sphere_skin = Some(BoundingSphere::new(sphere_center, sphere_radius));

        true
    }

    pub fn get_skin_bbox_or_default(&self) -> Aabb
    {
        if let Some(b_box_skin) = self.get_data().b_box_skin
        {
            // scaled_wrt_center and not scaled: scaled() multiplies mins/maxs and would move the box
            // off the mesh for geometry that is far away from the origin
            let s = self.get_data().b_volume_skin_multiplier;
            return b_box_skin.scaled_wrt_center(Vec3::new(s, s, s));
        }

        if let Some(mesh_resource) = self.mesh_resource.as_ref()
        {
            let mesh_resource = mesh_resource.read().unwrap();
            let data = mesh_resource.get_data();

            return data.b_box;
        }

        Aabb::new_invalid()
    }

    pub fn get_skin_bbox(&self) -> Option<Aabb>
    {
        let data = self.get_data();

        if let Some(b_box_skin) = data.b_box_skin
        {
            let s = data.b_volume_skin_multiplier;
            return Some(b_box_skin.scaled_wrt_center(Vec3::new(s, s, s)));
        }

        None
    }

    pub fn get_skin_bounding_sphere(&self) -> Option<BoundingSphere>
    {
        let data = self.get_data();

        if let Some(b_sphere_skin) = data.b_sphere_skin
        {
            let s = data.b_volume_skin_multiplier;
            return Some(BoundingSphere::new(b_sphere_skin.center(), b_sphere_skin.radius() * s));
        }

        None
    }

    pub fn get_skin_bounding_sphere_or_default(&self) -> BoundingSphere
    {
        if let Some(b_sphere_skin) = self.get_skin_bounding_sphere()
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
        let b_box = self.get_skin_bbox_or_default();
        (b_box.maxs.y - b_box.mins.y).abs()
    }

    pub fn get_width(&self) -> f32
    {
        let b_box = self.get_skin_bbox_or_default();
        (b_box.maxs.x - b_box.mins.x).abs()
    }

    pub fn get_depth(&self) -> f32
    {
        let b_box = self.get_skin_bbox_or_default();
        (b_box.maxs.z - b_box.mins.z).abs()
    }

    pub fn intersect_b_box(&self, ray_inverse: &Ray, solid: bool) -> Option<f32>
    {
        let b_box = self.get_skin_bbox_or_default();

        b_box.cast_local_ray(&ray_inverse, std::f32::MAX, solid)
    }

    pub fn intersect_b_sphere(&self, ray_inverse: &Ray, solid: bool) -> Option<f32>
    {
        let b_sphere = self.get_skin_bounding_sphere_or_default();

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

            mesh_resource.write().unwrap().ui(ui);
        }

        ui.separator();

        if self.get_data().b_box_skin.is_some()
        {
            info_box_with_body(ui, |ui|
            {
                ui.label(RichText::new("Skinned Mesh BBox Margin").strong());
                ui.label("The bounding volume is built from the per joint boxes and already covers skinning and morph targets up to a weight of 1.");
                ui.label("Raise this only if something pushes vertices beyond that - overshooting morph weights or procedural deformation.");
            });

            let mut changed = false;
            let mut b_box_skin_multiplier = self.get_data().b_volume_skin_multiplier;

            ui.horizontal(|ui|
            {
                ui.label("Margin: ");
                changed = ui.add(egui::Slider::new(&mut b_box_skin_multiplier, 1.0..=10.0).fixed_decimals(2)).changed() || changed;
            });

            if changed
            {
                let data = self.get_data_mut().get_mut();
                data.b_volume_skin_multiplier = b_box_skin_multiplier;
            }
        }

        ui.checkbox(&mut self.update_skin_bbox_on_animation, "exact skin bbox (slow)")
.on_hover_text("off: the bbox is rebuilt from the per joint boxes (cheap)\non: it is recalculated from every vertex - exact but O(vertices)");
    }
}

#[cfg(test)]
mod tests
{
    use super::*;
    use std::sync::{Arc, RwLock};
    use crate::state::resources::mesh_resource::MeshResource;

    // two vertices, each rigidly bound to its own joint - joint 0 sits left, joint 1 right
    fn two_joint_mesh() -> Mesh
    {
        let vertices = vec!
        [
            Point3::new(-2.0, 0.0, 0.0),
            Point3::new(-1.0, 1.0, 0.0),
            Point3::new( 1.0, 0.0, 0.0),
            Point3::new( 2.0, 3.0, 0.0),
        ];
        let indices = vec![[0, 1, 2], [1, 2, 3]];

        let mut resource = MeshResource::new_with_data("test", vertices, indices, vec![], vec![], vec![], vec![]);
        {
            let data = resource.get_data_mut().get_mut();
            data.joints = vec![[0, 0, 0, 0], [0, 0, 0, 0], [1, 0, 0, 0], [1, 0, 0, 0]];
            data.weights = vec![[1.0, 0.0, 0.0, 0.0]; 4];
        }

        let mut mesh = Mesh::new("test");
        mesh.mesh_resource = OptionOrId::Some(Arc::new(RwLock::new(Box::new(resource))));
        mesh.calc_skin_joint_bounds();

        mesh
    }

    #[test]
    fn joint_bounds_cover_every_vertex_of_their_joint()
    {
        let mesh = two_joint_mesh();
        let bounds = &mesh.get_data().skin_joint_bounds;

        assert_eq!(bounds.len(), 2, "one box per joint");

        let (min0, max0) = bounds[0].unwrap();
        assert_eq!((min0.x, max0.x), (-2.0, -1.0));
        assert_eq!((min0.y, max0.y), (0.0, 1.0));

        let (min1, max1) = bounds[1].unwrap();
        assert_eq!((min1.x, max1.x), (1.0, 2.0));
        assert_eq!((min1.y, max1.y), (0.0, 3.0));
    }

    #[test]
    fn bind_pose_volume_is_exact()
    {
        let mut mesh = two_joint_mesh();

        // identity joints = bind pose -> the union has to be exactly the mesh bounds, no guess factor
        let changed = mesh.update_skin_bounding_volume_from_joints(&vec![Matrix4::identity(); 2]);
        assert!(changed);

        let b_box = mesh.get_data().b_box_skin.unwrap();
        assert!((b_box.mins.x - (-2.0)).abs() < 1.0e-6, "mins.x was {}", b_box.mins.x);
        assert!((b_box.maxs.x - 2.0).abs() < 1.0e-6, "maxs.x was {}", b_box.maxs.x);
        assert!((b_box.maxs.y - 3.0).abs() < 1.0e-6, "maxs.y was {}", b_box.maxs.y);
    }

    #[test]
    fn volume_follows_the_animated_joint()
    {
        let mut mesh = two_joint_mesh();

        // move joint 1 ten units up - only its own box may follow
        let mut joint_1 = Matrix4::<f32>::identity();
        joint_1[(1, 3)] = 10.0;

        mesh.update_skin_bounding_volume_from_joints(&vec![Matrix4::identity(), joint_1]);

        let b_box = mesh.get_data().b_box_skin.unwrap();
        assert!((b_box.maxs.y - 13.0).abs() < 1.0e-6, "maxs.y was {}", b_box.maxs.y);
        assert!((b_box.mins.y - 0.0).abs() < 1.0e-6, "mins.y was {}", b_box.mins.y);
    }

    #[test]
    fn unchanged_pose_reports_no_change()
    {
        let mut mesh = two_joint_mesh();
        let joints = vec![Matrix4::<f32>::identity(); 2];

        assert!(mesh.update_skin_bounding_volume_from_joints(&joints), "first call sets the volume");
        assert!(!mesh.update_skin_bounding_volume_from_joints(&joints), "same pose must not dirty the scene");
    }
}
