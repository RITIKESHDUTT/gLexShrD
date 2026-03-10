use ash::Entry;
/// Safe wrapper - owns Entry, guarantees it stays loaded
pub struct VulkanEntry {
	entry: Entry,
}

impl VulkanEntry {
	/// Safe to call - handles unsafe internally
	pub fn new() -> Result<Self, ash::LoadingError> {
		let entry = unsafe { Entry::load()? };
		Ok(Self { entry })
	}
	
	pub fn entry_handle(&self) -> &Entry {
		&self.entry
	}
}