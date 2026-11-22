use crate::Shape;
use crate::contact::Manifold;
use crate::segment::closest_segment_segment;
use crate::solid::Solid;
use belfry_math::Vec3;

const AXES: [Vec3; 3] = [Vec3::X, Vec3::Y, Vec3::Z];
const EDGE_BIAS: f32 = 1.05;

#[derive(Clone, Copy)]
enum Kind {
    FaceFirst(usize),
    FaceSecond(usize),
    Edge(usize, usize),
}

#[derive(Clone, Copy)]
struct Axis {
    normal: Vec3,
    overlap: f32,
    kind: Kind,
    score: f32,
}

pub(crate) fn box_box(first: &Solid, second: &Solid, gap: f32) -> Option<Manifold> {
    let best = best_axis(first, second)?;
    if -best.overlap > gap {
        return None;
    }
    let first_he = half_extents(&first.shape);
    let second_he = half_extents(&second.shape);
    let normal = first.rotation.rotate_vec3(best.normal);
    let points = match best.kind {
        Kind::FaceFirst(axis) => {
            face_points(first, first_he, axis, best.normal, second, second_he, gap)
        }
        Kind::FaceSecond(axis) => face_points(
            second,
            second_he,
            axis,
            second.rotation.inverse().rotate_vec3(-normal),
            first,
            first_he,
            gap,
        ),
        Kind::Edge(first_axis, second_axis) => {
            let first_edge = support_edge(first, first_he, first_axis, best.normal);
            let second_edge = support_edge(second, second_he, second_axis, -normal);
            let (held, other) = closest_segment_segment(first_edge, second_edge);
            vec![((held + other) * 0.5, -best.overlap)]
        }
    };
    Manifold::new(normal, points, gap)
}

pub(crate) fn overlap(first: &Solid, second: &Solid) -> bool {
    best_axis(first, second).is_some_and(|axis| axis.overlap >= 0.0)
}

fn best_axis(first: &Solid, second: &Solid) -> Option<Axis> {
    let first_he = half_extents(&first.shape);
    let second_he = half_extents(&second.shape);
    let relative = first.rotation.inverse() * second.rotation;
    let second_axes = [
        relative.rotate_vec3(Vec3::X),
        relative.rotate_vec3(Vec3::Y),
        relative.rotate_vec3(Vec3::Z),
    ];
    let translation = first
        .rotation
        .inverse()
        .rotate_vec3(second.position - first.position);
    let mut axes = [Axis {
        normal: Vec3::ZERO,
        overlap: 0.0,
        kind: Kind::FaceFirst(0),
        score: 0.0,
    }; 15];
    let mut count = 0;
    for (axis, &direction) in AXES.iter().enumerate() {
        let distance = translation.dot(direction);
        let overlap = first_he.to_array()[axis] + projected(second_he, second_axes, direction)
            - distance.abs();
        axes[count] = Axis {
            normal: direction * sign(distance),
            overlap,
            kind: Kind::FaceFirst(axis),
            score: overlap,
        };
        count += 1;
    }
    for (axis, &direction) in second_axes.iter().enumerate() {
        let distance = translation.dot(direction);
        let overlap =
            projected(first_he, AXES, direction) + second_he.to_array()[axis] - distance.abs();
        axes[count] = Axis {
            normal: direction * sign(distance),
            overlap,
            kind: Kind::FaceSecond(axis),
            score: overlap,
        };
        count += 1;
    }
    for (first_axis, &first_direction) in AXES.iter().enumerate() {
        for (second_axis, &second_direction) in second_axes.iter().enumerate() {
            let direction = first_direction.cross(second_direction);
            let length_squared = direction.length_squared();
            if length_squared < 1e-8 {
                continue;
            }
            let direction = direction / length_squared.sqrt();
            if axes[..count]
                .iter()
                .any(|axis| axis.normal.cross(direction).length_squared() < 1e-8)
            {
                continue;
            }
            let distance = translation.dot(direction);
            let overlap = projected(first_he, AXES, direction)
                + projected(second_he, second_axes, direction)
                - distance.abs();
            axes[count] = Axis {
                normal: direction * sign(distance),
                overlap,
                kind: Kind::Edge(first_axis, second_axis),
                score: overlap * EDGE_BIAS,
            };
            count += 1;
        }
    }
    let separated = axes[..count].iter().any(|axis| axis.overlap < 0.0);
    let mut best: Option<Axis> = None;
    for axis in &axes[..count] {
        if (axis.overlap < 0.0) != separated {
            continue;
        }
        let replace = best.is_none_or(|current| {
            if separated {
                axis.overlap > current.overlap
            } else {
                axis.score < current.score
            }
        });
        if replace {
            best = Some(*axis);
        }
    }
    best
}

fn sign(value: f32) -> f32 {
    if value < 0.0 { -1.0 } else { 1.0 }
}

