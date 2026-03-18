use glex_platform::csd::{Color, CsdTheme};
use gLexShrD::WaylandWindowImpl;
use glex_platform::platform::Platform;
use gLexShrD::VulkanContext;
use glex_platform::platform::WindowConfig;
use gLexShrD::WaylandPlatform;
use gLexShrD::Glex;
use glex_platform::platform::{ControlFlow};

#[cfg(test)]
#[test]
pub fn empty_window() {
	let title = "gLexShrD Demo - Particle Vortex";
	let mut platform = WaylandPlatform::new().unwrap();
	let mut window = platform.create_window(WindowConfig::new(title, 1280, 720)).unwrap();
	
	let (ctx, surface) = VulkanContext::new::<WaylandWindowImpl>(&window).unwrap();
	
	let mut glex = Glex::app(&ctx, &surface, &window).expect("Failed");
	window.set_theme( CsdTheme { window_bg: Color::DARK_SLATE_GRAY, ..CsdTheme::default()});
	
	loop {
		let (control_flow, _events) = window.pump();
		if matches!(control_flow, ControlFlow::Exit) {
			break;
		}
		let frame = glex.begin_frame(&ctx, &mut window).expect("Failed");
		if let Some((graph, _info)) = frame {
			glex.end_frame(&ctx, &mut window, graph).expect("Failed");
		};
	}
	glex.gpu.device().wait_idle().ok();
}