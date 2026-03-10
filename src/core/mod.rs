mod backend;
mod resource;
mod cmd;
mod sync;
mod render;
mod barrier;
mod exec;


pub(crate) mod type_state_queue;

pub use backend::types;
pub use backend::{
	data_size,
	push_range,
	push_size,
	shader_stages,
	vertex_attr,
	vertex_binding,
	vertex_config,
	vertex_stride,
	Backend,
	BufferBarrierInfo2,
	CommandOps,
	DeviceOps,
	ImageBarrierInfo,
	RenderingDesc,
	SemaphoreSubmit,
};

pub use resource::{
	buf_state,
	desc_state,
	img_state,
	Binding,
	Buffer,
	DescriptorLayout,
	DescriptorPool,
	DescriptorSet,
	DescriptorSetInterface,
	Image,
	ImageView,
	Sampler,
};

pub use exec::frame::{PassBuilder, FrameGraph};
pub use exec::push_data;
pub use exec::RenderTarget;
pub use exec::{
	executor::{Executor, PresentSync},
	lane::WorkLane,
};
pub use render::cache::{BlendConfig, DepthConfig, PipelineId, PipelineManager, RasterConfig, RenderTargetConfig, VertexConfig};
pub use render::RenderingInfoBuilder;
pub use resource::SwapchainImage;
pub use sync::{BinarySemaphore, FrameSync};