use super::instance::VulkanInstance;
use super::physical_device::PhysicalDevice;
use crate::core::types::Format;
use crate::core::BinarySemaphore;
use crate::core::ImageView;
use crate::infra::platform::Surface;
use crate::infra::vulkan::backend::VulkanBackend;
use crate::infra::vulkan::backend::VulkanDevice;
use ash::vk;

pub struct Swapchain<'dev> {
	device: &'dev VulkanDevice,
	loader: ash::khr::swapchain::Device,
	swapchain: vk::SwapchainKHR,
	images: Vec<vk::Image>,
	image_views: Vec<ImageView<'dev, VulkanBackend>>,
	render_semaphores: Vec<BinarySemaphore<'dev, VulkanBackend>>,
	format: vk::SurfaceFormatKHR,
	extent: vk::Extent2D,
}

/// Deferred-destruction handle. Drop only after GPU timeline passes retire_value.
pub struct RetiredSwapchainResources<'dev> {
	loader: ash::khr::swapchain::Device,
	handle: vk::SwapchainKHR,
	_image_views: Vec<ImageView<'dev, VulkanBackend>>,
	_render_semaphores: Vec<BinarySemaphore<'dev, VulkanBackend>>,
}

impl Drop for RetiredSwapchainResources<'_> {
	fn drop(&mut self) {
		// Views and semaphores must die before the swapchain handle.
		self._image_views.clear();
		self._render_semaphores.clear();
		unsafe { self.loader.destroy_swapchain(self.handle, None); }
	}
}

impl<'dev> Swapchain<'dev, > {
	pub(crate) fn required_device_extensions() -> Vec<*const i8> {
		vec![ash::khr::swapchain::NAME.as_ptr()]
	}

	pub(crate) fn new(
		instance: &VulkanInstance,
		device: &'dev VulkanDevice,
		physical: &PhysicalDevice,
		surface: &Surface,
		width: u32,
		height: u32,
	) -> Result<Self, vk::Result> {
		let caps = unsafe { surface.capabilities(physical.handle())? };

		let composite_alpha = select_composite_alpha(&caps);
		let formats = unsafe { surface.formats(physical.handle())? };

		let format = formats
			.iter()
			.find(|f| {
				f.format == vk::Format::B8G8R8A8_SRGB
					&& f.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR
			})
			.or(formats.first())
			.copied()
			.ok_or(vk::Result::ERROR_FORMAT_NOT_SUPPORTED)?;
		let extent = resolve_extent(&caps, width, height);
		let image_count = select_image_count(&caps);

		let create_info = vk::SwapchainCreateInfoKHR::default()
			.surface(surface.handle())
			.min_image_count(image_count)
			.image_format(format.format)
			.image_color_space(format.color_space)
			.image_extent(extent)
			.image_array_layers(1)
			.image_usage(
				vk::ImageUsageFlags::COLOR_ATTACHMENT
					| vk::ImageUsageFlags::TRANSFER_DST
			)
			.image_sharing_mode(vk::SharingMode::EXCLUSIVE)
			.pre_transform(caps.current_transform)
			.composite_alpha(composite_alpha)
			.present_mode(vk::PresentModeKHR::FIFO)
			.clipped(true)
			.old_swapchain(vk::SwapchainKHR::null());

		let device_handle = &device.inner;
		let loader = ash::khr::swapchain::Device::new(
			instance.instance(),
			device_handle,
		);

		let swapchain = unsafe { loader.create_swapchain(&create_info, None)? };
		let images = unsafe { loader.get_swapchain_images(swapchain)? };

		let image_views: Result<Vec<_>, _> = images
			.iter()
			.map(|&image| ImageView::color_2d(device, image, Format(format.format.as_raw())))
			.collect();
		let image_views = image_views?;

		let render_semaphores: Result<Vec<_>, _> = (0..images.len())
			.map(|_| BinarySemaphore::new(device))
			.collect();
		let render_semaphores = render_semaphores?;

		Ok(Self { device, loader, swapchain, images, image_views, render_semaphores, format, extent })
	}

	pub(crate) fn acquire_next(
		&self,
		semaphore: vk::Semaphore,
	) -> Result<(u32, bool), vk::Result> {
		unsafe {
			self.loader.acquire_next_image(
				self.swapchain, u64::MAX, semaphore, vk::Fence::null(),
			)
		}
	}

