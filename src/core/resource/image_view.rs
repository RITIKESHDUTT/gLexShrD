use crate::core::backend::types::{Format, ImageAspect};
use crate::core::backend::{Backend, DeviceOps};

pub struct ImageView<'dev, B: Backend> {
	device: &'dev B::Device,
	handle: B::ImageView,
}

impl<'dev, B: Backend> ImageView<'dev, B> {
	pub fn create_2d(
		device: &'dev B::Device,
		image: B::Image,
		format: Format,
		aspect: ImageAspect,
	) -> Result<Self, B::Error> {
		let handle = device.create_image_view_2d(image, format, aspect)?;
		Ok(Self { device, handle })
	}
	
	pub fn color_2d(
		device: &'dev B::Device,
		image: B::Image,
		format: Format,
	) -> Result<Self, B::Error> {
		Self::create_2d(device, image, format, ImageAspect::Color)
	}
	
	pub fn depth_2d(
		device: &'dev B::Device,
		image: B::Image,
		format: Format,
	) -> Result<Self, B::Error> {
		Self::create_2d(device, image, format, ImageAspect::Depth)
	}
	
	pub fn handle(&self) -> B::ImageView {
		self.handle
	}
}

impl<B: Backend> Drop for ImageView<'_, B> {
	fn drop(&mut self) {
		self.device.destroy_image_view(self.handle);
	}
}