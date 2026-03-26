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
	let p0: Vec4 = particles[vertex_id as usize * 2];
	let p1: Vec4 = particles[vertex_id as usize * 2 + 1];
	let pos = vec3(p0.x, p0.y, p0.z);
	let vel = vec3(p1.x, p1.y, p1.z);
	
	let speed = vel.length();
	let dist  = vec2(pos.x, pos.y).length();
	let aspect = params.surface_extent.x / params.surface_extent.y;
	
	// World: xy = disk plane, z = thickness
	// Remap to view: x→x, z→y (up), y→z (depth)
	let wx = pos.x;
	let wy = pos.z;
	let wz = pos.y;
	
	// Camera orbit (Y-axis)
	let orbit = params.time * 0.3;
	let ca = cos(orbit);
	let sa = sin(orbit);
	let rx =  wx * ca + wz * sa;
	let rz = -wx * sa + wz * ca;
	
	// Tilt ~35 degrees (X-axis)
	let tilt = 0.55;
	let ct = cos(tilt);
	let st = sin(tilt);
	let fy = wy * ct - rz * st;
	let fz = wy * st + rz * ct;
	
	// Perspective
	let cam_dist = 2.2;
	let z_eye = fz + cam_dist;
	let scale = 1.8 / z_eye;
	
	let px = rx * scale;
	let py = fy * scale;
	
	// apply aspect symmetrically
	out_position = vec4(px, py * aspect, 0.5, 1.0);
	out_point_size = clamp(scale * 3.5, 1.0, 7.0);
	
	// ── Galaxy palette: warm nucleus → blue-white disk → blue edge ──
	let nucleus = vec3(1.0, 0.95, 0.85);
	let inner   = vec3(1.0, 0.7,  0.4);
	let mid     = vec3(0.5, 0.7,  1.0);
	let outer   = vec3(0.3, 0.4,  0.95);
	
	let d = clamp(dist * 1.5, 0.0, 1.0);
	let mut rgb = vec3(0.0, 0.0, 0.0);
	if d < 0.08 {
		rgb = nucleus.mix(inner, d / 0.08);
	} else if d < 0.3 {
		rgb = inner.mix(mid, (d - 0.08) / 0.22);
	} else {
		rgb = mid.mix(outer, (d - 0.3) / 0.7);
	}
	
	rgb = rgb * (0.6 + clamp(speed * 3.0, 0.0, 0.8));
	let alpha = mix(0.5, 1.0, clamp(speed * 3.0, 0.0, 1.0));
	
	v_color = vec4_v3f(rgb, alpha);
}

pub static VERT_SPV: &[u8] = PARTICLE_VERT_SPV;
pub const VERT_SHADER: &str = PARTICLE_VERT_GLSL;