fn projected(half_extents: Vec3, axes: [Vec3; 3], direction: Vec3) -> f32 {
    half_extents.x * direction.dot(axes[0]).abs()
        + half_extents.y * direction.dot(axes[1]).abs()
        + half_extents.z * direction.dot(axes[2]).abs()
}

fn half_extents(shape: &Shape) -> Vec3 {
    match shape {
        Shape::Cuboid { half_extents } => *half_extents,
        _ => panic!("a cuboid collider holds a cuboid"),
    }
}

fn face_points(
    reference: &Solid,
    reference_half_extents: Vec3,
    reference_axis: usize,
    reference_normal: Vec3,
    incident: &Solid,
    incident_half_extents: Vec3,
    gap: f32,
) -> Vec<(Vec3, f32)> {
    let incident_normal = -reference.rotation.rotate_vec3(reference_normal);
    let mut incident_axis = 0;
    let mut incident_dot: f32 = 0.0;
    for (axis, &held) in AXES.iter().enumerate() {
        let dot = incident.rotation.rotate_vec3(held).dot(incident_normal);
        if dot.abs() > incident_dot.abs() {
            incident_dot = dot;
            incident_axis = axis;
        }
    }
    let incident_sign = if incident_dot >= 0.0 { 1.0 } else { -1.0 };
    let (first_side, second_side) = match incident_axis {
        0 => (1, 2),
        1 => (0, 2),
        _ => (0, 1),
    };
    let extent = incident_half_extents.to_array();
    let corners = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)];
    let mut polygon = corners
        .iter()
        .map(|(first_sign, second_sign)| {
            let mut local = [0.0; 3];
            local[incident_axis] = incident_sign * extent[incident_axis];
            local[first_side] = first_sign * extent[first_side];
            local[second_side] = second_sign * extent[second_side];
            let world = incident.position + incident.rotation.rotate_vec3(Vec3::from_array(local));
            reference
                .rotation
                .inverse()
                .rotate_vec3(world - reference.position)
        })
        .collect::<Vec<_>>();
    for axis in 0..3 {
        if axis == reference_axis {
            continue;
        }
        let bound = reference_half_extents.to_array()[axis];
        polygon = clip(polygon, axis, -bound, true);
        polygon = clip(polygon, axis, bound, false);
        if polygon.is_empty() {
            break;
        }
    }
    let mut points = polygon
        .into_iter()
        .map(|point| {
            let separation =
                reference_normal.dot(point) - reference_half_extents.to_array()[reference_axis];
            (
                reference.position + reference.rotation.rotate_vec3(point),
                separation,
            )
        })
        .filter(|(_, separation)| *separation <= gap)
        .collect::<Vec<_>>();
    if points.is_empty() {
        let support = support_vertex(incident, incident_half_extents, incident_normal);
        let local = reference
            .rotation
            .inverse()
            .rotate_vec3(support - reference.position);
        let separation =
            reference_normal.dot(local) - reference_half_extents.to_array()[reference_axis];
        if separation <= gap {
            points.push((support, separation));
        }
    }
    points
}

fn clip(polygon: Vec<Vec3>, axis: usize, bound: f32, greater: bool) -> Vec<Vec3> {
    let mut clipped = Vec::with_capacity(polygon.len() + 1);
    for index in 0..polygon.len() {
        let current = polygon[index];
        let next = polygon[(index + 1) % polygon.len()];
        let current_value = current.to_array()[axis] - bound;
        let next_value = next.to_array()[axis] - bound;
        let current_inside = if greater {
            current_value >= 0.0
        } else {
            current_value <= 0.0
        };
        let next_inside = if greater {
            next_value >= 0.0
        } else {
            next_value <= 0.0
        };
        if current_inside {
            clipped.push(current);
        }
        if current_inside != next_inside {
            let phase = current_value / (current_value - next_value);
            clipped.push(current.lerp(next, phase));
        }
    }
    clipped
}

fn support_vertex(incident: &Solid, half_extents: Vec3, direction: Vec3) -> Vec3 {
    let mut local = Vec3::ZERO;
    for (axis, &held) in AXES.iter().enumerate() {
        let sign = if incident.rotation.rotate_vec3(held).dot(direction) >= 0.0 {
            1.0
        } else {
            -1.0
        };
        local += held * (half_extents.to_array()[axis] * sign);
    }
    incident.position + incident.rotation.rotate_vec3(local)
}

fn support_edge(solid: &Solid, half_extents: Vec3, axis: usize, direction: Vec3) -> (Vec3, Vec3) {
    let mut center = solid.position;
    for (other, &held) in AXES.iter().enumerate() {
        if other == axis {
            continue;
        }
        let world = solid.rotation.rotate_vec3(held);
        let sign = if world.dot(direction) >= 0.0 {
            1.0
        } else {
            -1.0
        };
        center += world * (half_extents.to_array()[other] * sign);
    }
    let direction = solid.rotation.rotate_vec3(AXES[axis]);
    let extent = half_extents.to_array()[axis];
    (center - direction * extent, center + direction * extent)
}
