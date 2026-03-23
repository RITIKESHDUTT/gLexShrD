use crate::core::types::{Extent2D, Extent3D};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceKind {
	Buffer,
	Image,
}


#[derive(Debug, Clone, Copy)]
pub enum ResourceHandle {
	Buffer {
		raw: u64,
		offset: u64,
		size: u64,
	},
	Image {
		raw: u64,
		extent: Extent3D,
	},
}
pub struct ResourceDecl{
	pub id: super::ResourceId,
	pub kind: ResourceKind,
	pub handle: ResourceHandle,
}