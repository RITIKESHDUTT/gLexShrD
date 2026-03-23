use crate::core::cmd::{CommandBuffer, Recording};
use crate::core::types::{
	Extent2D,
	Extent3D,
	Format,
	ImageAspect,
	ImageUsage,
	QUEUE_FAMILY_IGNORED
};
use crate::core::{Allocation, Backend, CommandOps, DeviceOps, ImageBarrierInfo};
use crate::domain::{Access, ImageLayout, Stage};
use std::marker::PhantomData;
use std::mem::ManuallyDrop;

pub mod img_state {
	pub struct Undefined;
	pub struct TransferDst;
	pub struct TransferSrc;
	pub struct ColorAttachment;
	pub struct DepthStencilAttachment;
	pub struct ShaderReadOnly;
	pub struct PresentSrc;
}

// ─────────────────────────────────────────────────────────────
// Owned Image (allocated by us, destroyed on Drop)
// ─────────────────────────────────────────────────────────────

// CHANGE: replaced `memory: B::DeviceMemory` with `sub: Option<B::Allocation>`.
// Some = arena-backed (owned), None = non-owning (future use, mirrors Buffer).
// Drop no longer calls free_memory — sub drops automatically, pushing a
// FreeRequest to the ReturnQueue for the next reap() to reclaim.
pub struct Image<'dev, S, B: Backend> {
	device: &'dev B::Device,
	handle: B::Image,
	// CHANGE: was `memory: B::DeviceMemory`
	sub:    Option<B::Allocation>,
	format: Format,
	extent: Extent3D,
	pub(crate) family: u32,
	_state: PhantomData<S>,
}

impl<S, B: Backend> Image<'_, S, B> {
	pub fn handle(&self) -> B::Image  { self.handle }
	pub fn extent(&self) -> Extent3D  { self.extent }
	pub fn format(&self) -> Format    { self.format }
	pub fn family(&self) -> u32       { self.family }
}

// CHANGE: Drop no longer calls free_memory.
// self.sub drops automatically after destroy_image, pushing a FreeRequest
// to the ReturnQueue. The arena reclaims the memory on the next reap() call.
impl<S, B: Backend> Drop for Image<'_, S, B> {
	fn drop(&mut self) {
		self.device.destroy_image(self.handle);
		// self.sub drops here → FreeRequest pushed to ReturnQueue (if Some)
	}
}

// CHANGE: allocate_2d no longer takes `memory_type_index: u32` and calls
// allocate_memory internally. The caller (GpuContext::allocate_image_2d)
// probes requirements, calls the arena, and passes the SubAllocation in.
// Old signature: (device, format, extent, usage, memory_type_index, family)
// New signature: (device, sub, format, extent, usage, family)
impl<'dev, B: Backend> Image<'dev, img_state::Undefined, B>
	where B::Allocation: Allocation<Memory = B::DeviceMemory>
{
	pub fn allocate_2d(
		device: &'dev B::Device,
		// CHANGE: was (memory_type_index: u32) — caller now owns the ticket.
		sub:    B::Allocation,
		format: Format,
		extent: Extent2D,
		usage:  ImageUsage,
		family: u32,
	) -> Result<Self, B::Error> {
		let handle = device.create_image_2d(format, extent.width(), extent.height(), usage)?;
		// CHANGE: bind at sub.offset() not 0 — sub-allocation may start at a
		// non-zero offset within its parent DeviceMemory block.
		
		let req = device.get_image_memory_requirements(handle);
		
		assert!(sub.size() >= req.size, "suballocation too small for image");
		assert!(sub.memory_offset() % req.alignment == 0, "misaligned image bind");
		
		device.bind_image_memory(handle, sub.memory(), sub.memory_offset())?;
		Ok(Self {
			device, handle,
			sub: Some(sub),  // CHANGE: wrapped in Some
			format,
			extent: Extent3D::new(extent.width(), extent.height(), 1),
			family,
			_state: PhantomData,
		})
	}
}


// ─────────────────────────────────────────────────────────────
// Swapchain Image (NOT owned — swapchain manages lifetime)
// ─────────────────────────────────────────────────────────────

pub struct SwapchainImage<'dev, S, B: Backend> {
	device: &'dev B::Device,
	handle: B::Image,
	extent: Extent3D,
	_state: PhantomData<S>,
}

