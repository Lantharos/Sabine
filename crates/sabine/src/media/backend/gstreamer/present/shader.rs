use std::num::NonZeroU32;

use glow::HasContext;

use crate::media::geometry::Frame;

const VERTEX: &str = r#"#version 330 core
const vec2 CORNERS[3] = vec2[3](vec2(-1.0, -1.0), vec2(3.0, -1.0), vec2(-1.0, 3.0));
void main() {
    gl_Position = vec4(CORNERS[gl_VertexID], 0.0, 1.0);
}
"#;

const FRAGMENT: &str = r#"#version 330 core
uniform sampler2D picture_texture;
uniform float surface_height;
uniform vec4 bounds;
uniform vec4 picture;
uniform vec2 spread;
uniform float radius;
out vec4 color;

float coverage(vec2 point) {
    vec2 half_size = bounds.zw * 0.5;
    float corner_radius = min(radius, min(half_size.x, half_size.y));
    vec2 corner = abs(point - bounds.xy - half_size) - half_size + vec2(corner_radius);
    float distance = length(max(corner, 0.0)) + min(max(corner.x, corner.y), 0.0) - corner_radius;
    return clamp(0.5 - distance, 0.0, 1.0);
}

vec3 sample_picture(vec2 uv) {
    if (spread.x <= 0.0) {
        return texture(picture_texture, uv).rgb;
    }
    return 0.25 * (texture(picture_texture, uv - spread).rgb
        + texture(picture_texture, uv + spread).rgb
        + texture(picture_texture, uv + vec2(spread.x, -spread.y)).rgb
        + texture(picture_texture, uv + vec2(-spread.x, spread.y)).rgb);
}

void main() {
    vec2 point = vec2(gl_FragCoord.x, surface_height - gl_FragCoord.y);
    float alpha = coverage(point);
    vec2 uv = (point - picture.xy) / picture.zw;
    vec3 rgb = vec3(0.0);
    if (picture.z > 0.0 && all(greaterThanEqual(uv, vec2(0.0))) && all(lessThanEqual(uv, vec2(1.0)))) {
        rgb = sample_picture(uv);
    }
    color = vec4(rgb * alpha, alpha);
}
"#;

/// Downscaling past this ratio averages several taps to avoid shimmering.
const SUPERSAMPLE_RATIO: f32 = 1.5;

/// A decoded picture ready to sample.
#[derive(Clone, Copy)]
pub(super) struct Picture {
    pub(super) texture: u32,
    pub(super) texture_size: (i32, i32),
    pub(super) display_size: (f64, f64),
}

pub(super) struct VideoProgram {
    program: glow::Program,
    vertex_array: glow::VertexArray,
    sampler: glow::Sampler,
    surface_height: Option<glow::UniformLocation>,
    bounds: Option<glow::UniformLocation>,
    picture: Option<glow::UniformLocation>,
    spread: Option<glow::UniformLocation>,
    radius: Option<glow::UniformLocation>,
}

impl VideoProgram {
    pub(super) fn new(gl: &glow::Context) -> Result<Self, String> {
        unsafe {
            let program = gl.create_program()?;
            let mut shaders = Vec::new();
            for (kind, source) in [
                (glow::VERTEX_SHADER, VERTEX),
                (glow::FRAGMENT_SHADER, FRAGMENT),
            ] {
                let shader = gl.create_shader(kind)?;
                gl.shader_source(shader, source);
                gl.compile_shader(shader);
                if !gl.get_shader_compile_status(shader) {
                    return Err(gl.get_shader_info_log(shader));
                }
                gl.attach_shader(program, shader);
                shaders.push(shader);
            }
            gl.link_program(program);
            for shader in shaders {
                gl.detach_shader(program, shader);
                gl.delete_shader(shader);
            }
            if !gl.get_program_link_status(program) {
                return Err(gl.get_program_info_log(program));
            }
            let sampler = gl.create_sampler()?;
            for (name, value) in [
                (glow::TEXTURE_MIN_FILTER, glow::LINEAR),
                (glow::TEXTURE_MAG_FILTER, glow::LINEAR),
                (glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE),
                (glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE),
            ] {
                gl.sampler_parameter_i32(sampler, name, value as i32);
            }
            gl.use_program(Some(program));
            gl.uniform_1_i32(
                gl.get_uniform_location(program, "picture_texture").as_ref(),
                0,
            );
            Ok(Self {
                vertex_array: gl.create_vertex_array()?,
                sampler,
                surface_height: gl.get_uniform_location(program, "surface_height"),
                bounds: gl.get_uniform_location(program, "bounds"),
                picture: gl.get_uniform_location(program, "picture"),
                spread: gl.get_uniform_location(program, "spread"),
                radius: gl.get_uniform_location(program, "radius"),
                program,
            })
        }
    }

    pub(super) fn draw(&self, gl: &glow::Context, frame: &Frame, picture: Option<Picture>) {
        let (width, height) = frame.buffer;
        let [x, y, bounds_width, bounds_height] = frame.video;
        let (placed, spread) = picture
            .map(|picture| fit(picture, frame.video))
            .unwrap_or(([0.0; 4], [0.0; 2]));
        unsafe {
            gl.viewport(0, 0, width, height);
            gl.use_program(Some(self.program));
            gl.bind_vertex_array(Some(self.vertex_array));
            gl.uniform_1_f32(self.surface_height.as_ref(), height as f32);
            gl.uniform_4_f32(self.bounds.as_ref(), x, y, bounds_width, bounds_height);
            gl.uniform_4_f32(
                self.picture.as_ref(),
                placed[0],
                placed[1],
                placed[2],
                placed[3],
            );
            gl.uniform_2_f32(self.spread.as_ref(), spread[0], spread[1]);
            gl.uniform_1_f32(self.radius.as_ref(), frame.radius);
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(
                glow::TEXTURE_2D,
                picture
                    .and_then(|picture| NonZeroU32::new(picture.texture))
                    .map(glow::NativeTexture),
            );
            gl.bind_sampler(0, Some(self.sampler));
            gl.draw_arrays(glow::TRIANGLES, 0, 3);
        }
    }
}

fn fit(picture: Picture, bounds: [f32; 4]) -> ([f32; 4], [f32; 2]) {
    let [x, y, width, height] = bounds;
    let aspect = (picture.display_size.0 / picture.display_size.1) as f32;
    let (placed_width, placed_height) = if width / height > aspect {
        (height * aspect, height)
    } else {
        (width, width / aspect)
    };
    let placed = [
        x + (width - placed_width) * 0.5,
        y + (height - placed_height) * 0.5,
        placed_width,
        placed_height,
    ];
    let ratio = (picture.texture_size.0 as f32 / placed_width)
        .max(picture.texture_size.1 as f32 / placed_height);
    let spread = if ratio > SUPERSAMPLE_RATIO {
        [0.25 / placed_width, 0.25 / placed_height]
    } else {
        [0.0; 2]
    };
    (placed, spread)
}
