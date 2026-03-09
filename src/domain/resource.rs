#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceKind {
	Buffer,
	Image,
}


#[derive(Debug, Clone, Copy)]
pub enum ResourceHandle {
	Image(u64),
	Buffer(u64),
}

pub struct ResourceDecl{
	pub id: super::ResourceId,
	pub kind: ResourceKind,
	pub handle: ResourceHandle,
}