use crate::core::exec::{RasterConfig,DepthConfig, BlendConfig, RenderTargetConfig};
use crate::core::VertexConfig;
use std::ffi::CStr;
use std::ops::BitOr;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct Extent2D {
	width: u32,
	height: u32,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct Extent3D {
	width:u32,
	height: u32,
	depth: u32,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct Offset2D {
	x: i32,
	y: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Format(pub i32);

impl Format {
	pub const UNDEFINED: Self = Self(0);
	pub const R8_UNORM: Self = Self(9);
	pub const R8G8B8A8_UNORM: Self = Self(37);
	pub const B8G8R8A8_UNORM: Self = Self(44);
	pub const B8G8R8A8_SRGB: Self = Self(50);
	pub const R32_SFLOAT: Self = Self(100);
	pub const R32G32_SFLOAT: Self = Self(103);
	pub const R32G32B32_SFLOAT: Self = Self(106);
	pub const R32G32B32A32_SFLOAT: Self = Self(109);
	pub const D32_SFLOAT: Self = Self(126);
	pub const D24_UNORM_S8_UINT: Self = Self(129);
}

// ── Draw / Bind ─────────────────────────────────────────────

#[repr(transparent)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct IndexType(pub(crate) i32);
impl IndexType {
	#[inline] pub const fn from_raw(x: i32) -> Self { Self(x) }
	#[inline] pub const fn as_raw(self) -> i32 { self.0 }
	pub const U16: Self = Self(0);
	pub const U32: Self = Self(1);
}



#[repr(transparent)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct PipelineBindPoint(pub(crate) i32);
impl PipelineBindPoint {
	#[inline] pub const fn from_raw(x: i32) -> Self { Self(x) }
	#[inline] pub const fn as_raw(self) -> i32 { self.0 }
	pub const GRAPHICS: Self = Self(0);
	pub const COMPUTE:  Self = Self(1);
}


// ── Sampler ─────────────────────────────────────────────────

#[repr(transparent)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct Filter(pub(crate) i32);
impl Filter {
	#[inline] pub const fn from_raw(x: i32) -> Self { Self(x) }
	#[inline] pub const fn as_raw(self) -> i32 { self.0 }
	pub const NEAREST: Self = Self(0);
	pub const LINEAR:  Self = Self(1);
}

#[repr(transparent)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct SamplerAddressMode(pub(crate) i32);
impl SamplerAddressMode {
	#[inline] pub const fn from_raw(x: i32) -> Self { Self(x) }
	#[inline] pub const fn as_raw(self) -> i32 { self.0 }
	pub const REPEAT:          Self = Self(0);
	pub const MIRRORED_REPEAT: Self = Self(1);
	pub const CLAMP_TO_EDGE:   Self = Self(2);
	pub const CLAMP_TO_BORDER: Self = Self(3);
}

// ── Resource Sharing ────────────────────────────────────────

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum SharingMode { Exclusive }

// ── Image ───────────────────────────────────────────────────
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ImageAspect { Color, Depth, Stencil, DepthStencil }
// ── Descriptors ─────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DescriptorType {
	Sampler,
	CombinedImageSampler,
	SampledImage,
	StorageImage,
	UniformTexelBuffer,
	StorageTexelBuffer,
	UniformBuffer,
	StorageBuffer,
	UniformBufferDynamic,
	StorageBufferDynamic,
	InputAttachment,
}


// ── Rasterization ───────────────────────────────────────────

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum CullMode { None, Front, Back, FrontAndBack }

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum FrontFace { CounterClockwise, Clockwise }

#[repr(transparent)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct PolygonMode(pub(crate) i32);
impl PolygonMode {
	#[inline] pub const fn from_raw(x: i32) -> Self { Self(x) }
	#[inline] pub const fn as_raw(self) -> i32 { self.0 }
	pub const FILL:  Self = Self(0);
	pub const LINE:  Self = Self(1);
	pub const POINT: Self = Self(2);
}


#[repr(transparent)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct PrimitiveTopology(pub(crate) i32);
impl PrimitiveTopology {
	#[inline] pub const fn from_raw(x: i32) -> Self { Self(x) }
	#[inline] pub const fn as_raw(self) -> i32 { self.0 }
	pub const POINT_LIST:                    Self = Self(0);
	pub const LINE_LIST:                     Self = Self(1);
	pub const LINE_STRIP:                    Self = Self(2);
	pub const TRIANGLE_LIST:                 Self = Self(3);
	pub const TRIANGLE_STRIP:                Self = Self(4);
	pub const TRIANGLE_FAN:                  Self = Self(5);
	pub const LINE_LIST_WITH_ADJACENCY:      Self = Self(6);
	pub const LINE_STRIP_WITH_ADJACENCY:     Self = Self(7);
	pub const TRIANGLE_LIST_WITH_ADJACENCY:  Self = Self(8);
	pub const TRIANGLE_STRIP_WITH_ADJACENCY: Self = Self(9);
	pub const PATCH_LIST:                    Self = Self(10);
}

// ── Depth ───────────────────────────────────────────────────

#[repr(transparent)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct CompareOp(pub(crate) i32);
impl CompareOp {
	#[inline] pub const fn from_raw(x: i32) -> Self { Self(x) }
	#[inline] pub const fn as_raw(self) -> i32 { self.0 }
	pub const NEVER:            Self = Self(0);
	pub const LESS:             Self = Self(1);
	pub const EQUAL:            Self = Self(2);
	pub const LESS_OR_EQUAL:    Self = Self(3);
	pub const GREATER:          Self = Self(4);
	pub const NOT_EQUAL:        Self = Self(5);
	pub const GREATER_OR_EQUAL: Self = Self(6);
	pub const ALWAYS:           Self = Self(7);
}

// ── Blend ───────────────────────────────────────────────────

#[repr(transparent)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct BlendFactor(pub(crate) i32);
impl BlendFactor {
	#[inline] pub const fn from_raw(x: i32) -> Self { Self(x) }
	#[inline] pub const fn as_raw(self) -> i32 { self.0 }
	pub const ZERO:                     Self = Self(0);
	pub const ONE:                      Self = Self(1);
	pub const SRC_COLOR:                Self = Self(2);
	pub const ONE_MINUS_SRC_COLOR:      Self = Self(3);
	pub const DST_COLOR:                Self = Self(4);
	pub const ONE_MINUS_DST_COLOR:      Self = Self(5);
	pub const SRC_ALPHA:                Self = Self(6);
	pub const ONE_MINUS_SRC_ALPHA:      Self = Self(7);
	pub const DST_ALPHA:                Self = Self(8);
	pub const ONE_MINUS_DST_ALPHA:      Self = Self(9);
	pub const CONSTANT_COLOR:           Self = Self(10);
	pub const ONE_MINUS_CONSTANT_COLOR: Self = Self(11);
	pub const CONSTANT_ALPHA:           Self = Self(12);
	pub const ONE_MINUS_CONSTANT_ALPHA: Self = Self(13);
	pub const SRC_ALPHA_SATURATE:       Self = Self(14);
	pub const SRC1_COLOR:               Self = Self(15);
	pub const ONE_MINUS_SRC1_COLOR:     Self = Self(16);
	pub const SRC1_ALPHA:               Self = Self(17);
	pub const ONE_MINUS_SRC1_ALPHA:     Self = Self(18);
}



#[repr(transparent)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct BlendOp(pub(crate) i32);
impl BlendOp {
	#[inline] pub const fn from_raw(x: i32) -> Self { Self(x) }
	#[inline] pub const fn as_raw(self) -> i32 { self.0 }
	pub const ADD:              Self = Self(0);
	pub const SUBTRACT:         Self = Self(1);
	pub const REVERSE_SUBTRACT: Self = Self(2);
	pub const MIN:              Self = Self(3);
	pub const MAX:              Self = Self(4);
}

// ── Vertex Input ────────────────────────────────────────────

#[repr(transparent)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct VertexInputRate(pub(crate) i32);
impl VertexInputRate {
	#[inline] pub const fn from_raw(x: i32) -> Self { Self(x) }
	#[inline] pub const fn as_raw(self) -> i32 { self.0 }
	pub const VERTEX:   Self = Self(0);
	pub const INSTANCE: Self = Self(1);
}


#[derive(Debug, Copy, Clone)]
pub struct VertexBindingDesc {
	pub binding: u32,
	pub stride: u32,
	pub input_rate: VertexInputRate,
}

#[derive(Debug, Copy, Clone)]
pub struct VertexAttributeDesc {
	pub location: u32,
	pub binding: u32,
	pub format: Format,
	pub offset: u32,
}

// ── Bitflags (u64 — modern Vulkan) ──────────────────────────

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct ShaderStages(pub u64);

impl ShaderStages {
	pub const VERTEX: Self = Self(1);
	pub const FRAGMENT: Self = Self(2);
	pub const COMPUTE: Self = Self(4);
	pub const ALL_GRAPHICS: Self = Self(1 | 2);
	pub const ALL: Self = Self(1 | 2 | 4);
}

impl BitOr for ShaderStages {
	type Output = Self;
	fn bitor(self, rhs: Self) -> Self { Self(self.0 | rhs.0) }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct BufferUsage(pub u64);

impl BufferUsage {
	pub const TRANSFER_SRC: Self = Self(0x0001);
	pub const TRANSFER_DST: Self = Self(0x0002);
	pub const UNIFORM:      Self = Self(0x0010);
	pub const STORAGE:      Self = Self(0x0020);
	pub const INDEX:        Self = Self(0x0040);
	pub const VERTEX:       Self = Self(0x0080);
	pub const INDIRECT:     Self = Self(0x0100);
}


impl BitOr for BufferUsage {
	type Output = Self;
	fn bitor(self, rhs: Self) -> Self { Self(self.0 | rhs.0) }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct ImageUsage(pub u64);

impl ImageUsage {
	pub const TRANSFER_SRC: Self = Self(1);
	pub const TRANSFER_DST: Self = Self(2);
	pub const SAMPLED: Self = Self(4);
	pub const STORAGE: Self = Self(8);
	pub const COLOR_ATTACHMENT: Self = Self(16);
	pub const DEPTH_STENCIL: Self = Self(32);
}

impl BitOr for ImageUsage {
	type Output = Self;
	fn bitor(self, rhs: Self) -> Self { Self(self.0 | rhs.0) }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct CommandPoolFlags(pub u64);

impl CommandPoolFlags {
	pub const TRANSIENT: Self = Self(1);
	pub const RESET_COMMAND_BUFFER: Self = Self(2);
	
}

impl BitOr for CommandPoolFlags {
	type Output = Self;
	fn bitor(self, rhs: Self) -> Self { Self(self.0 | rhs.0) }
}


// ── Memory ──────────────────────────────────────────────────
#[derive(Clone, Copy)]
pub struct MemoryRequirements {
	pub size: u64,
	pub alignment: u64,
	pub memory_type_bits: u32,
}
// ── Pipeline Layout ─────────────────────────────────────────

#[derive(Debug, Copy, Clone)]
pub struct PushConstantRange {
	pub stages: ShaderStages,
	pub offset: u32,
	pub size: u32,
}
impl PushConstantRange {
	pub const fn stages(&self) -> ShaderStages { self.stages }
	pub const fn offset(&self) -> u32 { self.offset }
	pub const fn size(&self) -> u32 { self.size }
}
// ── Descriptor Binding ──────────────────────────────────────

// ── Descriptor Binding ──────────────────────────────────────

#[derive(Debug, Copy, Clone)]
pub struct DescriptorBinding {
	pub binding: u32,
	pub descriptor_type: DescriptorType,
	pub count: u32,
	pub stages: ShaderStages,
}

#[derive(Debug, Copy, Clone)]
pub struct DescriptorPoolSize {
	pub descriptor_type: DescriptorType,
	pub count: u32,
}

// ── Queue Family Sentinel ───────────────────────────────────

pub const QUEUE_FAMILY_IGNORED: u32 = u32::MAX;


#[derive(Clone, Copy, Debug)]
pub struct Viewport {
	pub x: f32,
	pub y: f32,
	pub width: f32,
	pub height: f32,
	pub min_depth: f32,
	pub max_depth: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct Rect2D {
	offset: Offset2D,
	extent: Extent2D,
}

impl Offset2D {
	// Constructor
	pub fn new(x: i32, y: i32) -> Self {
		Self { x, y }
	}
	
	// Getters
	pub fn x(&self) -> i32 {
		self.x
	}
	
	pub fn y(&self) -> i32 {
		self.y
	}
	
	// Setters
	pub fn set_x(&mut self, x: i32) {
		self.x = x;
	}
	
	pub fn set_y(&mut self, y: i32) {
		self.y = y;
	}
	
	// Utility
	pub fn translate(&mut self, dx: i32, dy: i32) {
		self.x += dx;
		self.y += dy;
	}
	
	pub fn is_origin(&self) -> bool {
		self.x == 0 && self.y == 0
	}
}
impl Extent2D {
	// Constructor
	pub fn new(width: u32, height: u32) -> Self {
		Self { width, height }
	}
	
	// Getters
	pub fn width(&self) -> u32 {
		self.width
	}
	
	pub fn height(&self) -> u32 {
		self.height
	}
	
	// Setters
	pub fn set_width(&mut self, width: u32) {
		self.width = width;
	}
	
	pub fn set_height(&mut self, height: u32) {
		self.height = height;
	}
	
	// Utility
	pub fn area(&self) -> u32 {
		self.width * self.height
	}
	
	pub fn is_zero(&self) -> bool {
		self.width == 0 || self.height == 0
	}
}

impl Rect2D {
	pub fn new(offset: Offset2D, extent: Extent2D) -> Self {
		Self { offset, extent }
	}
	
	// ---- structured access ----
	
	pub fn offset(&self) -> Offset2D {
		self.offset
	}
	
	pub fn extent(&self) -> Extent2D {
		self.extent
	}
	
	pub fn set_offset(&mut self, offset: Offset2D) {
		self.offset = offset;
	}
	
	pub fn set_extent(&mut self, extent: Extent2D) {
		self.extent = extent;
	}
	
	// ---- delegated access ----
	
	pub fn x(&self) -> i32 {
		self.offset.x()
	}
	
	pub fn y(&self) -> i32 {
		self.offset.y()
	}
	
	pub fn width(&self) -> u32 {
		self.extent.width()
	}
	
	pub fn height(&self) -> u32 {
		self.extent.height()
	}
	
	pub fn set_x(&mut self, x: i32) {
		self.offset.set_x(x);
	}
	
	pub fn set_y(&mut self, y: i32) {
		self.offset.set_y(y);
	}
	
	pub fn set_width(&mut self, width: u32) {
		self.extent.set_width(width);
	}
	
	pub fn set_height(&mut self, height: u32) {
		self.extent.set_height(height);
	}
	
	// ---- utility ----
	
	pub fn area(&self) -> u32 {
		self.extent.area()
	}
	
	pub fn is_zero(&self) -> bool {
		self.extent.is_zero()
	}
}

impl Extent3D {
	pub fn new(width: u32, height: u32, depth: u32) -> Self {
		Self { width, height, depth }
	}
	
	pub fn width(&self) -> u32 {
		self.width
	}
	
	pub fn height(&self) -> u32 {
		self.height
	}
	
	pub fn depth(&self) -> u32 {
		self.depth
	}
	
	pub fn set_width(&mut self, width: u32) {
		self.width = width;
	}
	
	pub fn set_height(&mut self, height: u32) {
		self.height = height;
	}
	
	pub fn set_depth(&mut self, depth: u32) {
		self.depth = depth;
	}
}
#[repr(transparent)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct CommandBufferUsageFlags(pub(crate) u32);
impl CommandBufferUsageFlags {
	pub const ONE_TIME_SUBMIT:    Self = Self(0b1);
	pub const RENDER_PASS_CONTINUE: Self = Self(0b10);
	pub const SIMULTANEOUS_USE:   Self = Self(0b100);
	#[inline] pub const fn from_raw(x: u32) -> Self { Self(x) }
	#[inline] pub const fn as_raw(self) -> u32 { self.0 }
	#[inline] pub const fn bits(self) -> u32 { self.0 }
}
impl std::ops::BitOr for CommandBufferUsageFlags {
	type Output = Self;
	fn bitor(self, rhs: Self) -> Self { Self(self.0 | rhs.0) }
}
impl std::ops::BitAnd for CommandBufferUsageFlags {
	type Output = Self;
	fn bitand(self, rhs: Self) -> Self { Self(self.0 & rhs.0) }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PipelineStageFlags2(pub u64);

impl PipelineStageFlags2 {
	pub const NONE: Self = Self(0);
	pub const TOP_OF_PIPE: Self = Self(0x00000001);
	pub const DRAW_INDIRECT: Self = Self(0x00000002);
	pub const VERTEX_INPUT: Self = Self(0x00000004);
	pub const VERTEX_SHADER: Self = Self(0x00000008);
	pub const FRAGMENT_SHADER: Self = Self(0x00000080);
	pub const EARLY_FRAGMENT: Self = Self(0x00000100);
	pub const LATE_FRAGMENT: Self = Self(0x00000200);
	pub const COLOR_ATTACHMENT_OUTPUT: Self = Self(0x00000400);
	pub const COMPUTE_SHADER: Self = Self(0x00000800);
	pub const TRANSFER: Self = Self(0x00001000);
	pub const BOTTOM_OF_PIPE: Self = Self(0x00002000);
	pub const ALL_GRAPHICS: Self = Self(0x00008000);
	pub const ALL_COMMANDS: Self = Self(0x00010000);
	pub const COPY: Self = Self(0x100000000);
	pub const RESOLVE: Self = Self(0x200000000);
	pub const BLIT: Self = Self(0x400000000);
	pub const CLEAR: Self = Self(0x800000000);
	pub const HOST: Self = Self(0x00004000);
}
impl std::ops::BitOr for PipelineStageFlags2 {
	type Output = Self;
	fn bitor(self, rhs: Self) -> Self { Self(self.0 | rhs.0) }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AccessFlags2(pub u64);

impl AccessFlags2 {
	pub const NONE: Self = Self(0);
	pub const INDIRECT_COMMAND_READ: Self = Self(0x00000001);
	pub const INDEX_READ: Self = Self(0x00000002);
	pub const VERTEX_ATTRIBUTE_READ: Self = Self(0x00000004);
	pub const UNIFORM_READ: Self = Self(0x00000008);
	pub const SHADER_READ: Self = Self(0x00000020);
	pub const SHADER_WRITE: Self = Self(0x00000040);
	pub const COLOR_ATTACHMENT_READ: Self = Self(0x00000080);
	pub const COLOR_ATTACHMENT_WRITE: Self = Self(0x00000100);
	pub const TRANSFER_READ: Self = Self(0x00000800);
	pub const TRANSFER_WRITE: Self = Self(0x00001000);
	pub const MEMORY_READ: Self = Self(0x00008000);
	pub const MEMORY_WRITE: Self = Self(0x00010000);
	pub const DEPTH_STENCIL_ATTACHMENT_READ: Self = Self(0x00000200);
	pub const DEPTH_STENCIL_ATTACHMENT_WRITE: Self = Self(0x00000400);
	pub const HOST_READ: Self = Self(0x00002000);
	pub const HOST_WRITE: Self = Self(0x00004000);
}
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct MemoryPropertyFlags(pub u32);

impl MemoryPropertyFlags {
	pub const DEVICE_LOCAL: Self  = Self(0b00001);
	pub const HOST_VISIBLE: Self  = Self(0b00010);
	pub const HOST_COHERENT: Self = Self(0b00100);
	pub const HOST_CACHED: Self   = Self(0b01000);
	pub const LAZILY_ALLOCATED: Self = Self(0b10000);
	
	pub const fn empty() -> Self {
		Self(0)
	}
	
	pub const fn contains(self, other: Self) -> bool {
		(self.0 & other.0) == other.0
	}
}

impl std::ops::BitOr for MemoryPropertyFlags {
	type Output = Self;
	fn bitor(self, rhs: Self) -> Self {
		Self(self.0 | rhs.0)
	}
}
impl std::ops::BitOrAssign for MemoryPropertyFlags {
	fn bitor_assign(&mut self, rhs: Self) {
		self.0 |= rhs.0;
	}
}

impl std::ops::BitOr for AccessFlags2 {
	type Output = Self;
	fn bitor(self, rhs: Self) -> Self { Self(self.0 | rhs.0) }
}

use crate::domain::{Stage, Access};
impl From<Stage> for PipelineStageFlags2 {
	fn from(s: Stage) -> Self {
		match s {
			Stage::None => Self::NONE,
			Stage::Top => Self::TOP_OF_PIPE,
			Stage::DrawIndirect => Self::DRAW_INDIRECT,
			Stage::VertexInput => Self::VERTEX_INPUT,
			Stage::Vertex => Self::VERTEX_SHADER,
			Stage::Fragment => Self::FRAGMENT_SHADER,
			Stage::EarlyFragmentTests => Self::EARLY_FRAGMENT,
			Stage::LateFragmentTests => Self::LATE_FRAGMENT,
			Stage::ColorOutput => Self::COLOR_ATTACHMENT_OUTPUT,
			Stage::Compute => Self::COMPUTE_SHADER,
			Stage::Transfer => Self::TRANSFER,
			Stage::Host => Self::HOST,
			Stage::Bottom => Self::BOTTOM_OF_PIPE,
			Stage::All => Self::ALL_COMMANDS,
		}
	}
}

impl From<Access> for AccessFlags2 {
	fn from(a: Access) -> Self {
		match a {
			Access::None => Self::NONE,
			Access::MemoryRead => Self::MEMORY_READ,
			Access::MemoryWrite => Self::MEMORY_WRITE,
			Access::UniformRead => Self::UNIFORM_READ,
			Access::SampledRead => Self::SHADER_READ,
			Access::StorageRead => Self::SHADER_READ,
			Access::StorageWrite => Self::SHADER_WRITE,
			Access::ColorAttachmentRead => Self::COLOR_ATTACHMENT_READ,
			Access::ColorAttachmentWrite => Self::COLOR_ATTACHMENT_WRITE,
			Access::DepthStencilRead => Self::DEPTH_STENCIL_ATTACHMENT_READ,
			Access::DepthStencilWrite => Self::DEPTH_STENCIL_ATTACHMENT_WRITE,
			Access::TransferRead => Self::TRANSFER_READ,
			Access::TransferWrite => Self::TRANSFER_WRITE,
			Access::VertexAttributeRead => Self::VERTEX_ATTRIBUTE_READ,
			Access::IndexRead => Self::INDEX_READ,
			Access::IndirectCommandRead => Self::INDIRECT_COMMAND_READ,
			Access::HostRead => Self::HOST_READ,
			Access::HostWrite => Self::HOST_WRITE,
		}
	}
}

#[repr(transparent)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct AttachmentLoadOp(pub(crate) i32);
impl AttachmentLoadOp {
	#[inline] pub const fn from_raw(x: i32) -> Self { Self(x) }
	#[inline] pub const fn as_raw(self) -> i32 { self.0 }
	pub const LOAD:      Self = Self(0);
	pub const CLEAR:     Self = Self(1);
	pub const DONT_CARE: Self = Self(2);
}

#[repr(transparent)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct AttachmentStoreOp(pub(crate) i32);
impl AttachmentStoreOp {
	#[inline] pub const fn from_raw(x: i32) -> Self { Self(x) }
	#[inline] pub const fn as_raw(self) -> i32 { self.0 }
	pub const STORE:     Self = Self(0);
	pub const DONT_CARE: Self = Self(1);
}



#[derive(Clone, Copy, Debug)]
pub enum ClearValue {
	Color([f32; 4]),
	DepthStencil(f32, u32),
}

use super::Backend;
pub struct GraphicsPipelineDesc<'a, B: Backend> {
	pub shaders: ShaderConfig<B>,
	pub layout: B::PipelineLayout,
	pub vertex: VertexConfig<'a>,
	pub raster: RasterConfig,
	pub depth: DepthConfig,
	pub blend: BlendConfig,
	pub target: RenderTargetConfig,
}
#[derive( Copy, Clone)]
pub struct ShaderConfig<B: Backend> {
	pub vert: B::ShaderModule,
	pub frag: B::ShaderModule,
	pub entry: &'static CStr,
}