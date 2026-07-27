#![allow(dead_code)]

use nalgebra::{Point3, Vector3};
use serde::{Deserialize, Serialize};

// handle length factor to approximate a quarter circle with a cubic bezier (4/3 * (sqrt(2) - 1))
pub const CIRCLE_HANDLE_FACTOR: f32 = 0.5522847;

const ARC_SAMPLES_PER_SEGMENT: u32 = 64;

#[derive(Serialize, Deserialize, Clone, Copy)]
pub struct PathPoint
{
    pub pos: Point3<f32>,

    // handles are relative to pos
    pub handle_in: Vector3<f32>,
    pub handle_out: Vector3<f32>,
}

impl PathPoint
{
    pub fn new(pos: Point3<f32>, handle_in: Vector3<f32>, handle_out: Vector3<f32>) -> PathPoint
    {
        PathPoint { pos, handle_in, handle_out }
    }
}

#[derive(Clone, Copy)]
struct ArcTableEntry
{
    distance: f32,
    segment: usize,
    t: f32,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct BezierPath
{
    pub points: Vec<PathPoint>,
    pub closed: bool,

    // arc length lookup table (distance -> segment + t) - needs rebuild_cache() after changing points
    #[serde(skip)]
    arc_table: Vec<ArcTableEntry>,

    #[serde(skip)]
    length: f32,
}

impl BezierPath
{
    pub fn new(points: Vec<PathPoint>, closed: bool) -> BezierPath
    {
        let mut path = BezierPath
        {
            points,
            closed,

            arc_table: vec![],
            length: 0.0,
        };

        path.rebuild_cache();

        path
    }

    // circle on the x/z plane out of 4 bezier segments
    pub fn circle(center: Point3<f32>, radius: f32) -> BezierPath
    {
        let handle = radius * CIRCLE_HANDLE_FACTOR;

        let x = Vector3::<f32>::x();
        let z = Vector3::<f32>::z();

        let points = vec!
        [
            PathPoint::new(center + (x * radius), z * -handle, z * handle),
            PathPoint::new(center + (z * radius), x * handle, x * -handle),
            PathPoint::new(center - (x * radius), z * handle, z * -handle),
            PathPoint::new(center - (z * radius), x * -handle, x * handle),
        ];

        BezierPath::new(points, true)
    }

    pub fn segment_count(&self) -> usize
    {
        let points = self.points.len();

        if points < 2
        {
            return 0;
        }

        if self.closed
        {
            points
        }
        else
        {
            points - 1
        }
    }

    pub fn total_length(&self) -> f32
    {
        self.length
    }

    pub fn has_cache(&self) -> bool
    {
        self.arc_table.len() >= 2
    }

    fn segment_control_points(&self, segment: usize) -> (Point3<f32>, Point3<f32>, Point3<f32>, Point3<f32>)
    {
        let start = &self.points[segment];
        let end = &self.points[(segment + 1) % self.points.len()];

        (start.pos, start.pos + start.handle_out, end.pos + end.handle_in, end.pos)
    }

    pub fn position(&self, segment: usize, t: f32) -> Point3<f32>
    {
        let (p0, c0, c1, p1) = self.segment_control_points(segment);

        let u = 1.0 - t;

        Point3::<f32>::from
        (
            (p0.coords * (u * u * u)) +
            (c0.coords * (3.0 * u * u * t)) +
            (c1.coords * (3.0 * u * t * t)) +
            (p1.coords * (t * t * t))
        )
    }

    pub fn tangent(&self, segment: usize, t: f32) -> Vector3<f32>
    {
        let (p0, c0, c1, p1) = self.segment_control_points(segment);

        let u = 1.0 - t;

        ((c0 - p0) * (3.0 * u * u)) +
        ((c1 - c0) * (6.0 * u * t)) +
        ((p1 - c1) * (3.0 * t * t))
    }

