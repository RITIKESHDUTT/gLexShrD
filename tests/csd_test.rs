use glex_platform::csd::{Color, CsdTheme};
use gLexShrD::WaylandWindowImpl;
use glex_platform::platform::Platform;
use gLexShrD::VulkanContext;
use glex_platform::platform::WindowConfig;
use gLexShrD::WaylandPlatform;
use gLexShrD::Glex;

#[cfg(test)]
pub fn empty_window() {
	tracing_subscriber::fmt()
		.with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
		.init();
	let title = "gLexShrD Demo - Particle Vortex";
	let mut platform = WaylandPlatform::new().unwrap();
	let mut window = platform.create_window(WindowConfig::new(title, 1280, 720)).unwrap();
	
	let (ctx, surface) = VulkanContext::new::<WaylandWindowImpl>(&window).unwrap();
	let mut glex = Glex::app(&ctx, &surface, &window).expect("Failed");
	window.set_theme( CsdTheme { window_bg: Color::DARK_SLATE_GRAY, ..CsdTheme::default()});
	
	let app  = glex.run(&ctx, &mut window).expect("Failed");
	
}