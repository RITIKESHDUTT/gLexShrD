use crate::core::exec::BarrierEdge;
use crate::domain::PassDomain;
use crate::domain::ResourceId;
use crate::domain::UsageIntent;

/// Barrier with queue ownership resolved.
/// No Vulkan types — just domain intents + queue family indices.
#[derive(Debug, Clone, Copy)]
pub struct BarrierDesc {
	pub resource: ResourceId,
	pub src_usage: UsageIntent,
	pub dst_usage: UsageIntent,
	pub src_queue_family: u32,
	pub dst_queue_family: u32,
}

pub fn resolve_barrier(
	edge: &BarrierEdge,
	domain_to_family: impl Fn(PassDomain) -> u32,
) -> BarrierDesc {
	let src = domain_to_family(edge.src_domain);
	let dst = domain_to_family(edge.dst_domain);
	let (src_qf, dst_qf) = if src == dst {
		(u32::MAX, u32::MAX)  // same queue — no ownership transfer
	} else {
		(src, dst)
	};
	BarrierDesc {
		resource: edge.resource,
		src_usage: edge.src_usage,
		dst_usage: edge.dst_usage,
		src_queue_family: src_qf,
		dst_queue_family: dst_qf,
	}
}