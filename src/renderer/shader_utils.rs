use std::fmt::Debug;
use glex_shader_macro::vertex_input;
use crate::renderer::prelude::*;


pub fn build_graphics_pipeline<B: Backend>(
	pipelines: &mut PipelineManager<B>,
	device: &B::Device,
	vert_spv: &[u8],
	frag_spv: &[u8],
	desc_layouts: &[B::DescriptorSetLayout],
	push_ranges: &[PushConstantRange],
	vertex: VertexConfig,
	blend: BlendConfig,
	color_format: Format,
) -> Result<PipelineId, B::Error>
	where
		B::Device: DeviceOps<B>, <B as Backend>::Pipeline: Debug
{
	let vert = device.create_shader_module(vert_spv)?;
	let frag = device.create_shader_module(frag_spv)?;
	
	let shaders = ShaderConfig {
		vert,
		frag,
		entry: c"main",
	};
	
	let slot = pipelines.create_graphics(
		desc_layouts,
		push_ranges,
		|layout| GraphicsPipelineDesc {
			shaders,
			layout,
			vertex,
			raster: RasterConfig::default(),
			depth: DepthConfig::disabled(),
			blend,
			target: RenderTargetConfig { color_format },
		},
	)?;
	
	device.destroy_shader_module(vert);
	device.destroy_shader_module(frag);
	
	Ok(pipelines.push(slot))
}


pub fn build_compute_pipeline<B: Backend>(
	pipelines: &mut PipelineManager<B>,
	device: &B::Device,
	comp_spv: &[u8],
	desc_layouts: &[B::DescriptorSetLayout],
	push_ranges: &[PushConstantRange],
) -> Result<PipelineId, B::Error>
	where
		B::Device: DeviceOps<B>, <B as Backend>::Pipeline: Debug
{
	let comp_module = device.create_shader_module(comp_spv)?;
	let slot = pipelines.create_compute(
		desc_layouts,
		push_ranges,
		comp_module,
	);
	
	// 3. Clean up the module (it's baked into the pipeline/slot now)
	device.destroy_shader_module(comp_module);
	
	// 4. Register with the manager and return the ID
	Ok(pipelines.push(slot?))
}
// ── Vertex data for CSD ─────────────────────────────────────────────

pub const UNIT_QUAD: [Vertex2D; 6] = [
	Vertex2D { input_position: [0.0, 0.0], input_texture: [0.0, 0.0] },
	Vertex2D { input_position: [1.0, 0.0], input_texture: [1.0, 0.0] },
	Vertex2D { input_position: [1.0, 1.0], input_texture: [1.0, 1.0] },
	Vertex2D { input_position: [0.0, 0.0], input_texture: [0.0, 0.0] },
	Vertex2D { input_position: [1.0, 1.0], input_texture: [1.0, 1.0] },
	Vertex2D { input_position: [0.0, 1.0], input_texture: [0.0, 1.0] },
];

vertex_layout!(
    Vertex2D,
    binding = 0,
    rate = VertexInputRate::VERTEX,
    attrs = [
        0 => (Format::R32G32_SFLOAT, input_position),
        1 => (Format::R32G32_SFLOAT, input_texture),
    ],
    topology = PrimitiveTopology::TRIANGLE_LIST
);

#[vertex_input]
#[repr(C)]
#[derive(Copy, Clone)]
pub struct Vertex2D {
	pub input_position: [f32; 2],
	pub input_texture: [f32; 2],
}