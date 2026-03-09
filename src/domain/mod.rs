mod error;
mod resource;
mod types;

pub use error::GraphError;
pub use types::*;
pub use resource::{
	ResourceDecl,
	ResourceHandle,
	ResourceKind
};
