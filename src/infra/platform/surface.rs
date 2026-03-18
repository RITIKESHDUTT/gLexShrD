use glex_platform::WaylandNative;
use crate::infra::vulkan::VulkanEntry;
use crate::infra::vulkan::VulkanInstance;
use ash::vk;
use std::sync::Arc;
/// Wraps a VkSurfaceKHR. Owns the surface handle and its extension loader.
pub struct Surface {
	surface: vk::SurfaceKHR,
	loader: ash::khr::surface::Instance,
	_instance: Arc<VulkanInstance>,
}

impl Surface {
	pub const REQUIRED_EXTENSIONS_XLIB: &[*const i8] = &[
		ash::khr::surface::NAME.as_ptr(),
		ash::khr::xlib_surface::NAME.as_ptr(),
	];
	
	pub const REQUIRED_EXTENSIONS_WAYLAND: &[*const i8] = &[
		ash::khr::surface::NAME.as_ptr(),
		ash::khr::wayland_surface::NAME.as_ptr(),
	];
	
	/// Instance extensions required for any surface presentation.
	pub fn required_extensions() -> Vec<*const i8> {
		vec![
			ash::khr::surface::NAME.as_ptr(),
		]
	}
	
	/// Instance extensions required for X11 (Xlib) surface presentation.
	pub fn required_extensions_xlib() -> Vec<*const i8> {
		vec![
			ash::khr::surface::NAME.as_ptr(),
			ash::khr::xlib_surface::NAME.as_ptr(),
		]
	}
	
	/// Create a surface from an X11 (Xlib) display and window.
	///
	/// # Safety
	/// `display` must be a valid `*mut Display` and `window` a valid X11 Window.
	pub unsafe fn from_xlib(
		entry: &VulkanEntry,
		instance:Arc<VulkanInstance>,
		display: *mut std::ffi::c_void,
		window: u64,
	) -> Result<Self, vk::Result> {
		let xlib_loader = ash::khr::xlib_surface::Instance::new(
			entry.entry_handle(),
			instance.instance(),
		);
		
		let create_info = vk::XlibSurfaceCreateInfoKHR::default()
			.dpy(display as *mut _)
			.window(window);
		
		let surface = unsafe{xlib_loader.create_xlib_surface(&create_info, None)? };
		
		let loader = ash::khr::surface::Instance::new(
			entry.entry_handle(),
			instance.instance(),
		);
		
		Ok(Self { surface, loader, _instance:instance})
	}
	
	pub fn handle(&self) -> vk::SurfaceKHR {
		self.surface
	}
	
	pub fn loader(&self) -> &ash::khr::surface::Instance {
		&self.loader
	}
	
	/// Check if a queue family supports presentation to this surface.
	pub unsafe fn supports_queue_family(
		&self,
		physical_device: vk::PhysicalDevice,
		queue_family_index: u32,
	) -> Result<bool, vk::Result> {
		unsafe {
			self.loader.get_physical_device_surface_support(
				physical_device,
				queue_family_index,
				self.surface,
			)
		}
	}
	/// Query surface capabilities for a physical device.
	pub unsafe fn capabilities(
		&self,
		physical_device: vk::PhysicalDevice,
	) -> Result<vk::SurfaceCapabilitiesKHR, vk::Result> {
		unsafe {
			self.loader.get_physical_device_surface_capabilities(
				physical_device,
				self.surface,
			)
		}
	}
	
	/// Query supported present modes for a physical device.
	pub unsafe fn present_modes(
		&self,
		physical_device: vk::PhysicalDevice,
	) -> Result<Vec<vk::PresentModeKHR>, vk::Result> {
		unsafe {
			self.loader.get_physical_device_surface_present_modes(
				physical_device,
				self.surface,
			)
		}
	}

	/// Query supported surface formats for a physical device.
	pub unsafe fn formats(
		&self,
		physical_device: vk::PhysicalDevice,
	) -> Result<Vec<vk::SurfaceFormatKHR>, vk::Result> {
		unsafe {
			self.loader.get_physical_device_surface_formats(
				physical_device,
				self.surface,
			)
		}
	}
	
	/// Instance extensions required for Wayland surface presentation.
	pub fn required_extensions_wayland() -> Vec<*const i8> {
		vec![
			ash::khr::surface::NAME.as_ptr(),
			ash::khr::wayland_surface::NAME.as_ptr(),
		]
	}
	pub fn required_extensions_wayland_slice() -> &'static [*const i8] {
		Self::REQUIRED_EXTENSIONS_WAYLAND
	}
	
	/// Create a surface from a Wayland display and surface.
	///
	/// # Safety
	/// `display` must be a valid `*mut wl_display`
	/// `surface` must be a valid `*mut wl_surface`
	
	pub unsafe fn from_wayland(
		entry: &VulkanEntry,
		instance: Arc<VulkanInstance>,
		native: WaylandNative,
	) -> Result<Self, vk::Result> {
		unsafe {
			let wayland_loader = ash::khr::wayland_surface::Instance::new(
				entry.entry_handle(),
				instance.instance(),
			);
			
			let create_info = vk::WaylandSurfaceCreateInfoKHR::default()
				.display(native.display as *mut _)
				.surface(native.surface as *mut _);
			
			let surface = wayland_loader.create_wayland_surface(&create_info, None)?;
			
			let loader = ash::khr::surface::Instance::new(
				entry.entry_handle(),
				instance.instance(),
			);
			
			Ok(Self { surface, loader, _instance: instance })
		}
	}
	pub fn from_wayland_window(
		entry: &VulkanEntry,
		instance: Arc<VulkanInstance>,
		native: WaylandNative,
	) -> Result<Self, vk::Result> {
		 unsafe { Self::from_wayland(entry, instance, native) }
	}
	
	
}

impl Drop for Surface {
	fn drop(&mut self) {
		unsafe {
			self.loader.destroy_surface(self.surface, None);
		}
	}
}