	/// Returns retired resources for deferred destruction.
	pub fn recreate(
		&mut self,
		physical: &PhysicalDevice,
		surface: &Surface,
		width: u32,
		height: u32,
	) -> Result<RetiredSwapchainResources<'dev>, vk::Result> {
		self.recreate_with_present_mode(physical, surface, width, height, vk::PresentModeKHR::FIFO)
	}

	pub fn recreate_with_present_mode(
		&mut self,
		physical: &PhysicalDevice,
		surface: &Surface,
		width: u32,
		height: u32,
		present_mode: vk::PresentModeKHR,
	)-> Result<RetiredSwapchainResources<'dev>, vk::Result> {
		let supported = unsafe { surface.present_modes(physical.handle())? };
		let present_mode = if supported.contains(&present_mode) {
			present_mode
		} else if supported.contains(&vk::PresentModeKHR::MAILBOX) {
			vk::PresentModeKHR::MAILBOX
		} else {
			vk::PresentModeKHR::FIFO
		};

		let caps = unsafe { surface.capabilities(physical.handle())? };
		let composite_alpha = select_composite_alpha(&caps);
		let extent = resolve_extent(&caps, width, height);
		let image_count = select_image_count(&caps);
		let old_swapchain = self.swapchain;

		let create_info = vk::SwapchainCreateInfoKHR::default()
			.surface(surface.handle())
			.min_image_count(image_count)
			.image_format(self.format.format)
			.image_color_space(self.format.color_space)
			.image_extent(extent)
			.image_array_layers(1)
			.image_usage(
				vk::ImageUsageFlags::COLOR_ATTACHMENT
					| vk::ImageUsageFlags::TRANSFER_DST
			)
			.image_sharing_mode(vk::SharingMode::EXCLUSIVE)
			.pre_transform(caps.current_transform)
			.composite_alpha(composite_alpha)
			.present_mode(present_mode)
			.clipped(true)
			.old_swapchain(old_swapchain);

		let new_swapchain = unsafe { self.loader.create_swapchain(&create_info, None)? };
		let images = unsafe { self.loader.get_swapchain_images(new_swapchain)? };

		let image_views: Result<Vec<_>, _> = images
			.iter()
			.map(|&image| ImageView::color_2d(self.device, image, Format(self.format.format.as_raw())))
			.collect();
		let image_views = image_views?;

		let new_render_semaphores: Result<Vec<_>, _> = (0..images.len())
			.map(|_| BinarySemaphore::new(self.device))
			.collect();
		let new_render_semaphores = new_render_semaphores?;

		let old_views = std::mem::replace(&mut self.image_views, image_views);
		let old_sems  = std::mem::replace(&mut self.render_semaphores, new_render_semaphores);

		let retired = RetiredSwapchainResources {
			loader: self.loader.clone(),
			handle: old_swapchain,
			_image_views: old_views,
			_render_semaphores: old_sems,
		};

		self.swapchain = new_swapchain;
		self.images = images;
		self.extent = extent;
		Ok(retired)
	}

	/// Indexed by image, not frame slot — presentation engine holds until re-acquire.
	pub(crate) fn render_semaphore(&self, image_index: u32) -> vk::Semaphore {
		self.render_semaphores[image_index as usize].handle()
	}

	pub(crate) fn format(&self) -> vk::Format { self.format.format }
	pub(crate) fn extent(&self) -> vk::Extent2D { self.extent }
	pub(crate) fn image(&self, index: u32) -> vk::Image { self.images[index as usize] }
	pub(crate) fn image_view(&self, index: u32) -> &ImageView<'_, VulkanBackend> { &self.image_views[index as usize] }
	pub(crate) fn _image_count(&self) -> u32 { self.images.len() as u32 }

	pub(crate) fn present_raw(
		&self,
		queue: vk::Queue,
		index: u32,
		wait_semaphore: vk::Semaphore,
	) -> Result<bool, vk::Result> {
		let swapchains = [self.swapchain];
		let indices = [index];
		let wait = [wait_semaphore];

		let present_info = vk::PresentInfoKHR::default()
			.wait_semaphores(&wait)
			.swapchains(&swapchains)
			.image_indices(&indices);

		unsafe {
			match self.loader.queue_present(queue, &present_info) {
				Ok(suboptimal) => Ok(suboptimal),
				Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => Ok(true),
				Err(e) => Err(e),
			}
		}
	}
}

impl Drop for Swapchain<'_> {
	fn drop(&mut self) {
		unsafe {
			// Views reference swapchain images — destroy before swapchain.
			self.image_views.clear();
			self.loader.destroy_swapchain(self.swapchain, None);
		}
	}
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn select_composite_alpha(caps: &vk::SurfaceCapabilitiesKHR) -> vk::CompositeAlphaFlagsKHR {
	if caps.supported_composite_alpha.contains(vk::CompositeAlphaFlagsKHR::PRE_MULTIPLIED) {
		vk::CompositeAlphaFlagsKHR::PRE_MULTIPLIED
	} else if caps.supported_composite_alpha.contains(vk::CompositeAlphaFlagsKHR::POST_MULTIPLIED) {
		vk::CompositeAlphaFlagsKHR::POST_MULTIPLIED
	} else {
		vk::CompositeAlphaFlagsKHR::OPAQUE
	}
}

fn resolve_extent(caps: &vk::SurfaceCapabilitiesKHR, width: u32, height: u32) -> vk::Extent2D {
	if caps.current_extent.width != u32::MAX {
		caps.current_extent
	} else {
		vk::Extent2D {
			width:  width.clamp(caps.min_image_extent.width,  caps.max_image_extent.width),
			height: height.clamp(caps.min_image_extent.height, caps.max_image_extent.height),
		}
		
	}
	
}

fn select_image_count(caps: &vk::SurfaceCapabilitiesKHR) -> u32 {
	(caps.min_image_count + 1).min(
		if caps.max_image_count == 0 { u32::MAX } else { caps.max_image_count }
	)
}