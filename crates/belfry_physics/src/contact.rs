use crate::Shape;
use crate::box_contact;
use crate::mesh_contact;
use crate::segment::{closest_point_segment, closest_segment_segment};
use crate::shape::capsule_segment;
use crate::solid::Solid;
use belfry_math::Vec3;

pub(crate) const CONTACT_GAP: f32 = 0.01;
const DISTANCE_EPSILON: f32 = 1e-6;

pub(crate) struct Manifold {
    pub normal: Vec3,
    pub points: Vec<ManifoldPoint>,
    pub feature: u32,
}

pub(crate) struct ManifoldPoint {
    pub position: Vec3,
    pub separation: f32,
}

impl Manifold {
    pub(crate) fn new(normal: Vec3, mut points: Vec<(Vec3, f32)>, gap: f32) -> Option<Self> {
        let length = normal.length();
        assert!(
            length > DISTANCE_EPSILON,
            "a contact manifold carries no direction",
        );
        let normal = normal / length;
        points.retain(|(_, separation)| *separation <= gap);
        points.sort_by(|left, right| {
            let left = left.0.to_array();
            let right = right.0.to_array();
            left[0]
                .total_cmp(&right[0])
                .then(left[1].total_cmp(&right[1]))
                .then(left[2].total_cmp(&right[2]))
        });
        points.dedup_by(|left, right| (left.0 - right.0).length_squared() < 1e-8);
        points.truncate(4);
        if points.is_empty() {
            return None;
        }
        Some(Self {
            normal,
            points: points
                .into_iter()
                .map(|(position, separation)| ManifoldPoint {
                    position,
                    separation,
                })
                .collect(),
            feature: 0,
        })
    }
}

pub(crate) fn intersects(first: &Solid, second: &Solid) -> bool {
    match (&first.shape, &second.shape) {
        (Shape::Mesh { .. }, _) => mesh_contact::intersects(first, second),
        (_, Shape::Mesh { .. }) => mesh_contact::intersects(second, first),
        (Shape::Cuboid { .. }, Shape::Cuboid { .. }) => box_contact::overlap(first, second),
        (Shape::Sphere { radius: a }, Shape::Sphere { radius: b }) => {
            (first.position - second.position).length_squared() <= (a + b) * (a + b)
        }
        (Shape::Sphere { radius }, Shape::Cuboid { half_extents }) => {
            sphere_box_intersects(first, *radius, second, *half_extents)
        }
        (Shape::Cuboid { half_extents }, Shape::Sphere { radius }) => {
            sphere_box_intersects(second, *radius, first, *half_extents)
        }
        (
            Shape::Sphere { radius },
            Shape::Capsule {
                radius: capsule_radius,
                half_height,
            },
        ) => sphere_capsule_intersects(first, *radius, second, *capsule_radius, *half_height),
        (
            Shape::Capsule {
                radius: capsule_radius,
                half_height,
            },
            Shape::Sphere { radius },
        ) => sphere_capsule_intersects(second, *radius, first, *capsule_radius, *half_height),
        (Shape::Capsule { .. }, Shape::Capsule { .. }) => capsule_capsule_intersects(first, second),
        (
            Shape::Capsule {
                radius,
                half_height,
            },
            Shape::Cuboid { half_extents },
        ) => capsule_box_intersects(first, *radius, *half_height, second, *half_extents),
        (
            Shape::Cuboid { half_extents },
            Shape::Capsule {
                radius,
                half_height,
            },
        ) => capsule_box_intersects(second, *radius, *half_height, first, *half_extents),
    }
}

pub(crate) fn collide(first: &Solid, second: &Solid, gap: f32, contacts: &mut Vec<Manifold>) {
    if matches!(&first.shape, Shape::Mesh { .. }) || matches!(&second.shape, Shape::Mesh { .. }) {
        mesh_contact::collide(first, second, gap, contacts);
        return;
    }
    if let Some(manifold) = single(first, second, gap) {
        contacts.push(manifold);
    }
}

