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
	push_size,
	shader_stages,
	vertex_stride,
	push_range,
	vertex_binding,
	vertex_attr,
	vertex_config,
	data_size,
	Backend,
	BufferBarrierInfo2,
	CommandOps,
	DeviceOps,
	ImageBarrierInfo,
	RenderingDesc,
	SemaphoreSubmit,
	
};

pub use resource::{
	Binding,
	DescriptorLayout,
	DescriptorPool,
	DescriptorSetInterface,
	ImageView,
	Sampler,
	Image,
	DescriptorSet,
	Buffer,
	img_state,
	buf_state,
	desc_state,
};

pub use sync::{BinarySemaphore, FrameSync};
pub use exec::{
	recorder::{RenderRecorder2D, TransferRecorder, ComputeRecorder, PassRecord},
	executor::{Executor, PresentSync},
	lane::WorkLane
};
pub use render::cache::{RasterConfig, BlendConfig, DepthConfig, RenderTargetConfig, PipelineManager, VertexConfig, PipelineId};
pub use render::RenderingInfoBuilder;
pub use exec::frame::FrameGraph;
pub use resource::SwapchainImage;
pub use exec::RenderTarget;