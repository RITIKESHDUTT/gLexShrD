use crate::core::backend::types::{AttachmentLoadOp, AttachmentStoreOp, ClearValue, Extent2D, Offset2D, Rect2D};
use crate::core::backend::Backend;
use crate::core::backend::{ColorAttachment, DepthAttachment, RenderingDesc};
use crate::core::ImageView;
use crate::domain::ImageLayout;

pub struct RenderingInfoBuilder<B: Backend> {
	color_attachments: Vec<ColorAttachment<B>>,
	depth_attachment: Option<DepthAttachment<B>>,
	offset: Offset2D,
	extent: Extent2D,
}

impl<B: Backend> RenderingInfoBuilder<B> {
	pub fn new(offset: Offset2D, extent: Extent2D) -> Self {
		Self {
			color_attachments: Vec::new(),
			depth_attachment: None,
			offset,
			extent,
		}
	}
	
	pub fn color_clear(mut self, view: &ImageView<'_, B>, clear_color: [f32; 4]) -> Self {
		self.color_attachments.push(ColorAttachment {
			view: view.handle(),
			layout: ImageLayout::ColorAttachment,
			load_op: AttachmentLoadOp::CLEAR,
			store_op: AttachmentStoreOp::STORE,
			clear_value: ClearValue::Color(clear_color),
		});
		self
	}
	
	pub fn color_load(mut self, view: &ImageView<'_, B>) -> Self {
		self.color_attachments.push(ColorAttachment {
			view: view.handle(),
			layout: ImageLayout::ColorAttachment,
			load_op:  AttachmentLoadOp::LOAD,
			store_op: AttachmentStoreOp::STORE,
			clear_value: ClearValue::Color([0.0; 4]),
		});
		self
	}
	
	pub fn depth_clear(mut self, view: &ImageView<'_, B>, clear_depth: f32) -> Self {
		self.depth_attachment = Some(DepthAttachment {
			view: view.handle(),
			layout: ImageLayout::DepthAttachment,
			load_op:  AttachmentLoadOp::CLEAR,
			store_op: AttachmentStoreOp::STORE,
			clear_depth,
		});
		self
	}
	
	pub fn build(self) -> RenderingDesc<B> {
		let offset = self.offset;
		RenderingDesc {
			area: Rect2D::new(
				offset,
				self.extent,
			),
			color_attachments: self.color_attachments,
			depth_attachment: self.depth_attachment,
		}
	}
}