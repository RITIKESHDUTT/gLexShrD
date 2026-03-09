mod pipelines;
mod shader_utils;
mod prelude;
mod shaders;
mod windowed;

pub use pipelines::{CsdPipelines, TextSet, TextPush, RectPush};
pub use shader_utils::{UNIT_QUAD, Vertex2D,};
pub use windowed::{CsdResources, record_csd};
pub use pipelines::GlyphAtlas;