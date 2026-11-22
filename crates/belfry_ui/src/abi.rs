use belfry_gpu_compiler::Fvec4;
use belfry_gpu_layout::{FieldLayout, FieldType, Record, RecordLayout};

macro_rules! record {
    ($record:ident, $layout:ident, $name:literal {
        $($field:ident: $ty:ty => $kind:ident),+ $(,)?
    }) => {
        #[repr(C)]
        #[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
        pub struct $record {
            $(pub $field: $ty,)+
        }

        pub const $layout: RecordLayout = RecordLayout {
            name: $name,
            size: std::mem::size_of::<$record>() as u32,
            fields: &[$(FieldLayout {
                name: stringify!($field),
                offset: std::mem::offset_of!($record, $field) as u32,
                ty: FieldType::$kind,
            },)+],
        };

        impl Record for $record {
            const LAYOUT: RecordLayout = $layout;
        }
    };
}

record!(UiInstanceRecord, UI_INSTANCE, "UiInstance" {
    rect: Fvec4 => F32x4,
    corners: Fvec4 => F32x4,
    color: Fvec4 => F32x4,
    color2: Fvec4 => F32x4,
    uv: Fvec4 => F32x4,
    clip: Fvec4 => F32x4,
    clip_corners: Fvec4 => F32x4,
    meta: belfry_gpu_compiler::Uvec4 => U32x4,
    extra: Fvec4 => F32x4,
});

record!(UiScreenRecord, UI_SCREEN, "UiScreen" {
    width: f32 => F32,
    height: f32 => F32,
    scale: f32 => F32,
    vertical: f32 => F32,
});
