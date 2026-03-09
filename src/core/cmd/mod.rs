mod pool;
mod buffer;


pub mod state {
	pub struct Initial;
	pub struct Recording;
	pub struct Executable;
	
	// Render scope: tracks whether we're inside or outside a render pass
	pub struct Outside;
	pub struct Inside;
}

pub use self::buffer::CommandBuffer;
pub use self::pool::CommandPool;
pub use state::{Executable, Initial, Recording, Outside, Inside};