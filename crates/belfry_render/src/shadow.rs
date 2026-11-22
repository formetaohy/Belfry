use crate::camera::Camera;
use crate::light::DirectionalLight;
use belfry_gpu_device::ClipSpace;
use belfry_math::{Mat4, Transform, Vec2, Vec3};
use belfry_render_abi::{SHADOW_ATLAS, SHADOW_ATLAS_COLUMNS, SHADOW_CASCADES, SHADOW_SIDE};

const SPLIT_LAMBDA: f32 = 0.75;
const DEPTH_MARGIN: f32 = 0.02;
const REFERENCE_UP_LIMIT: f32 = 0.9;

#[derive(Clone, Copy, Debug)]
pub(crate) struct Cascade {
    pub(crate) clip: Mat4,
    pub(crate) split: f32,
    pub(crate) texel: f32,
    pub(crate) depth_span: f32,
    pub(crate) atlas_origin: Vec2,
    pub(crate) atlas_scale: f32,
}

pub(crate) fn cascades(
    camera: Camera,
    transform: Transform,
    aspect: f32,
    light: DirectionalLight,
    light_transform: Transform,
    spheres: &[(Vec3, f32)],
    clip_space: ClipSpace,
) -> [Cascade; SHADOW_CASCADES as usize] {
    assert!(
        light.shadow_distance.is_finite() && light.shadow_distance > camera.near,
        "a light whose shadow reaches {} cannot cover a camera near plane of {}",
        light.shadow_distance,
        camera.near,
    );
    let reach = camera.far.min(light.shadow_distance);
    let eye = transform.translation;
    let forward = transform.rotation.rotate_vec3(-Vec3::Z).normalize();
    let travel = light_transform.rotation.rotate_vec3(-Vec3::Z).normalize();
    let reference = if travel.y.abs() < REFERENCE_UP_LIMIT {
        Vec3::Y
    } else {
        Vec3::Z
    };
    let right = travel.cross(reference).normalize();
    let up = right.cross(travel);
    let mut splits = [0.0f32; SHADOW_CASCADES as usize + 1];
    splits[0] = camera.near;
    for (index, split) in splits.iter_mut().enumerate().skip(1) {
        let ratio = index as f32 / SHADOW_CASCADES as f32;
        let logarithmic = camera.near * (reach / camera.near).powf(ratio);
        let uniform = camera.near + (reach - camera.near) * ratio;
        *split = SPLIT_LAMBDA * logarithmic + (1.0 - SPLIT_LAMBDA) * uniform;
    }
    let tan_vertical = (camera.vertical_fov * 0.5).tan();
    let tan_horizontal = tan_vertical * aspect;
    let corner = (tan_vertical * tan_vertical + tan_horizontal * tan_horizontal).sqrt();
    let scale = SHADOW_SIDE as f32 / SHADOW_ATLAS as f32;
    std::array::from_fn(|index| {
        let near = splits[index];
        let far = splits[index + 1];
        let near_radius = corner * near;
        let far_radius = corner * far;
        let center_depth = (near + far) * 0.5
            + (far_radius * far_radius - near_radius * near_radius) / (2.0 * (far - near));
        let radius = ((center_depth - near).powi(2) + near_radius * near_radius).sqrt();
        assert!(
            radius > 0.0 && radius.is_finite(),
            "cascade {index} of {near} to {far} bounds no sphere",
        );
        let texel = 2.0 * radius / (SHADOW_SIDE as f32 - 2.0);
        let extent = radius + texel;
        let center = eye + forward * center_depth;
        let held_x = right.dot(center);
        let held_y = up.dot(center);
        let snapped_x = (held_x / texel).round() * texel;
        let snapped_y = (held_y / texel).round() * texel;
        let mut min_depth = f32::INFINITY;
        let mut max_depth = f32::NEG_INFINITY;
        for (sphere, sphere_radius) in spheres {
            let x = right.dot(*sphere);
            let y = up.dot(*sphere);
            let depth = travel.dot(*sphere);
            if x + sphere_radius >= snapped_x - extent
                && x - sphere_radius <= snapped_x + extent
                && y + sphere_radius >= snapped_y - extent
                && y - sphere_radius <= snapped_y + extent
            {
                min_depth = min_depth.min(depth - sphere_radius);
                max_depth = max_depth.max(depth + sphere_radius);
            }
        }
        if min_depth > max_depth {
            min_depth = travel.dot(center) - extent;
            max_depth = travel.dot(center) + extent;
        }
        let margin = ((max_depth - min_depth) * DEPTH_MARGIN).max(texel);
        let min_depth = min_depth - margin;
        let max_depth = max_depth + margin;
        let span = max_depth - min_depth;
        assert!(
            span > 0.0 && span.is_finite(),
            "cascade {index} spans {span}"
        );
        let eye = center
            - right * (held_x - snapped_x)
            - up * (held_y - snapped_y)
            - travel * (travel.dot(center) - min_depth);
        let view = Mat4::look_at(eye, eye + travel, up);
        let projection = Mat4::orthographic(-extent, extent, -extent, extent, 0.0, span);
        let clip = projection * view;
        let clip = match clip_space {
            ClipSpace::YUp => clip,
            ClipSpace::YDown => clip.flip_clip_y(),
        };
        let column = index as u32 % SHADOW_ATLAS_COLUMNS;
        let row = index as u32 / SHADOW_ATLAS_COLUMNS;
        Cascade {
            clip,
            split: far,
            texel,
            depth_span: span,
            atlas_origin: Vec2::new(column as f32 * scale, row as f32 * scale),
            atlas_scale: scale,
        }
    })
}
