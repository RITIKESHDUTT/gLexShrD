use crate::renderer::prelude::*;
use crate::renderer::pipelines::TextPush;

#[fragment_shader]
fn text_frag(
	#[binding(set = 0, binding = 0)] atlas: Sampler2D,     // descriptor
	#[location(0)] frag_uv: Vec2,        // in location = 0
	#[location(1)] frag_color: Vec4,     // in location = 1
	#[location(0, out)] out_color: Vec4,      // out location = 0
) {
	let alpha: f32 = texture(atlas, frag_uv).r;
	
	out_color = Vec4::new(
		frag_color.x,
		frag_color.y,
		frag_color.z,
		frag_color.w * alpha,
	);
}

// Push constant driven glyph vertex shader.
// Input: unit quad vertices [0,1]x[0,1].
// Push constants position the glyph in pixel coords and map UV from atlas.


#[vertex_shader]
fn text_vert(
	// Input: The vertex of the unit quad [0..1]
	#[location(0, in)] in_pos: Vec2,
	#[location(1, in)] in_uv: Vec2,
	#[push_constant(screen_size: Vec2, glyph_pos: Vec2, glyph_size: Vec2, uv_origin: Vec2,uv_size: Vec2,color: Vec4,)] pc: &TextPush,
	// Outputs: Values passed to the Fragment Shader
	#[location(0, out)] mut frag_uv:    Vec2,
	#[location(1, out)] mut frag_color: Vec4,
	// Built-in: The final coordinate for the GPU
	#[builtin(position)] mut position:  Vec4,
) {
	// 1. Calculate the pixel coordinate (Top-Left + Offset)
	let pixel: Vec2 = pc.glyph_pos + (in_pos * pc.glyph_size);
	
	// 2. Map Pixels to NDC Space (-1.0 to 1.0)
	let ndc: Vec2 = (pixel / pc.screen_size) * 2.0 - 1.0;
	
	// 3. Map Unit UV [0..1] to Atlas UV [origin..origin+size]
	let uv: Vec2 = pc.uv_origin + (in_uv * pc.uv_size);
	
	// 4. Assign to outputs
	position   = vec4(ndc, 0.0, 1.0);
	frag_uv    = uv;
	frag_color = pc.color;
}

pub const VERT_SHADER: &str = TEXT_VERT_GLSL;
pub const FRAG_SHADER: &str = TEXT_FRAG_GLSL;


pub static VERT_SPV:    &[u8] = TEXT_VERT_SPV;
pub static FRAG_SPV:    &[u8] = TEXT_FRAG_SPV;