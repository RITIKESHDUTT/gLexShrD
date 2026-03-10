use super::types::*;
use crate::core::VertexConfig;

#[macro_export]
macro_rules! vertex_offset {
    ($ty:ty, $field:ident) => {
        std::mem::offset_of!($ty, $field) as u32
    };
}
#[macro_export]
macro_rules! descriptor_set {
    ($name:ident : $($binding:ty),* $(,)?) => {
        pub struct $name;

        impl DescriptorSetInterface for $name {
              const BINDINGS: &'static [DescriptorBinding] = &[
                $(
                    DescriptorBinding {
                        binding: <$binding>::INDEX,
                        descriptor_type: <$binding>::TYPE,
                        count: <$binding>::COUNT,
                        stages: <$binding>::STAGES,
                    },
                )*
            ];
        }
    };
}


#[inline(always)]
pub const fn vertex_stride<T>() -> u32 {
    core::mem::size_of::<T>() as u32
}
#[inline(always)]
pub fn data_size<T>(data: &[T]) -> u64 {
    (data.len() * core::mem::size_of::<T>()) as u64
}

#[inline(always)]
pub const fn push_size<T>() -> u32 {
    core::mem::size_of::<T>() as u32
}

#[inline(always)]
pub const fn shader_stages(a: ShaderStages, b: ShaderStages) -> ShaderStages {
    ShaderStages(a.0 | b.0)
}


pub const fn push_range<T>(stages: ShaderStages, offset: u32) -> PushConstantRange {
    PushConstantRange {
        stages,
        offset,
        size: push_size::<T>(),
    }
}

pub const fn vertex_binding<T>(
    binding: u32,
    input_rate: VertexInputRate,
) -> VertexBindingDesc {
    VertexBindingDesc {
        binding,
        stride: vertex_stride::<T>(),
        input_rate,
    }
}

pub const fn vertex_attr(
    location: u32,
    format: Format,
    offset: u32,
) -> VertexAttributeDesc {
    VertexAttributeDesc {
        location,
        binding: 0,
        format,
        offset,
    }
}

pub const fn vertex_config(
    bindings: &'static [VertexBindingDesc],
    attributes: &'static [VertexAttributeDesc],
    topology: PrimitiveTopology,
) -> VertexConfig<'static> {
    VertexConfig { bindings, attributes, topology }
}

#[macro_export]
macro_rules! binding {
    (
        $name:ident,
        index = $index:expr,
        type  = $ty:expr,
        stages = $stages:expr
    ) => {
        pub struct $name;

        impl Binding for $name {
            const INDEX: u32 = $index;
            const TYPE: DescriptorType = $ty;
            const STAGES: ShaderStages = $stages;
        }
    };
}

#[macro_export]
macro_rules! vertex_layout {
    (
        $vertex:ty,
        binding = $binding:expr,
        rate = $rate:expr,
        attrs = [
            $( $loc:expr => ($fmt:expr, $field:ident) ),* $(,)?
        ],
        topology = $topology:expr
    ) => {
        const VERTEX_BINDINGS: &[VertexBindingDesc] = &[
            vertex_binding::<$vertex>($binding, $rate)
        ];

        const VERTEX_ATTRIBUTES: &[VertexAttributeDesc] = &[
            $(
                vertex_attr(
                    $loc,
                    $fmt,
                    vertex_offset!($vertex, $field)
                )
            ),*
        ];

        pub const VERTEX_CONFIG: VertexConfig =
            vertex_config(VERTEX_BINDINGS, VERTEX_ATTRIBUTES, $topology);
    };
}