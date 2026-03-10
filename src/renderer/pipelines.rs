mod client_decor;
mod text;
mod csdpipeline;

pub use client_decor::create_rect_pipeline;
pub use client_decor::RectPush;
pub(crate) use client_decor::RECT_PUSH_RANGE;
pub use csdpipeline::CsdPipelines;
pub use text::TextPush;
pub(crate) use text::TEXT_PUSH_RANGE;
pub use text::{create_text_pipeline, GlyphAtlas, TextSet};
