/// Which execution domain this pass runs in.
/// This determines which queue type is required.
///
/// ## It explicitly selects:
/// which queue
///
/// ##  And Implicitly:
///
/// which command pool
///
/// which recorder view
///
/// which synchronization rules
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DescriptorSetId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PassDomain {
	Graphics,
	Compute,
	Transfer,
}
pub type ResourceId = u32;
pub type PassId = u32;

///Pipeline stage - Which shader/fixed-function unit needs the resource.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum Stage {
	None,
	Top,
	DrawIndirect,
	VertexInput,
	Vertex,
	Fragment,
	EarlyFragmentTests,
	LateFragmentTests,
	ColorOutput,
	Compute,
	Transfer,
	Host,
	Bottom,
	All,
}
///Memory access type - how the resource is read/written.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum Access {
	None,
	// Generic
	MemoryRead,
	MemoryWrite,
	
	// Shader
	UniformRead,
	SampledRead,
	StorageRead,
	StorageWrite,
	
	// Attachments
	ColorAttachmentRead,
	ColorAttachmentWrite,
	DepthStencilRead,
	DepthStencilWrite,
	
	// Transfer
	TransferRead,
	TransferWrite,
	
	// Vertex / Index / Indirect
	VertexAttributeRead,
	IndexRead,
	IndirectCommandRead,
	
	// Host
	HostRead,
	HostWrite,
}
/// Image layout — only meaningful for image resources. Buffers use Undefined.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum ImageLayout {
	Undefined,
	General,
	
	ShaderReadOnly,
	TransferSrc,
	TransferDst,
	
	ColorAttachment,
	DepthAttachment,
	DepthReadOnly,
	
	Present,
}

// What a pass intends to do with a resource.
/// Pure domain type — no Vulkan dependency.
#[derive(Debug, Clone, Copy)]
pub struct UsageIntent {
	stage: Stage,
	access: Access,
	layout: ImageLayout,
}

impl UsageIntent {
	pub fn new(stage: Stage, access: Access, layout: ImageLayout) -> Self {
		Self { stage, access, layout }
	}
	
	pub fn stage(&self) -> Stage { self.stage }
	pub fn access(&self) -> Access { self.access }
	pub fn layout(&self) -> ImageLayout { self.layout }
	
	pub fn compute_storage_read() -> Self {
		Self::new(
			Stage::Compute,
			Access::StorageRead,
			ImageLayout::General
		)
	}
	
	pub fn compute_storage_write() -> Self {
		Self::new(
			Stage::Compute,
			Access::StorageWrite,
			ImageLayout::General
		)
	}
	pub fn fragment_sampled_read() -> Self {
		Self::new(
			Stage::Fragment,
			Access::SampledRead,
			ImageLayout::ShaderReadOnly
		)
	}
	pub fn color_attachment_write() -> Self {
		Self::new(
			Stage::ColorOutput,
			Access::ColorAttachmentWrite,
			ImageLayout::ColorAttachment,
		)
	}
	pub fn depth_write() -> Self {
		Self::new(
			Stage::EarlyFragmentTests,
			Access::DepthStencilWrite,
			ImageLayout::DepthAttachment,
		)
	}
	
	pub fn depth_read() -> Self {
		Self::new(
			Stage::Fragment,
			Access::DepthStencilRead,
			ImageLayout::DepthReadOnly,
		)
	}
	
	pub fn transfer_read() -> Self {
		Self::new(
			Stage::Transfer,
			Access::TransferRead,
			ImageLayout::TransferSrc,
		)
	}
	
	pub fn transfer_write() -> Self {
		Self::new(
			Stage::Transfer,
			Access::TransferWrite,
			ImageLayout::TransferDst
		)
	}
	pub fn vertex_buffer_read() -> Self {
		Self::new(
			Stage::VertexInput,
			Access::VertexAttributeRead,
			ImageLayout::Undefined
		)
	}
	
	pub fn index_buffer_read() -> Self {
		Self::new(
			Stage::VertexInput,
			Access::IndexRead,
			ImageLayout::Undefined)
	}
	
	pub fn indirect_read() -> Self {
		Self::new(
			Stage::DrawIndirect,
			Access::IndirectCommandRead,
			ImageLayout::Undefined
		)
	}
	pub fn present() -> Self {
		Self::new(
			Stage::Bottom,
			Access::MemoryRead,
			ImageLayout::Present
		)
	}
}