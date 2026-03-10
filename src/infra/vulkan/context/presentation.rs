use crate::core::type_state_queue::sealed::QueueHandle;
use crate::core::types::Extent2D;
use crate::core::types::Format;
use crate::core::types::MemoryPropertyFlags;
use crate::core::types::MemoryRequirements;
use crate::core::Backend;
use crate::infra::platform::Surface;
use crate::infra::vulkan::backend::{PhysicalDevice, Swapchain, VulkanBackend, VulkanInstance};
use crate::infra::VulkanContext;
use glex_platform::platform::Window;


/// Separate from `VulkanContext` — presentation is optional.
/// Headless and compute-only backends never create this.
///
/// Lifetime tied to `VulkanDevice` — cannot outlive the device.


pub struct Presentation<'dev,> {
	pub swapchain:     Swapchain<'dev>,
	pub surface:       Surface,
	pub queue:		 ash::vk::Queue,
	present_mode: PresentMode,
}

impl<'dev> Presentation<'dev> {
	pub fn new(
		ctx: &'dev VulkanContext,
		surface: Surface,
		window: &impl Window
	) -> Result<Self, <VulkanBackend as Backend>::Error> {
		let extent = window.extent();
		let swapchain = Swapchain::new(
			&ctx.instance, &ctx.device, &ctx.physical,
			&surface, extent.width(), extent.height(),
		)?;
		
		let present_queue = ctx.queues().present()
							   .map(|q| q.raw())
							   .unwrap_or_else(|| ctx.queues().graphics().raw());
		
		Ok( Self {
			swapchain,
			surface,
			queue:present_queue,
			present_mode: PresentMode::Vsync
		})
	}
	
	
	pub fn recreate_with_present_mode(
		&mut self,
		physical: &PhysicalDevice,
		width: u32,
		height: u32,
		present_mode: PresentMode,
	) -> Result<(), ash::vk::Result> {
		let vk_mode = match present_mode {
			PresentMode::Vsync => ash::vk::PresentModeKHR::FIFO,
			PresentMode::Mailbox => ash::vk::PresentModeKHR::MAILBOX,
		};
		self.swapchain.recreate_with_present_mode(
			physical, &self.surface, width, height, vk_mode,
		)
	}
	
	pub fn set_present_mode(&mut self, mode: PresentMode) {
		self.present_mode = mode;
	}
	
	pub fn present_mode(&self) -> PresentMode {
		self.present_mode
	}
	/// Acquire the next swapchain image.
	/// Returns (image_index, suboptimal).
	pub fn acquire(&self, semaphore: ash::vk::Semaphore) -> Result<(u32, bool), ash::vk::Result> {
		self.swapchain.acquire_next(semaphore)
	}
	
	/// Recreate after resize or suboptimal.
	/// Caller must ensure device_wait_idle before calling.
	pub fn recreate(&mut self, physical: &PhysicalDevice, width: u32, height: u32)
					-> Result<(), ash::vk::Result>
	{
		let vk_mode = match self.present_mode {
			PresentMode::Vsync => ash::vk::PresentModeKHR::FIFO,
			PresentMode::Mailbox => ash::vk::PresentModeKHR::MAILBOX,
		};
		self.swapchain.recreate_with_present_mode(
			physical, &self.surface, width, height, vk_mode,
		)
		
	}
	
	pub fn extent(&self) -> Extent2D {
		let e = self.swapchain.extent();
		Extent2D::new(e.width, e.height)
	}
	pub fn present(
		&self,
		image_index: u32,
		wait_semaphore: <VulkanBackend as Backend>::Semaphore,
	) -> Result<bool, <VulkanBackend as Backend>::Error> {
		self.swapchain.present_raw(self.queue, image_index, wait_semaphore)
	}
	
	pub fn format(&self) -> Format {
		Format::from(Format(self.swapchain.format().as_raw()))
	}
}
#[derive(Copy, Clone)]
pub struct MemoryIndices {
	pub staging: u32,      // Host Visible + Coherent
	pub device_local: u32, // GPU Private
	pub transcript: u32,   // Optional: Host Visible but NOT Coherent (for advanced mapping)
}
impl MemoryIndices {
	pub fn find(
		physical: &PhysicalDevice,
		instance: &VulkanInstance,
		req: MemoryRequirements,
	) -> Self {
		
		let staging = physical.find_memory_type(
			instance,
			req,
			MemoryPropertyFlags::HOST_VISIBLE | MemoryPropertyFlags::HOST_COHERENT,
		).expect("No HOST_VISIBLE|HOST_COHERENT memory");
		
		let device_local = physical.find_memory_type(
			instance,
			req,
			MemoryPropertyFlags::DEVICE_LOCAL,
		).expect("No DEVICE_LOCAL memory");
		
		let transcript = physical.find_memory_type(
			instance,
			req,
			MemoryPropertyFlags::HOST_VISIBLE,
		);
		
		Self {
			staging,
			device_local,
			transcript: transcript.expect("REASON"),
		}
	}
}

#[derive(Copy, Clone, PartialEq, Eq)]
pub enum PresentMode {
	Vsync,
	Mailbox,
}