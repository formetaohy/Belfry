use crate::material::Material;
use crate::mesh::Mesh;
use belfry_math::Vec3;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeshInstance {
    pub mesh: Mesh,
    pub material: Material,
    pub tint: Vec3,
}
