use macroquad::material::{gl_use_default_material, gl_use_material, load_material};
use macroquad::prelude::*;

// GLSL vertex shader for macroquad materials: transforms the quad with the
// Model/Projection matrices and passes the texture coordinates along.
const CRT_VERTEX_SHADER: &str = r#"
#version 100

attribute vec3 position;
attribute vec2 texcoord;

uniform mat4 Model;
uniform mat4 Projection;

varying vec2 v_tex_coords;

void main() {
    v_tex_coords = texcoord;
    gl_Position = Projection * Model * vec4(position, 1.0);
}
"#;

// GLSL fragment shader for CRT monitor effect.
// - Horizontal scanlines (soft, target-aligned)
// - Light vignette on edges
const CRT_FRAGMENT_SHADER: &str = r#"
#version 100
precision highp float;

varying vec2 v_tex_coords;

uniform sampler2D Texture;
// Window size in logical pixels (used only for vignette consistency)
uniform vec2 resolution;
// Render target size in texels
uniform vec2 target_size;

float vignette(float strength) {
    vec2 centered = v_tex_coords * 2.0 - 1.0;
    float r2 = dot(centered, centered);
    return pow(max(0.0, 1.0 - r2), strength);
}

void main() {
    vec4 tex_color = texture2D(Texture, v_tex_coords);

    // Target-aligned scanlines: one cycle spans SCANLINES_PER_CYCLE
    // horizontal rows of the low-res render target. This keeps the
    // dark bands aligned to whole texel rows and avoids slicing voxels
    // when upscaling to any window size.
    const float SCANLINES_PER_CYCLE = 3.0;
    float period = max(1.0, target_size.y / SCANLINES_PER_CYCLE);
    float phase = fract(v_tex_coords.y * target_size.y / period);

    // Very soft modulation (0.96..1.0) — removes the "aggressive" CRT look
    float scanline = 0.96 + 0.04 * cos(phase * 6.28318530718);

    // Subtler vignette to avoid dark corners crushing detail
    float intensity = mix(0.8, 1.0, vignette(1.8));

    gl_FragColor = vec4(tex_color.rgb * intensity * scanline, tex_color.a);
}
"#;

/// CRT post-processing filter that applies a CRT monitor effect to the rendered scene.
/// The scene is rendered into a low-resolution render target, which is then drawn
/// to the screen with the CRT shader applied, creating a scanline/vignette look.
pub struct CrtFilter {
    /// Low-resolution target the scene is rendered into.
    render_target: RenderTarget,
    /// Material for the CRT effect.
    material: macroquad::material::Material,
    /// Width of the render target in pixels.
    width: u32,
    /// Height of the render target in pixels.
    height: u32,
}

impl CrtFilter {
    /// Creates a new CRT filter with the given render target dimensions.
    pub fn new(width: u32, height: u32) -> Self {
        // The scene is rendered into this low-resolution target and then
        // upscaled to the screen by the CRT shader.
        //
        // CRITICAL: the target MUST have a depth attachment. The shortcut
        // `render_target()` creates one with `depth: false`; rendering a
        // Camera3D scene into it then performs no depth testing at all
        // (there is no depth buffer to test against), so faces are painted
        // in submission order and nearer/farther geometry overwrites each
        // other - the "blocks are invisible / cut in half" artefact.
        let render_target = render_target_ex(
            width,
            height,
            RenderTargetParams {
                sample_count: 1,
                depth: true,
            },
        );
        render_target.texture.set_filter(FilterMode::Nearest);

        let material = load_material(
            ShaderSource::Glsl {
                vertex: CRT_VERTEX_SHADER,
                fragment: CRT_FRAGMENT_SHADER,
            },
            MaterialParams {
                pipeline_params: Default::default(),
                uniforms: vec![
                    UniformDesc {
                        name: "resolution".to_string(),
                        uniform_type: UniformType::Float2,
                        array_count: 1,
                    },
                    UniformDesc {
                        name: "target_size".to_string(),
                        uniform_type: UniformType::Float2,
                        array_count: 1,
                    },
                ],

                ..Default::default()
            },
        )
        .expect("Failed to create CRT material");

        Self {
            render_target,
            material,
            width,
            height,
        }
    }

    /// Pixel dimensions of the low-resolution render target, in texels.
    ///
    /// Draw calls that target the RT (the procedural sky) must be sized
    /// with these values, **not** with `screen_width()`/`screen_height()`:
    /// those always report the window size even when a render target is
    /// bound, which would make a full-screen background only cover a
    /// fraction of the buffer and leave the rest black.
    pub fn target_size(&self) -> (f32, f32) {
        (self.width as f32, self.height as f32)
    }

    /// Points a 2D camera at the filter's low-resolution render target so
    /// screen-space effects (e.g. the procedural sky gradient) can be
    /// drawn *behind* the 3D scene: call this, draw the background, then
    /// call `begin_render` for the world.
    ///
    /// Camera2D disables the depth test/write, so these draws can never
    /// occlude the 3D pass that follows.
    pub fn begin_ui_pass(&self) {
        // Camera2D must cover the *entire* render target rect (pixel space),
        // not the window. Using from_display_rect with target dims avoids the
        // half-frame split when drawing to a lower-res render target.
        let mut camera =
            Camera2D::from_display_rect(Rect::new(0.0, 0.0, self.width as f32, self.height as f32));
        camera.render_target = Some(self.render_target.clone());
        set_camera(&camera);
    }

    /// Points the camera at the filter's low-resolution render target.
    ///
    /// The scene must be drawn after this call and before `draw_to_screen`.
    pub fn begin_render(&self, mut camera: Camera3D) {
        camera.render_target = Some(self.render_target.clone());
        set_camera(&camera);
    }

    /// Draws the stored render target with the CRT shader applied.
    pub fn draw_to_screen(&self) {
        // Back to the default camera so this pass is drawn to the screen.
        set_default_camera();

        // Use our custom material
        gl_use_material(&self.material);

        // `resolution` must be the *window* size (logical pixels): the
        // scanline pitch is anchored to real screen rows. It is refreshed
        // every frame so resizing the window (or a DPI change) keeps the
        // lines aligned. NOTE: pass the Vec2 by value - a reference would
        // also be 8 bytes and pass set_uniform's size check, copying the
        // pointer instead of the data.
        let res = vec2(screen_width(), screen_height());
        self.material.set_uniform("resolution", res);
        self.material
            .set_uniform("target_size", vec2(self.width as f32, self.height as f32));

        // Draw a full-screen rectangle with the render target stretched over it.
        //
        // flip_y is required: a scene rendered with Camera3D into a render
        // target comes out vertically flipped (macroquad's Camera2D compensates
        // for this internally, but Camera3D does not), so it must be flipped
        // back when blitting it to the screen.
        draw_texture_ex(
            &self.render_target.texture,
            0.0,
            0.0,
            WHITE,
            DrawTextureParams {
                dest_size: Some(vec2(screen_width(), screen_height())),
                flip_y: true,
                ..Default::default()
            },
        );

        // Reset to default material
        gl_use_default_material();
    }
}
