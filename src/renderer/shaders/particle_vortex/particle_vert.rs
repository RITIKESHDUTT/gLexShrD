use glex_shader_types::Vec4o0;
use crate::renderer::prelude::*;
use crate::renderer::shaders::GfxPush;

#[vertex_shader]
fn particle_vert(
	#[storage(set = 0, binding = 0, read)] particles: &[Vec4],
	#[push_constant] params: &GfxPush,
	#[builtin(vertex_index)] vertex_id: u32,
	mut out_position: Vec4,
	mut out_point_size: f32,
	mut v_color: Vec4o0,
) {
	let p: Vec4   = particles[vertex_id as usize];
	let pos: Vec2 = p.xy();
	let vel: Vec2 = p.zw();
	
	let speed = vel.length();
	let dist  = pos.length();
	
	let aspect = params.surface_extent.x / params.surface_extent.y;
	out_position = vec4(pos.x / aspect, pos.y, 0.0, 1.0);
	
	// Dense starfield — small points
	out_point_size = clamp(1.5 + dist * 3.0, 1.5, 4.5);
	
	// ── Galaxy palette: warm nucleus → blue-white disk → blue edge ──────
	let nucleus = vec3(1.0,  0.9,  0.7);
	let inner   = vec3(1.0,  0.85, 0.6);
	let mid     = vec3(0.8,  0.85, 1.0);
	let outer   = vec3(0.4,  0.5,  0.95);
	
	let d = clamp(dist * 1.3, 0.0, 1.0);
	let mut rgb = vec3(0.0, 0.0, 0.0);
	if d < 0.1 {
		rgb = nucleus.mix(inner, d / 0.1);
	} else if d < 0.4 {
		rgb = inner.mix(mid, (d - 0.1) / 0.3);
	} else {
		rgb = mid.mix(outer, (d - 0.4) / 0.6);
	}
	
	// Slight speed boost
	rgb = rgb * (0.7 + clamp(speed * 2.0, 0.0, 0.6));
	
	// Fade slow particles slightly
	let alpha = mix(0.6, 0.9, clamp(speed * 4.0, 0.0, 1.0));
	
	v_color = vec4_v3f(rgb, alpha);
}

pub static VERT_SPV: &[u8] = PARTICLE_VERT_SPV;
pub const VERT_SHADER: &str = PARTICLE_VERT_GLSL;