fn single(first: &Solid, second: &Solid, gap: f32) -> Option<Manifold> {
    match (&first.shape, &second.shape) {
        (Shape::Cuboid { .. }, Shape::Cuboid { .. }) => box_contact::box_box(first, second, gap),
        (Shape::Sphere { radius: a }, Shape::Sphere { radius: b }) => {
            sphere_sphere(first, *a, second, *b, gap)
        }
        (Shape::Sphere { radius }, Shape::Cuboid { half_extents }) => {
            sphere_box(first, *radius, second, *half_extents, gap, false)
        }
        (Shape::Cuboid { half_extents }, Shape::Sphere { radius }) => {
            sphere_box(second, *radius, first, *half_extents, gap, true)
        }
        (
            Shape::Sphere { radius },
            Shape::Capsule {
                radius: capsule_radius,
                half_height,
            },
        ) => sphere_capsule(
            first,
            *radius,
            second,
            *capsule_radius,
            *half_height,
            gap,
            false,
        ),
        (
            Shape::Capsule {
                radius: capsule_radius,
                half_height,
            },
            Shape::Sphere { radius },
        ) => sphere_capsule(
            second,
            *radius,
            first,
            *capsule_radius,
            *half_height,
            gap,
            true,
        ),
        (Shape::Capsule { .. }, Shape::Capsule { .. }) => capsule_capsule(first, second, gap),
        (
            Shape::Capsule {
                radius,
                half_height,
            },
            Shape::Cuboid { half_extents },
        ) => capsule_box(
            first,
            *radius,
            *half_height,
            second,
            *half_extents,
            gap,
            false,
        ),
        (
            Shape::Cuboid { half_extents },
            Shape::Capsule {
                radius,
                half_height,
            },
        ) => capsule_box(
            second,
            *radius,
            *half_height,
            first,
            *half_extents,
            gap,
            true,
        ),
        (Shape::Mesh { .. }, _) | (_, Shape::Mesh { .. }) => {
            panic!("a mesh pair dispatches its own contacts")
        }
    }
}

fn sphere_box_intersects(sphere: &Solid, radius: f32, cuboid: &Solid, half_extents: Vec3) -> bool {
    let local = cuboid.to_local(sphere.position);
    let closest = Vec3::new(
        local.x.clamp(-half_extents.x, half_extents.x),
        local.y.clamp(-half_extents.y, half_extents.y),
        local.z.clamp(-half_extents.z, half_extents.z),
    );
    (local - closest).length_squared() <= radius * radius
}

fn sphere_capsule_intersects(
    sphere: &Solid,
    radius: f32,
    capsule: &Solid,
    capsule_radius: f32,
    half_height: f32,
) -> bool {
    let segment = capsule_segment(capsule.position, capsule.rotation, half_height);
    let closest = closest_point_segment(sphere.position, segment);
    let reach = radius + capsule_radius;
    (closest - sphere.position).length_squared() <= reach * reach
}

fn capsule_capsule_intersects(first: &Solid, second: &Solid) -> bool {
    let (first_radius, first_height) = capsule_of(&first.shape);
    let (second_radius, second_height) = capsule_of(&second.shape);
    let first_segment = capsule_segment(first.position, first.rotation, first_height);
    let second_segment = capsule_segment(second.position, second.rotation, second_height);
    let (held, other) = closest_segment_segment(first_segment, second_segment);
    let reach = first_radius + second_radius;
    (held - other).length_squared() <= reach * reach
}

fn capsule_box_intersects(
    capsule: &Solid,
    radius: f32,
    half_height: f32,
    cuboid: &Solid,
    half_extents: Vec3,
) -> bool {
    let segment = capsule_segment(capsule.position, capsule.rotation, half_height);
    let local = (cuboid.to_local(segment.0), cuboid.to_local(segment.1));
    let (segment_point, box_point) =
        crate::segment::closest_segment_box(local.0, local.1 - local.0, half_extents);
    (segment_point - box_point).length_squared() <= radius * radius
}

fn sphere_sphere(
    first: &Solid,
    first_radius: f32,
    second: &Solid,
    second_radius: f32,
    gap: f32,
) -> Option<Manifold> {
    let delta = second.position - first.position;
    let distance = delta.length();
    let normal = if distance > DISTANCE_EPSILON {
        delta / distance
    } else {
        Vec3::Y
    };
    let separation = distance - first_radius - second_radius;
    let point = first.position + normal * (first_radius + separation * 0.5);
    Manifold::new(normal, vec![(point, separation)], gap)
}

