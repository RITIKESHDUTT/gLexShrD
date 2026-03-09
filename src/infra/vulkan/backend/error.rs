use crate::domain::GraphError;

impl From<ash::vk::Result> for GraphError {
	fn from(e: ash::vk::Result) -> Self {
		GraphError::BackendFailure(e.to_string())
	}
}