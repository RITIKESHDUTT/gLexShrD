use crate::renderer::prelude::*;
use glex_shader_macro::push_constant;
use glex_shader_macro::shader_binding;

// ── Push constant structs ────────────────────────────────────
#[push_constant]
#[repr(C)]
#[derive(Copy, Clone)]
pub struct ComputePush {
	pub dt: f32,
	pub frame: u32,
	pub count: u32,
}

#[repr(C, align(8))]
#[derive(Copy, Clone)]
#[push_constant]
pub struct GfxPush {
	pub viewport_offset: [f32; 2],  // 0
	pub viewport_extent: [f32; 2],  // 8
	pub surface_extent:  [f32; 2],  // 16
	pub frame: u32,                 // 24
	pub time: f32,                // 28 → forces 32 total
}

binding!(
	StorageRead,
	index = 0,
	set = 0,
	type = DescriptorType::StorageBuffer,
	stages = ShaderStages::COMPUTE
);

binding!(
	StorageWrite,
	index = 1, set = 0,
	type = DescriptorType::StorageBuffer,
	stages = ShaderStages::COMPUTE
);

descriptor_set!(ComputeStorage: StorageRead, StorageWrite);

binding!(
	VertexStorage,
	index = 0,
	set = 0,
	type = DescriptorType::StorageBuffer,
	stages = ShaderStages::VERTEX
);
descriptor_set!(GfxStorage: VertexStorage);