fn sphere_box(
    sphere: &Solid,
    radius: f32,
    cuboid: &Solid,
    half_extents: Vec3,
    gap: f32,
    flip: bool,
) -> Option<Manifold> {
    let local = cuboid.to_local(sphere.position);
    let closest = Vec3::new(
        local.x.clamp(-half_extents.x, half_extents.x),
        local.y.clamp(-half_extents.y, half_extents.y),
        local.z.clamp(-half_extents.z, half_extents.z),
    );
    let (normal, separation, point) = if closest == local {
        let (distance, axis) = nearest_face(local, half_extents);
        let face = cuboid.rotation.rotate_vec3(axis);
        let normal = -face;
        let separation = distance - radius;
        let point = sphere.position + face * ((radius + distance) * 0.5);
        (normal, separation, point)
    } else {
        let delta = closest - local;
        let distance = delta.length();
        let normal = cuboid.rotation.rotate_vec3(delta / distance);
        let separation = distance - radius;
        let point = sphere.position + normal * (radius + separation * 0.5);
        (normal, separation, point)
    };
    Manifold::new(
        if flip { -normal } else { normal },
        vec![(point, separation)],
        gap,
    )
}

fn sphere_capsule(
    sphere: &Solid,
    radius: f32,
    capsule: &Solid,
    capsule_radius: f32,
    half_height: f32,
    gap: f32,
    flip: bool,
) -> Option<Manifold> {
    let (start, end) = capsule_segment(capsule.position, capsule.rotation, half_height);
    let closest = closest_point_segment(sphere.position, (start, end));
    let delta = closest - sphere.position;
    let distance = delta.length();
    let normal = if distance > DISTANCE_EPSILON {
        delta / distance
    } else {
        capsule.rotation.rotate_vec3(Vec3::Y)
    };
    let separation = distance - radius - capsule_radius;
    let point = sphere.position + normal * (radius + separation * 0.5);
    Manifold::new(
        if flip { -normal } else { normal },
        vec![(point, separation)],
        gap,
    )
}

fn capsule_capsule(first: &Solid, second: &Solid, gap: f32) -> Option<Manifold> {
    let (first_radius, first_height) = capsule_of(&first.shape);
    let (second_radius, second_height) = capsule_of(&second.shape);
    let first_segment = capsule_segment(first.position, first.rotation, first_height);
    let second_segment = capsule_segment(second.position, second.rotation, second_height);
    let (first_point, second_point) = closest_segment_segment(first_segment, second_segment);
    let delta = second_point - first_point;
    let distance = delta.length();
    let first_direction = first_segment.1 - first_segment.0;
    let second_direction = second_segment.1 - second_segment.0;
    let normal = if distance > DISTANCE_EPSILON {
        delta / distance
    } else {
        let cross = first_direction.cross(second_direction);
        if cross.length_squared() > 1e-12 {
            cross.normalize()
        } else if first_direction.length_squared() > 1e-12 {
            perpendicular(first_direction)
        } else {
            Vec3::Y
        }
    };
    let separation = distance - first_radius - second_radius;
    if separation > gap {
        return None;
    }
    let point = first_point + normal * (first_radius + separation * 0.5);
    let mut points = vec![(point, separation)];
    let parallel = first_direction.length_squared() > 1e-12
        && second_direction.length_squared() > 1e-12
        && first_direction.dot(second_direction).abs()
            > first_direction.length() * second_direction.length() * 0.999;
    if parallel {
        for endpoint in [first_segment.0, first_segment.1] {
            let closest = closest_point_segment(endpoint, second_segment);
            let held = (closest - endpoint).length() - first_radius - second_radius;
            if held <= gap {
                points.push((endpoint + normal * (first_radius + held * 0.5), held));
            }
        }
        for endpoint in [second_segment.0, second_segment.1] {
            let closest = closest_point_segment(endpoint, first_segment);
            let held = (closest - endpoint).length() - first_radius - second_radius;
            if held <= gap {
                points.push((closest + normal * (first_radius + held * 0.5), held));
            }
        }
        if points.len() > 2 {
            let axis = first_direction.normalize();
            points.sort_by(|left, right| {
                (left.0 - first_segment.0)
                    .dot(axis)
                    .total_cmp(&(right.0 - first_segment.0).dot(axis))
            });
            let low = points[0];
            let high = *points.last().expect("a manifold holds a point");
            points = vec![low, high];
        }
    }
    Manifold::new(normal, points, gap)
}

