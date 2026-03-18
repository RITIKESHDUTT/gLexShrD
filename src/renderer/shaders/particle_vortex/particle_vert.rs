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
	
	out_position   = vec4(pos.x / params.aspect_ratio, pos.y, 0.0, 1.0);
	out_point_size = clamp(mix(3.0, 2.0, clamp(dist * 2.0, 0.0, 1.0)), 3.5, 6.0);
	
	let core_color  = vec3(1.0,  0.95, 0.85);
	let inner_color = vec3(1.0,  0.7,  0.35);
	let mid_color   = vec3(0.5,  0.65, 1.0);
	let outer_color = vec3(0.3,  0.35, 0.8);
	
	let d = clamp(dist * 2.5, 0.0, 1.0);
	let mut rgb = vec3(0.0, 0.0, 0.0);
	if d < 0.15 {
		rgb = core_color.mix(inner_color, d / 0.15);
	} else if d < 0.45 {
		rgb = inner_color.mix(mid_color, (d - 0.15) / 0.3);
	} else {
		rgb = mid_color.mix(outer_color, (d - 0.45) / 0.55);
	}
	
	// Brighten fast-moving particles
	rgb *= 0.8 + clamp(speed * 3.0, 0.0, 0.8);
	
	// Fade slow particles slightly
	let alpha = mix(0.6, 0.9, clamp(speed * 4.0, 0.0, 1.0));
	
	v_color = vec4_v3f(rgb, alpha);
}

pub static VERT_SPV: &[u8] = PARTICLE_VERT_SPV;
pub const VERT_SHADER: &str = PARTICLE_VERT_GLSL;