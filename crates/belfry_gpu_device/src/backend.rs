pub use belfry_gpu::shader::Backend;

pub const PREFERENCE: &[Backend] = &[
    #[cfg(metal_backend)]
    Backend::Metal,
    #[cfg(vulkan_backend)]
    Backend::Vulkan,
];
