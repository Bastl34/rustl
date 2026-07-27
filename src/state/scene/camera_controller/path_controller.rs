#![allow(dead_code)]

use nalgebra::{Point3, Vector3};
use serde::{Deserialize, Serialize};
use strum::IntoEnumIterator;

use crate::{camera_controller_impl_default, helper::{change_tracker::ChangeTracker, curve::BezierPath, easing::{easing, Easing}, math::approx_zero_vec3}, state::{scene::{camera::CameraData, node::NodeItem, scene::Scene}, state::{get_delta_t, InputOutput}}};

use super::camera_controller::{CameraController, CameraControllerBase};

const DEFAULT_CIRCLE_RADIUS: f32 = 5.0;
const DEFAULT_CIRCLE_HEIGHT: f32 = 1.0;
const DEFAULT_SPEED: f32 = 2.0;

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Copy)]
pub enum PathSpace
{
    World,
    Node,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Copy)]
pub enum PathLoopMode
{
    Once,
    Loop,
    PingPong,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Copy)]
pub enum PathOrientation
{
    PathDirection,
    LookAt,
    Keep,
}

#[derive(Serialize, Deserialize)]
#[serde(default)]
pub struct PathControllerData
{
    pub path: BezierPath,

    pub space: PathSpace,
    pub loop_mode: PathLoopMode,
    pub orientation: PathOrientation,
    pub easing: Easing,

    // units per second - negative speed runs the path backwards
    pub speed: f32,

    pub look_at_pos: Point3<f32>,
    pub up: Vector3<f32>,
    pub offset: Vector3<f32>,

    pub playing: bool,

    // traveled distance on the path
    pub progress: f32,

    // 1.0 or -1.0 (flipped by ping pong)
    pub direction: f32,
}

