use crate::core::types::PushConstantRange;
use crate::core::PipelineId;
use crate::domain::DescriptorSetId;
use crate::domain::ResourceId;

#[derive(Debug)]
pub enum PassCommand {
	PushConstants {range: PushConstantRange, data:  Box<[u8]>},
	Draw { vertex_count: u32 },
	DrawIndexed { index_count: u32, instance_count: u32, first_index: u32 },
	Dispatch { x: u32, y: u32, z: u32 },
	CopyBuffer { src: ResourceId, dst: ResourceId, size: u64, dst_offset: u64 },
	CopyBufferToImage { src: ResourceId, dst: ResourceId},
	BindVertexBuffer(ResourceId, u64),
	BindIndexBuffer(ResourceId, u64),
	BindPipeline(PipelineId),
	BindDescriptorSet(DescriptorSetId),
}


/// Reinterpret a `#[repr(C), Copy]` value as raw bytes.
/// Used to fill PassCommand push constant data.
#[inline]
pub fn push_data<T: Copy>(data: &T) -> &[u8] {
	unsafe {
		std::slice::from_raw_parts(
			data as *const T as *const u8,
			std::mem::size_of::<T>(),
		)
	}
}