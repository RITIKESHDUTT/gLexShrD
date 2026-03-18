use glex_shader_types::f32i3f;
use glex_shader_types::Vec2i2f;
use glex_shader_types::Vec2i1;
use glex_shader_types::Vec4i0;
use glex_shader_types::f32o3f;
use glex_shader_types::Vec2o2f;
use glex_shader_types::Vec2o1;
use glex_shader_types::Vec4o0;
use crate::renderer::pipelines::RectPush;
use crate::renderer::prelude::*;



#[vertex_shader(vertex = Vertex2D)]
fn csd_vert(
	input_position: Vec2,
	input_texture: Vec2,
	#[push_constant] params: &RectPush,
	mut frag_color: Vec4o0,
	mut v_uv: Vec2o1,
	mut v_size: Vec2o2f,
	mut v_radius:f32o3f,
	mut out_position: Vec4,
) {
	let pixel: Vec2 = params.rect_pos + input_position * params.rect_size;
	let ndc:   Vec2 = (pixel / params.screen_size) * 2.0 - 1.0;
	
	out_position = vec4(ndc, 0.0, 1.0);
	frag_color   = params.color;
	v_uv         = input_texture;
	v_size       = params.rect_size;
	v_radius     = params.radius;
}


#[shader_fn]
fn sd_rounded_box(p: Vec2, half_size: Vec2, r: f32) -> f32 {
	let q: Vec2 = p.abs() - (half_size - vec2(r, r));
	q.max_f(0.0).length()
		+ min(max(q.x, q.y), 0.0)
		- r
}
#[fragment_shader]
fn csd_frag(
	frag_color: Vec4i0,
	v_uv: Vec2i1,
	v_size: Vec2i2f,
	v_radius: f32i3f,
	mut out_color: Vec4o0,
) {
	let half_size: Vec2 = v_size * 0.5;
	let p: Vec2 = (v_uv - vec2(0.5, 0.5)) * v_size;
	
	let r: f32 = clamp(v_radius, 0.0, min(half_size.x, half_size.y));
	
	let q: Vec2 = p.abs() - (half_size - vec2(r, r));
	let d: f32 =
		q.max_f(0.0).length()
			+ min(max(q.x, q.y), 0.0)
			- r;
	
	let alpha: f32 = 1.0 - smoothstep(-1.0, 1.0, d);
	
	if alpha <= 0.0 {
		out_color = vec4(0.0, 0.0, 0.0, 0.0);
		return;
	}
	
	let mut color: Vec4 = frag_color;
	color.a *= alpha;
	color = vec4_v3f(color.rgb() * color.a, color.a);
	
	out_color = color;
}


pub const  VERTEX_SHADER: &str = CSD_VERT_GLSL;
pub const FRAGMENT_SHADER:&str = CSD_FRAG_GLSL;

pub static VERT_SPV:    &[u8] = CSD_VERT_SPV;
pub static FRAG_SPV:    &[u8] = CSD_FRAG_SPV;