impl Default for PathControllerData
{
    fn default() -> Self
    {
        Self
        {
            path: BezierPath::circle(Point3::<f32>::new(0.0, DEFAULT_CIRCLE_HEIGHT, 0.0), DEFAULT_CIRCLE_RADIUS),

            space: PathSpace::World,
            loop_mode: PathLoopMode::Loop,
            orientation: PathOrientation::PathDirection,
            easing: Easing::None,

            speed: DEFAULT_SPEED,

            look_at_pos: Point3::<f32>::origin(),
            up: Vector3::<f32>::y(),
            offset: Vector3::<f32>::zeros(),

            playing: true,
            progress: 0.0,
            direction: 1.0,
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct PathController
{
    base: CameraControllerBase,

    pub data: ChangeTracker<PathControllerData>,
}

impl PathController
{
    pub fn new() -> PathController
    {
        PathController
        {
            base: CameraControllerBase::new("Path Controller".to_string(), "〰".to_string()),

            data: ChangeTracker::new(PathControllerData::default()),
        }
    }

    pub fn default() -> Self
    {
        PathController::new()
    }
}

#[typetag::serde]
impl CameraController for PathController
{
    camera_controller_impl_default!();

    fn run_after_deserialize(&mut self, _context: &mut crate::state::scene::components::component::DeserializationContext)
    {
        self.data.get_unmarked_mut().path.rebuild_cache();
    }

    fn update(&mut self, node: Option<NodeItem>, _scene: &mut Scene, _io: &mut InputOutput, cam_data: &mut ChangeTracker<CameraData>, frame_scale: f32) -> bool
    {
        let data_changed = self.data.consume_change();
        let data = self.data.get_unmarked_mut();

        if data_changed || !data.path.has_cache()
        {
            data.path.rebuild_cache();
        }

        if data.path.segment_count() == 0 || data.path.total_length() <= 0.0
        {
            return false;
        }

        let length = data.path.total_length();

        if data.playing
        {
            let progress = data.progress + (data.speed * data.direction * get_delta_t(frame_scale));

            match data.loop_mode
            {
                PathLoopMode::Once =>
                {
                    data.progress = progress.clamp(0.0, length);

                    // stop at the path end
                    if (data.speed > 0.0 && data.progress >= length) || (data.speed < 0.0 && data.progress <= 0.0)
                    {
                        data.playing = false;
                    }
                },
                PathLoopMode::Loop =>
                {
                    data.progress = progress.rem_euclid(length);
                },
                PathLoopMode::PingPong =>
                {
                    let mut progress = progress;

                    if progress > length
                    {
                        progress = (2.0 * length) - progress;
                        data.direction = -data.direction;
                    }
                    else if progress < 0.0
                    {
                        progress = -progress;
                        data.direction = -data.direction;
                    }

                    data.progress = progress.clamp(0.0, length);
                },
            }
        }
        else if !data_changed && data.space == PathSpace::World
        {
            // nothing to do while paused (node space still updates - the node can move)
            return false;
        }

        // easing is applied on the normalized progress
        let normalized = data.progress / length;
        let eased = easing(data.easing, normalized).clamp(0.0, 1.0);

        let Some((pos, tangent)) = data.path.sample_at_distance(eased * length) else { return false; };

        // offset is applied in path space
        let mut pos = pos + data.offset;
        let mut tangent = tangent;
        let mut look_at_pos = data.look_at_pos;

        // node space: path, offset and look at target are relative to the camera target node
        if data.space == PathSpace::Node
        {
            if let Some(node) = node
            {
                let node = node.read().unwrap();
                let transform = node.get_full_transform();

                pos = transform.transform_point(&pos);
                tangent = transform.transform_vector(&tangent);
                look_at_pos = transform.transform_point(&look_at_pos);
            }
        }

        // look where the camera is traveling (also when running backwards)
        let travel_dir = if data.speed < 0.0 { -data.direction } else { data.direction };

        let cam_data = cam_data.get_mut();
        cam_data.eye_pos = pos;

        match data.orientation
        {
            PathOrientation::PathDirection =>
            {
                let dir = tangent * travel_dir;

                if !approx_zero_vec3(&dir)
                {
                    cam_data.dir = dir.normalize();
                }
            },
            PathOrientation::LookAt =>
            {
                let dir = look_at_pos - pos;

                if !approx_zero_vec3(&dir)
                {
                    cam_data.dir = dir.normalize();
                }
            },
            PathOrientation::Keep => {},
        }

        if data.orientation != PathOrientation::Keep && !approx_zero_vec3(&data.up)
        {
            cam_data.up = data.up.normalize();
        }

        true
    }

    fn ui(&mut self, ui: &mut egui::Ui)
    {
        let data = self.data.get_ref();

        let mut playing = data.playing;
        let mut speed = data.speed;
        let mut space = data.space;
        let mut loop_mode = data.loop_mode;
        let mut orientation = data.orientation;
        let mut easing_type = data.easing;
        let mut look_at_pos = data.look_at_pos;
        let mut up = data.up;
        let mut offset = data.offset;
        let mut direction = data.direction;

        let length = data.path.total_length();
        let mut normalized_progress = if length > 0.0 { data.progress / length } else { 0.0 };

        let mut points = data.path.points.clone();
        let mut closed = data.path.closed;

        let mut changed = false;
        let mut progress_changed = false;
        let mut points_changed = false;
        let mut reset_circle = false;

        ui.horizontal(|ui|
        {
            if ui.button(if playing { "⏸ Pause" } else { "▶ Play" }).clicked()
            {
                playing = !playing;
                changed = true;
            }

            if ui.button("⏮ Reset").clicked()
            {
                normalized_progress = 0.0;
                direction = 1.0;
                progress_changed = true;
            }

            ui.label(format!("Length: {:.2}", length));
        });

        ui.horizontal(|ui|
        {
            ui.label("Progress:");
            progress_changed = ui.add(egui::Slider::new(&mut normalized_progress, 0.0..=1.0)).changed() || progress_changed;
        });

        ui.horizontal(|ui|
        {
            ui.label("Speed:");
            changed = ui.add(egui::DragValue::new(&mut speed).speed(0.1).suffix(" u/s")).changed() || changed;
        });

        ui.horizontal(|ui|
        {
            ui.label("Loop mode:");
            egui::ComboBox::from_id_salt(ui.make_persistent_id("path_loop_mode")).selected_text(match loop_mode { PathLoopMode::Once => "Once", PathLoopMode::Loop => "Loop", PathLoopMode::PingPong => "Ping Pong" }).show_ui(ui, |ui|
            {
                changed = ui.selectable_value(&mut loop_mode, PathLoopMode::Once, "Once").changed() || changed;
                changed = ui.selectable_value(&mut loop_mode, PathLoopMode::Loop, "Loop").changed() || changed;
                changed = ui.selectable_value(&mut loop_mode, PathLoopMode::PingPong, "Ping Pong").changed() || changed;
            });
        });

        ui.horizontal(|ui|
        {
            ui.label("Space:");
            egui::ComboBox::from_id_salt(ui.make_persistent_id("path_space")).selected_text(match space { PathSpace::World => "World", PathSpace::Node => "Target Node" }).show_ui(ui, |ui|
            {
                changed = ui.selectable_value(&mut space, PathSpace::World, "World").changed() || changed;
                changed = ui.selectable_value(&mut space, PathSpace::Node, "Target Node").changed() || changed;
            });
        });

        ui.horizontal(|ui|
        {
            ui.label("Orientation:");
            egui::ComboBox::from_id_salt(ui.make_persistent_id("path_orientation")).selected_text(match orientation { PathOrientation::PathDirection => "Path Direction", PathOrientation::LookAt => "Look At", PathOrientation::Keep => "Keep" }).show_ui(ui, |ui|
            {
                changed = ui.selectable_value(&mut orientation, PathOrientation::PathDirection, "Path Direction").changed() || changed;
                changed = ui.selectable_value(&mut orientation, PathOrientation::LookAt, "Look At").changed() || changed;
                changed = ui.selectable_value(&mut orientation, PathOrientation::Keep, "Keep").changed() || changed;
            });
        });

        ui.horizontal(|ui|
        {
            ui.label("Easing:");
            egui::ComboBox::from_id_salt(ui.make_persistent_id("path_easing")).selected_text(easing_type.to_string()).show_ui(ui, |ui|
            {
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                ui.set_min_width(60.0);

                for item in Easing::iter()
                {
                    changed = ui.selectable_value(&mut easing_type, item, item.to_string()).changed() || changed;
                }
            });
        });

        if orientation == PathOrientation::LookAt
        {
            ui.horizontal(|ui|
            {
                ui.label("Look at:");
                changed = ui.add(egui::DragValue::new(&mut look_at_pos.x).speed(0.1).prefix("x: ")).changed() || changed;
                changed = ui.add(egui::DragValue::new(&mut look_at_pos.y).speed(0.1).prefix("y: ")).changed() || changed;
                changed = ui.add(egui::DragValue::new(&mut look_at_pos.z).speed(0.1).prefix("z: ")).changed() || changed;
            });
        }

        if orientation != PathOrientation::Keep
        {
            ui.horizontal(|ui|
            {
                ui.label("Up:");
                changed = ui.add(egui::DragValue::new(&mut up.x).speed(0.1).prefix("x: ")).changed() || changed;
                changed = ui.add(egui::DragValue::new(&mut up.y).speed(0.1).prefix("y: ")).changed() || changed;
                changed = ui.add(egui::DragValue::new(&mut up.z).speed(0.1).prefix("z: ")).changed() || changed;
            });
        }

        ui.horizontal(|ui|
        {
            ui.label("Offset:");
            changed = ui.add(egui::DragValue::new(&mut offset.x).speed(0.1).prefix("x: ")).changed() || changed;
            changed = ui.add(egui::DragValue::new(&mut offset.y).speed(0.1).prefix("y: ")).changed() || changed;
            changed = ui.add(egui::DragValue::new(&mut offset.z).speed(0.1).prefix("z: ")).changed() || changed;
        });

        ui.separator();

        ui.collapsing("Path Points", |ui|
        {
            points_changed = ui.checkbox(&mut closed, "closed").changed() || points_changed;

            let can_delete = points.len() > 2;
            let mut add_after: Option<usize> = None;
            let mut remove_point: Option<usize> = None;

            for (i, point) in points.iter_mut().enumerate()
            {
                ui.push_id(i, |ui|
                {
                    ui.horizontal(|ui|
                    {
                        ui.label(format!("Point {}", i));

                        if ui.button("➕").on_hover_text("insert point after this one").clicked()
                        {
                            add_after = Some(i);
                        }

                        if can_delete && ui.button("🗑").on_hover_text("delete point").clicked()
                        {
                            remove_point = Some(i);
                        }
                    });

                    ui.horizontal(|ui|
                    {
                        ui.label("Pos:");
                        points_changed = ui.add(egui::DragValue::new(&mut point.pos.x).speed(0.1).prefix("x: ")).changed() || points_changed;
                        points_changed = ui.add(egui::DragValue::new(&mut point.pos.y).speed(0.1).prefix("y: ")).changed() || points_changed;
                        points_changed = ui.add(egui::DragValue::new(&mut point.pos.z).speed(0.1).prefix("z: ")).changed() || points_changed;
                    });

                    ui.horizontal(|ui|
                    {
                        ui.label("In:");
                        points_changed = ui.add(egui::DragValue::new(&mut point.handle_in.x).speed(0.1).prefix("x: ")).changed() || points_changed;
                        points_changed = ui.add(egui::DragValue::new(&mut point.handle_in.y).speed(0.1).prefix("y: ")).changed() || points_changed;
                        points_changed = ui.add(egui::DragValue::new(&mut point.handle_in.z).speed(0.1).prefix("z: ")).changed() || points_changed;
                    });

                    ui.horizontal(|ui|
                    {
                        ui.label("Out:");
                        points_changed = ui.add(egui::DragValue::new(&mut point.handle_out.x).speed(0.1).prefix("x: ")).changed() || points_changed;
                        points_changed = ui.add(egui::DragValue::new(&mut point.handle_out.y).speed(0.1).prefix("y: ")).changed() || points_changed;
                        points_changed = ui.add(egui::DragValue::new(&mut point.handle_out.z).speed(0.1).prefix("z: ")).changed() || points_changed;
                    });

                    ui.separator();
                });
            }

            if let Some(i) = add_after
            {
                let next = (i + 1) % points.len();
                let mut new_point = points[i];

                if next != i
                {
                    // insert between the current and the next point
                    new_point.pos = Point3::<f32>::from((points[i].pos.coords + points[next].pos.coords) * 0.5);
                }
                else
                {
                    new_point.pos += Vector3::<f32>::x();
                }

                points.insert(i + 1, new_point);
                points_changed = true;
            }

            if let Some(i) = remove_point
            {
                points.remove(i);
                points_changed = true;
            }

            if ui.button("Reset to default circle").clicked()
            {
                reset_circle = true;
                points_changed = true;
            }
        });

        if changed || progress_changed || points_changed
        {
            let data = self.data.get_mut();

            data.playing = playing;
            data.speed = speed;
            data.space = space;
            data.loop_mode = loop_mode;
            data.orientation = orientation;
            data.easing = easing_type;
            data.look_at_pos = look_at_pos;
            data.up = up;
            data.offset = offset;

            if progress_changed
            {
                data.progress = normalized_progress * length;
                data.direction = direction;
            }

            if reset_circle
            {
                data.path = BezierPath::circle(Point3::<f32>::new(0.0, DEFAULT_CIRCLE_HEIGHT, 0.0), DEFAULT_CIRCLE_RADIUS);
            }
            else if points_changed
            {
                data.path.points = points;
                data.path.closed = closed;
            }
        }
    }
}
