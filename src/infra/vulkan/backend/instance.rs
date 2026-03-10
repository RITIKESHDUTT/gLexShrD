use super::entry::VulkanEntry;
use ash::vk;
use std::sync::Arc;

pub struct VulkanInstance {
	instance: ash::Instance,
	_entry: Arc<VulkanEntry>,
}

impl VulkanInstance {
	/// Create instance with no extensions (headless / compute only).
	pub fn new(entry: Arc<VulkanEntry>) -> Result<Self, vk::Result> {
		Self::with_extensions(entry, &[])
	}
	
	/// Create instance with explicit extension names.
	///
	/// For surface presentation, pass `Surface::required_extensions()`.
	pub fn with_extensions(
		entry: Arc<VulkanEntry>,
		extensions: &[*const i8],
	) -> Result<Self, vk::Result> {
		let app_info = vk::ApplicationInfo::default()
			.application_name(c"GLexShrd")
			.api_version(vk::API_VERSION_1_3);
		
		let info = vk::InstanceCreateInfo::default()
			.application_info(&app_info)
			.enabled_extension_names(extensions);
		
		let instance = unsafe {
			entry.entry_handle().create_instance(&info, None)?
		};
		
		Ok(Self { instance, _entry: entry })
	}
	
	pub fn instance(&self) -> &ash::Instance {
		&self.instance
	}
	
	pub fn entry(&self) -> &VulkanEntry {
		&self._entry
	}
}

impl Drop for VulkanInstance {
	fn drop(&mut self) {
		unsafe {
			self.instance.destroy_instance(None);
		}
	}
}