    pub fn rebuild_cache(&mut self)
    {
        self.arc_table.clear();
        self.length = 0.0;

        let segments = self.segment_count();

        if segments == 0
        {
            return;
        }

        self.arc_table.reserve((segments * ARC_SAMPLES_PER_SEGMENT as usize) + segments + 1);
        self.arc_table.push(ArcTableEntry { distance: 0.0, segment: 0, t: 0.0 });

        let mut prev = self.position(0, 0.0);

        for segment in 0..segments
        {
            if segment > 0
            {
                self.arc_table.push(ArcTableEntry { distance: self.length, segment, t: 0.0 });
            }

            for step in 1..=ARC_SAMPLES_PER_SEGMENT
            {
                let t = step as f32 / ARC_SAMPLES_PER_SEGMENT as f32;
                let pos = self.position(segment, t);

                self.length += (pos - prev).norm();
                self.arc_table.push(ArcTableEntry { distance: self.length, segment, t });

                prev = pos;
            }
        }
    }

    // maps a travelled distance to (segment, t) via the arc length table
    pub fn segment_and_t_at_distance(&self, distance: f32) -> Option<(usize, f32)>
    {
        if !self.has_cache() || self.length <= 0.0
        {
            return None;
        }

        let distance = distance.clamp(0.0, self.length);

        let index = self.arc_table.partition_point(|entry| entry.distance < distance).clamp(1, self.arc_table.len() - 1);

        let a = &self.arc_table[index - 1];
        let b = &self.arc_table[index];

        // entries of a segment boundary share the same distance
        if a.segment != b.segment
        {
            return Some((b.segment, b.t));
        }

        let span = b.distance - a.distance;
        let factor = if span > 0.0 { (distance - a.distance) / span } else { 0.0 };

        Some((a.segment, a.t + ((b.t - a.t) * factor)))
    }

    pub fn sample_at_distance(&self, distance: f32) -> Option<(Point3<f32>, Vector3<f32>)>
    {
        let (segment, t) = self.segment_and_t_at_distance(distance)?;

        Some((self.position(segment, t), self.tangent(segment, t)))
    }
}

#[cfg(test)]
mod tests
{
    use super::*;

    #[test]
    fn circle_length()
    {
        let radius = 5.0;
        let path = BezierPath::circle(Point3::<f32>::new(0.0, 0.0, 0.0), radius);

        let expected = 2.0 * std::f32::consts::PI * radius;

        assert!((path.total_length() - expected).abs() < 0.1, "length {} vs expected {}", path.total_length(), expected);
    }

    #[test]
    fn constant_speed_sampling()
    {
        let path = BezierPath::circle(Point3::<f32>::new(1.0, 2.0, 3.0), 4.0);

        let steps = 100;
        let step_length = path.total_length() / steps as f32;

        let mut prev = path.sample_at_distance(0.0).unwrap().0;

        for step in 1..=steps
        {
            let (pos, _) = path.sample_at_distance(step_length * step as f32).unwrap();
            let dist = (pos - prev).norm();

            assert!((dist - step_length).abs() < step_length * 0.05, "step {} moved {} instead of {}", step, dist, step_length);

            prev = pos;
        }
    }

    #[test]
    fn straight_line_lookup()
    {
        let points = vec!
        [
            PathPoint::new(Point3::<f32>::new(0.0, 0.0, 0.0), Vector3::<f32>::zeros(), Vector3::<f32>::zeros()),
            PathPoint::new(Point3::<f32>::new(10.0, 0.0, 0.0), Vector3::<f32>::zeros(), Vector3::<f32>::zeros()),
        ];

        let path = BezierPath::new(points, false);

        assert!((path.total_length() - 10.0).abs() < 0.01);

        // constant speed lookup has to counteract the non linear bezier parameterization
        let (pos, _) = path.sample_at_distance(5.0).unwrap();
        assert!((pos.x - 5.0).abs() < 0.01, "pos.x = {}", pos.x);

        let (pos, _) = path.sample_at_distance(2.5).unwrap();
        assert!((pos.x - 2.5).abs() < 0.02, "pos.x = {}", pos.x);
    }

    #[test]
    fn empty_path()
    {
        let path = BezierPath::new(vec![], false);

        assert_eq!(path.segment_count(), 0);
        assert!(path.sample_at_distance(1.0).is_none());
    }
}
