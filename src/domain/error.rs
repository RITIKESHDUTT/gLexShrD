use crate::domain::PassDomain;
use super::ResourceId;

#[derive(thiserror::Error, Debug)]
pub enum GraphError {
	#[error("Graph contains a cycle")]
	CycleDetected,
	#[error("Resource {0:?} not found")]
	ResourceNotFound(ResourceId),
	#[error("No lane attached for domain {0:?}")]
	MissingLane(PassDomain),
	#[error("Internal rendering backend error: {0}")]
	BackendFailure(String),
}

impl GraphError {
	pub fn backend(e: impl std::fmt::Display) -> Self {
		Self::BackendFailure(e.to_string())
	}
}