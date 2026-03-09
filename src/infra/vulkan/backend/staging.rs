// use crate::core::buf_state;
// use crate::Buffer;
// use ash::vk;
// use crate::infra::vulkan::backend::VulkanDevice;
// use super::{VulkanInstance, PhysicalDevice, find_memory_type};
// /// Host-visible staging buffer for CPU → GPU data transfer.
// ///
// /// Mapped persistently on creation. Write data with `write()`,
// /// then use the raw handle with copy commands.
// pub struct StagingBuffer<'dev> {
// 	device: &'dev VulkanDevice,
// 	buffer: vk::Buffer,
// 	memory: vk::DeviceMemory,
// 	size: vk::DeviceSize,
// 	mapped: *mut u8,
// 	family: u32,
// }
//
// impl<'dev> StagingBuffer<'dev> {
// 	pub fn new(
// 		device: &'dev VulkanDevice,
// 		instance: &VulkanInstance,
// 		physical: &PhysicalDevice,
// 		size: vk::DeviceSize,
// 		family: u32,
// 	) -> Result<Self, vk::Result> {
// 		let buffer_info = vk::BufferCreateInfo::default()
// 			.size(size)
// 			.usage(vk::BufferUsageFlags::TRANSFER_SRC)
// 			.sharing_mode(vk::SharingMode::EXCLUSIVE);
//
// 		let buffer = unsafe {
// 			device.create_buffer(&buffer_info, None)?
// 		};
//
// 		let mem_req = unsafe {
// 			device.get_buffer_memory_requirements(buffer)
// 		};
//
// 		let memory_type_index = find_memory_type(
// 			instance,
// 			physical,
// 			mem_req,
// 			vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
// 		).ok_or(vk::Result::ERROR_FEATURE_NOT_PRESENT)?;
//
// 		let alloc_info = vk::MemoryAllocateInfo::default()
// 			.allocation_size(mem_req.size)
// 			.memory_type_index(memory_type_index);
//
// 		let memory = unsafe {
// 			device.allocate_memory(&alloc_info, None)?
// 		};
//
// 		unsafe {
// 			device.bind_buffer_memory(buffer, memory, 0)?;
// 		}
//
// 		let mapped = unsafe {
// 			device.map_memory(memory, 0, size, vk::MemoryMapFlags::empty())?
// 		} as *mut u8;
//
// 		Ok(Self { device, buffer, memory, size, mapped, family })
// 	}
//
// 	/// Copy data from CPU memory into the staging buffer.
// 	///
// 	/// Panics if data exceeds buffer size.
// 	pub fn write<T: Copy>(&self, data: &[T]) {
// 		let byte_len = std::mem::size_of_val(data) as vk::DeviceSize;
// 		assert!(byte_len <= self.size, "data exceeds staging buffer size");
//
// 		unsafe {
// 			std::ptr::copy_nonoverlapping(
// 				data.as_ptr() as *const u8,
// 				self.mapped,
// 				byte_len as usize,
// 			);
// 		}
// 	}
//
// 	/// Raw Vulkan buffer handle for use with copy commands.
// 	pub fn handle(&self) -> vk::Buffer {
// 		self.buffer
// 	}
//
// 	pub fn size(&self) -> vk::DeviceSize {
// 		self.size
// 	}
//
// 	// Since as_typed() creates a Buffer that borrows from StagingBuffer,
// 	// make sure the StagingBuffer isn't dropped until the GPU finishes the copy.
// 	// You should keep the StagingBuffer alive until the end of the upload function.
// 	pub fn as_typed(&self) -> Buffer<'dev, buf_state::TransferSrc> {
// 		Buffer::from_raw_view(
// 			self.device,
// 			self.buffer,
// 			self.size,
// 			self.family,
// 		)
// 	}
// }
//
// // SAFETY: The mapped pointer is only accessed through &self methods,
// // and the buffer/memory lifetime is tied to the &'dev Device.
// unsafe impl Send for StagingBuffer<'_> {}
// unsafe impl Sync for StagingBuffer<'_> {}
//
// impl Drop for StagingBuffer<'_> {
// 	fn drop(&mut self) {
// 		unsafe {
// 			self.device.unmap_memory(self.memory);
// 			self.device.destroy_buffer(self.buffer, None);
// 			self.device.free_memory(self.memory, None);
// 		}
// 	}
// }