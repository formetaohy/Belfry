use crate::view::View;
use belfry_gpu_device::ClipSpace;
use belfry_math::{Mat4, Transform, Vec3};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    pub vertical_fov: f32,
    pub near: f32,
    pub far: f32,
}

impl Camera {
    pub fn new(vertical_fov: f32, near: f32, far: f32) -> Self {
        let camera = Self {
            vertical_fov,
            near,
            far,
        };
        camera.check();
        camera
    }

    fn check(self) {
        assert!(
            self.near > 0.0 && self.far > self.near && self.far.is_finite(),
            "a camera of near {} and far {} spans no depth",
            self.near,
            self.far,
        );
        assert!(
            self.vertical_fov > 0.0 && self.vertical_fov < std::f32::consts::PI,
            "a camera of {0} radians sees no cone",
            self.vertical_fov,
        );
    }

    pub fn view(self, transform: Transform, aspect: f32, clip_space: ClipSpace) -> View {
        self.check();
        let eye = transform.translation;
        let forward = transform.rotation.rotate_vec3(Vec3::new(0.0, 0.0, -1.0));
        let up = transform.rotation.rotate_vec3(Vec3::Y);
        let projection = Mat4::perspective(self.vertical_fov, aspect, self.near, self.far);
        let projection = match clip_space {
            ClipSpace::YUp => projection,
            ClipSpace::YDown => projection.flip_clip_y(),
        };
        View::of(projection * Mat4::look_at(eye, eye + forward, up))
    }
}