impl<S, B: Backend> SwapchainImage<'_, S, B> {
	pub fn handle(&self) -> B::Image { self.handle }
	pub fn extent(&self) -> Extent3D { self.extent }
}

impl<'dev, B: Backend> SwapchainImage<'dev, img_state::Undefined, B> {
	pub fn from_raw(device: &'dev B::Device, handle: B::Image, extent: Extent2D) -> Self {
		Self {
			device, handle,
			extent: Extent3D::new(extent.width(), extent.height(), 1),
			_state: PhantomData,
		}
	}
}


// ─────────────────────────────────────────────────────────────
// Barrier helper
// ─────────────────────────────────────────────────────────────

pub fn image_barrier<R, B: Backend>(
	device:     &B::Device,
	cmd:        &CommandBuffer<'_, Recording, B, R>,
	image:      B::Image,
	old_layout: ImageLayout,
	new_layout: ImageLayout,
	src_stage:  Stage,
	src_access: Access,
	dst_stage:  Stage,
	dst_access: Access,
	aspect:     ImageAspect,
	src_family: u32,
	dst_family: u32,
) {
	let (src_f, dst_f) = if src_family == dst_family {
		(QUEUE_FAMILY_IGNORED, QUEUE_FAMILY_IGNORED)
	} else {
		(src_family, dst_family)
	};
	device.cmd_image_barrier(cmd.handle(), &[ImageBarrierInfo {
		image,
		old_layout, new_layout,
		src_stage, src_access,
		dst_stage, dst_access,
		aspect,
		src_queue_family: src_f,
		dst_queue_family: dst_f,
	}]);
}

// CHANGE: retype uses std::mem::take on the Option<B::Allocation> field —
// same pattern as Buffer::retype. Option is always Default so no extra
// bound on B::Allocation is needed. ManuallyDrop prevents double-drop.
fn retype<'dev, Old, New, B: Backend>(img: Image<'dev, Old, B>) -> Image<'dev, New, B> {
	let mut img = ManuallyDrop::new(img);
	Image {
		device: img.device,
		handle: img.handle,
		// CHANGE: was `memory: img.memory` — move the Option out safely.
		sub:    std::mem::take(&mut img.sub),
		format: img.format,
		extent: img.extent,
		family: img.family,
		_state: PhantomData,
	}
}

fn retype_swapchain<'dev, Old, New, B: Backend>(
	img: SwapchainImage<'dev, Old, B>,
) -> SwapchainImage<'dev, New, B> {
	SwapchainImage {
		device: img.device,
		handle: img.handle,
		extent: img.extent,
		_state: PhantomData,
	}
}


// ─────────────────────────────────────────────────────────────
// Owned Image transitions
// ─────────────────────────────────────────────────────────────
// ─── Undefined transitions ───────────────────────────────────

impl<'dev, B: Backend> Image<'dev, img_state::Undefined, B> {
	pub fn into_transfer_dst(
		mut self,
		cmd: &CommandBuffer<'_, Recording, B>,
	) -> Image<'dev, img_state::TransferDst, B> {
		image_barrier(self.device, cmd, self.handle,
					  ImageLayout::Undefined,   ImageLayout::TransferDst,
					  Stage::Top,               Access::None,
					  Stage::Transfer,          Access::TransferWrite,
					  ImageAspect::Color,
					  QUEUE_FAMILY_IGNORED,     QUEUE_FAMILY_IGNORED,
		);
		self.family = cmd.family();
		retype(self)
	}
	
	pub fn into_color_attachment(
		mut self,
		cmd: &CommandBuffer<'_, Recording, B>,
	) -> Image<'dev, img_state::ColorAttachment, B> {
		image_barrier(self.device, cmd, self.handle,
					  ImageLayout::Undefined,        ImageLayout::ColorAttachment,
					  Stage::ColorOutput,            Access::None,
					  Stage::ColorOutput,            Access::ColorAttachmentWrite,
					  ImageAspect::Color,
					  QUEUE_FAMILY_IGNORED,          QUEUE_FAMILY_IGNORED,
		);
		self.family = cmd.family();
		retype(self)
	}
	
	pub fn into_transfer_src(
		mut self,
		cmd: &CommandBuffer<'_, Recording, B>,
	) -> Image<'dev, img_state::TransferSrc, B> {
		image_barrier(self.device, cmd, self.handle,
					  ImageLayout::Undefined,   ImageLayout::TransferSrc,
					  Stage::Top,               Access::None,
					  Stage::Transfer,          Access::TransferRead,
					  ImageAspect::Color,
					  QUEUE_FAMILY_IGNORED,     QUEUE_FAMILY_IGNORED,
		);
		self.family = cmd.family();
		retype(self)
	}
}

