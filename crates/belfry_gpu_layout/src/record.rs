#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldType {
    U32,
    I32,
    F32,
    U32x4,
    F32x4,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FieldLayout {
    pub name: &'static str,
    pub offset: u32,
    pub ty: FieldType,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecordLayout {
    pub name: &'static str,
    pub size: u32,
    pub fields: &'static [FieldLayout],
}

pub trait Record {
    const LAYOUT: RecordLayout;
}