fn capsule_box(
    capsule: &Solid,
    radius: f32,
    half_height: f32,
    cuboid: &Solid,
    half_extents: Vec3,
    gap: f32,
    flip: bool,
) -> Option<Manifold> {
    let segment = capsule_segment(capsule.position, capsule.rotation, half_height);
    let local = (cuboid.to_local(segment.0), cuboid.to_local(segment.1));
    let (segment_point, box_point) =
        crate::segment::closest_segment_box(local.0, local.1 - local.0, half_extents);
    let distance = (segment_point - box_point).length();
    let (normal, point, separation) = if distance > DISTANCE_EPSILON {
        let toward_box = cuboid
            .rotation
            .rotate_vec3((box_point - segment_point) / distance);
        let world = cuboid.to_world(segment_point);
        (
            toward_box,
            world + toward_box * (radius + (distance - radius) * 0.5),
            distance - radius,
        )
    } else {
        let (held, axis) = nearest_face(segment_point, half_extents);
        let face = cuboid.rotation.rotate_vec3(axis);
        let normal = -face;
        let world = cuboid.to_world(segment_point);
        (
            normal,
            world + face * ((radius + held) * 0.5),
            held - radius,
        )
    };
    if separation > gap {
        return None;
    }
    let mut points = vec![(point, separation)];
    for endpoint in [segment.0, segment.1] {
        let local = cuboid.to_local(endpoint);
        let closest = Vec3::new(
            local.x.clamp(-half_extents.x, half_extents.x),
            local.y.clamp(-half_extents.y, half_extents.y),
            local.z.clamp(-half_extents.z, half_extents.z),
        );
        let held = if closest == local {
            nearest_face(local, half_extents).0 - radius
        } else {
            (closest - local).length() - radius
        };
        if held <= gap {
            points.push((endpoint + normal * (radius + held * 0.5), held));
        }
    }
    let direction = segment.1 - segment.0;
    if points.len() > 2 && direction.length_squared() > 1e-12 {
        let axis = direction.normalize();
        points.sort_by(|left, right| {
            (left.0 - segment.0)
                .dot(axis)
                .total_cmp(&(right.0 - segment.0).dot(axis))
        });
        let low = points[0];
        let high = *points.last().expect("a manifold holds a point");
        points = vec![low, high];
    }
    Manifold::new(if flip { -normal } else { normal }, points, gap)
}

fn capsule_of(shape: &Shape) -> (f32, f32) {
    match shape {
        Shape::Capsule {
            radius,
            half_height,
        } => (*radius, *half_height),
        _ => panic!("a capsule collider holds a capsule"),
    }
}

fn nearest_face(point: Vec3, half_extents: Vec3) -> (f32, Vec3) {
    let distances = [
        half_extents.x - point.x.abs(),
        half_extents.y - point.y.abs(),
        half_extents.z - point.z.abs(),
    ];
    let axis = if distances[0] <= distances[1] && distances[0] <= distances[2] {
        0
    } else if distances[1] <= distances[2] {
        1
    } else {
        2
    };
    let direction = match axis {
        0 => Vec3::new(point.x.signum(), 0.0, 0.0),
        1 => Vec3::new(0.0, point.y.signum(), 0.0),
        _ => Vec3::new(0.0, 0.0, point.z.signum()),
    };
    (distances[axis], direction)
}

fn perpendicular(direction: Vec3) -> Vec3 {
    let reference = if direction.x.abs() < 0.9 {
        Vec3::X
    } else {
        Vec3::Y
    };
    direction.cross(reference).normalize()
}