// ─── TransferDst transitions ─────────────────────────────────

impl<'dev, B: Backend> Image<'dev, img_state::TransferDst, B> {
	pub fn into_shader_read(
		mut self,
		cmd: &CommandBuffer<'_, Recording, B>,
	) -> Image<'dev, img_state::ShaderReadOnly, B> {
		image_barrier(self.device, cmd, self.handle,
					  ImageLayout::TransferDst,   ImageLayout::ShaderReadOnly,
					  Stage::Transfer,            Access::TransferWrite,
					  Stage::Fragment,            Access::SampledRead,
					  ImageAspect::Color,
					  self.family,                cmd.family(),
		);
		self.family = cmd.family();
		retype(self)
	}
	
	pub fn release_to_shader_read(
		self,
		cmd:        &CommandBuffer<'_, Recording, B>,
		dst_family: u32,
	) -> Image<'dev, img_state::ShaderReadOnly, B> {
		image_barrier(self.device, cmd, self.handle,
					  ImageLayout::TransferDst,   ImageLayout::ShaderReadOnly,
					  Stage::Transfer,            Access::TransferWrite,
					  Stage::Fragment,            Access::SampledRead,
					  ImageAspect::Color,
					  self.family,                dst_family,
		);
		retype(self)
	}
	
	pub fn into_color_attachment(
		mut self,
		cmd: &CommandBuffer<'_, Recording, B>,
	) -> Image<'dev, img_state::ColorAttachment, B> {
		image_barrier(self.device, cmd, self.handle,
					  ImageLayout::TransferDst,      ImageLayout::ColorAttachment,
					  Stage::Transfer,               Access::TransferWrite,
					  Stage::ColorOutput,            Access::ColorAttachmentWrite,
					  ImageAspect::Color,
					  self.family,                   cmd.family(),
		);
		self.family = cmd.family();
		retype(self)
	}
}

// ─── ShaderReadOnly transitions ──────────────────────────────

impl<'dev, B: Backend> Image<'dev, img_state::ShaderReadOnly, B> {
	pub fn acquire(&mut self, cmd: &CommandBuffer<'_, Recording, B>) {
		if self.family == cmd.family() { return; }
		image_barrier(self.device, cmd, self.handle,
					  ImageLayout::TransferDst,   ImageLayout::ShaderReadOnly,
					  Stage::Top,                 Access::None,
					  Stage::Fragment,            Access::SampledRead,
					  ImageAspect::Color,
					  self.family,                cmd.family(),
		);
		self.family = cmd.family();
	}
	
	pub fn into_color_attachment(
		mut self,
		cmd: &CommandBuffer<'_, Recording, B>,
	) -> Image<'dev, img_state::ColorAttachment, B> {
		image_barrier(self.device, cmd, self.handle,
					  ImageLayout::ShaderReadOnly,   ImageLayout::ColorAttachment,
					  Stage::Fragment,               Access::SampledRead,
					  Stage::ColorOutput,            Access::ColorAttachmentWrite,
					  ImageAspect::Color,
					  QUEUE_FAMILY_IGNORED,          QUEUE_FAMILY_IGNORED,
		);
		self.family = cmd.family();
		retype(self)
	}
}

// ─── ColorAttachment transitions ─────────────────────────────

