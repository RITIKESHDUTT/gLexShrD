mod pipelines;
mod shader_utils;
mod prelude;
mod shaders;
mod windowed;

pub use pipelines::GlyphAtlas;
pub use pipelines::{CsdPipelines, RectPush, TextPush, TextSet, ParticleCPipelines, ParticleGPipelines, GFX_PUSH_RANGE, COMP_PUSH_RANGE};
pub use shader_utils::{Vertex2D, UNIT_QUAD, build_graphics_pipeline};
pub use windowed::{CsdResources, record_csd_layer};
pub use shaders::{
	PARTICLE_FRAG_SPV,
	PARTICLE_COMP_SPV,
	PARTICLE_VERT_SPV,
	VertexStorage,
	ComputeStorage,
	StorageRead,
	StorageWrite,
	GfxStorage,
	ComputePush,
	GfxPush,
};