impl<'dev, B: Backend> Image<'dev, img_state::ColorAttachment, B> {
	pub fn into_present_src(
		mut self,
		cmd: &CommandBuffer<'_, Recording, B>,
	) -> Image<'dev, img_state::PresentSrc, B> {
		image_barrier(self.device, cmd, self.handle,
					  ImageLayout::ColorAttachment,  ImageLayout::Present,
					  Stage::ColorOutput,            Access::ColorAttachmentWrite,
					  Stage::Bottom,                 Access::None,
					  ImageAspect::Color,
					  QUEUE_FAMILY_IGNORED,          QUEUE_FAMILY_IGNORED,
		);
		self.family = cmd.family();
		retype(self)
	}
	
	pub fn into_shader_read(
		mut self,
		cmd: &CommandBuffer<'_, Recording, B>,
	) -> Image<'dev, img_state::ShaderReadOnly, B> {
		image_barrier(self.device, cmd, self.handle,
					  ImageLayout::ColorAttachment,  ImageLayout::ShaderReadOnly,
					  Stage::ColorOutput,            Access::ColorAttachmentWrite,
					  Stage::Fragment,               Access::SampledRead,
					  ImageAspect::Color,
					  QUEUE_FAMILY_IGNORED,          QUEUE_FAMILY_IGNORED,
		);
		self.family = cmd.family();
		retype(self)
	}
	
	pub fn into_transfer_src(mut self, cmd: &CommandBuffer<'_, Recording, B>) -> Image<'dev, img_state::TransferSrc,
		B> {
		image_barrier(self.device, cmd, self.handle,
					  ImageLayout::ColorAttachment, ImageLayout::TransferSrc,
					  Stage::ColorOutput, Access::ColorAttachmentWrite,
					  Stage::Transfer, Access::TransferRead,
					  ImageAspect::Color,
					  QUEUE_FAMILY_IGNORED, QUEUE_FAMILY_IGNORED,
		);
		self.family = cmd.family();
		retype(self)
	}
}

// ─── TransferSrc transitions ─────────────────────────────────

impl<'dev, B: Backend> Image<'dev, img_state::TransferSrc, B> {
	pub fn into_shader_read(mut self, cmd: &CommandBuffer<'_, Recording, B>) -> Image<'dev, img_state::ShaderReadOnly,
		B> {
		image_barrier(self.device, cmd, self.handle,
					  ImageLayout::TransferSrc, ImageLayout::ShaderReadOnly,
					  Stage::Transfer, Access::TransferRead,
					  Stage::Fragment, Access::SampledRead,
					  ImageAspect::Color,
					  QUEUE_FAMILY_IGNORED, QUEUE_FAMILY_IGNORED,
		);
		self.family = cmd.family();
		retype(self)
	}
}

// ─── SwapchainImage transitions ──────────────────────────────

impl<'dev, B: Backend> SwapchainImage<'dev, img_state::Undefined, B> {
	pub fn into_color_attachment(
		self,
		cmd: &CommandBuffer<'_, Recording, B>,
	) -> SwapchainImage<'dev, img_state::ColorAttachment, B> {
		self.device.cmd_image_barrier(cmd.handle(), &[ImageBarrierInfo {
			image:            self.handle,
			old_layout:       ImageLayout::Undefined,
			new_layout:       ImageLayout::ColorAttachment,
			src_stage:        Stage::ColorOutput,
			src_access:       Access::None,
			dst_stage:        Stage::ColorOutput,
			dst_access:       Access::ColorAttachmentWrite,
			aspect:           ImageAspect::Color,
			src_queue_family: QUEUE_FAMILY_IGNORED,
			dst_queue_family: QUEUE_FAMILY_IGNORED,
		}]);
		retype_swapchain(self)
	}
}

impl<'dev, B: Backend> SwapchainImage<'dev, img_state::ColorAttachment, B> {
	pub fn into_present_src(
		self,
		cmd: &CommandBuffer<'_, Recording, B>,
	) -> SwapchainImage<'dev, img_state::PresentSrc, B> {
		self.device.cmd_image_barrier(cmd.handle(), &[ImageBarrierInfo {
			image:            self.handle,
			old_layout:       ImageLayout::ColorAttachment,
			new_layout:       ImageLayout::Present,
			src_stage:        Stage::ColorOutput,
			src_access:       Access::ColorAttachmentWrite,
			dst_stage:        Stage::Bottom,
			dst_access:       Access::None,
			aspect:           ImageAspect::Color,
			src_queue_family: QUEUE_FAMILY_IGNORED,
			dst_queue_family: QUEUE_FAMILY_IGNORED,
		}]);
		retype_swapchain(self)
	}
}
impl<'dev, S, B: Backend> Image<'dev, S, B>
	where B::Allocation: Allocation<Memory = B::DeviceMemory>
{
	pub fn finalize_lifetime(&mut self, t: u64) {
		if let Some(sub) = self.sub.as_mut() {
			sub.finalize_lifetime(t);
		}
	}
}