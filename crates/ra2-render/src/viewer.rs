//! A window showing a composed map: wgpu draws the map canvas as a texture
//! under a pan-and-zoom camera; toggling a layer recomposes it.
//!
//! Keys: arrows or drag to pan, scroll or `+`/`-` to zoom, `0` to reset,
//! `O` objects, `L` lighting, `S` shroud, `M` scaling mode, `F` (or
//! `F11`) fullscreen, `Esc` to quit.

use std::sync::Arc;

use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition};
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

use crate::canvas::Canvas;
use crate::compose::Layers;

const SHADER: &str = r"
struct Camera {
    // Texel at the window centre, zoom (screen pixels per texel), scaling
    // mode (0 nearest, 1 sharp bilinear).
    centre: vec2<f32>,
    zoom: f32,
    mode: f32,
    viewport: vec2<f32>,
    texture: vec2<f32>,
};
@group(0) @binding(0) var map: texture_2d<f32>;
@group(0) @binding(1) var map_sampler: sampler;
@group(0) @binding(2) var<uniform> camera: Camera;

@vertex
fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let p = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(p * 2.0 - 1.0, 0.0, 1.0);
}

@fragment
fn fs(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let texel = camera.centre + (pos.xy - camera.viewport * 0.5) / camera.zoom;
    if (texel.x < 0.0 || texel.y < 0.0 || texel.x >= camera.texture.x || texel.y >= camera.texture.y) {
        return vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }
    if (camera.mode < 0.5) {
        return textureLoad(map, vec2<i32>(floor(texel)), 0);
    }
    // Sharp bilinear: each texel is a flat square, blended with its
    // neighbour only across the last screen pixel at its edge.
    let t = texel - 0.5;
    let base = floor(t);
    let f = t - base;
    let edge = clamp((f - 0.5) * max(camera.zoom, 1.0) + 0.5, vec2<f32>(0.0), vec2<f32>(1.0));
    return textureSampleLevel(map, map_sampler, (base + edge + 0.5) / camera.texture, 0.0);
}
";

const SPRITE_SHADER: &str = r"
struct Frame {
    // The world texture's origin (world pixels) and size.
    origin: vec2<f32>,
    size: vec2<f32>,
};
@group(0) @binding(0) var atlas: texture_2d<u32>;
@group(0) @binding(1) var palettes: texture_2d<f32>;
@group(0) @binding(2) var<uniform> frame: Frame;

struct Out {
    @builtin(position) pos: vec4<f32>,
    @location(0) local: vec2<f32>,
    @location(1) @interpolate(flat) src: vec2<u32>,
    @location(2) @interpolate(flat) row: u32,
};

@vertex
fn vs(
    @builtin(vertex_index) v: u32,
    @location(0) dest: vec4<f32>,
    @location(1) src: vec2<u32>,
    @location(2) row: u32,
) -> Out {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(0.0, 1.0),
        vec2<f32>(0.0, 1.0), vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0),
    );
    let local = corners[v] * dest.zw;
    let texel = dest.xy + local - frame.origin;
    var o: Out;
    o.pos = vec4<f32>(texel.x / frame.size.x * 2.0 - 1.0, 1.0 - texel.y / frame.size.y * 2.0, 0.0, 1.0);
    o.local = local;
    o.src = src;
    o.row = row;
    return o;
}

@fragment
fn fs(o: Out) -> @location(0) vec4<f32> {
    let idx = textureLoad(atlas, o.src + vec2<u32>(floor(o.local)), 0).r;
    if (idx == 0u) {
        discard;
    }
    return textureLoad(palettes, vec2<u32>(idx, o.row), 0);
}

// Shadows darken what's beneath through the blend (by 127/255, which
// rounds to the CPU's halving for every byte); index 1 marks them.
@fragment
fn fs_shadow(o: Out) -> @location(0) vec4<f32> {
    let idx = textureLoad(atlas, o.src + vec2<u32>(floor(o.local)), 0).r;
    if (idx != 1u) {
        discard;
    }
    // (Touch the palettes so both pipelines share one bind group layout.)
    let rows = f32(textureDimensions(palettes).y);
    return vec4<f32>(1.0, 1.0, 1.0, 1.0) + vec4<f32>(0.0 * rows);
}
";

/// A running game's frame: the backdrop when it changed, and the sprites
/// to draw over it on the GPU.
#[derive(Debug)]
pub struct LiveFrame {
    pub backdrop: Option<Canvas>,
    pub sprites: crate::sprites::Sprites,
}

/// Something that can draw the map for a set of layers.
pub trait Composer {
    fn compose(&mut self, layers: Layers) -> Canvas;

    /// For a running game: advance by `elapsed` wall time and return the
    /// new frame, if it changed. Static maps return `None`.
    fn frame(&mut self, _layers: Layers, _elapsed: std::time::Duration) -> Option<Canvas> {
        None
    }

    /// For a running game drawn on the GPU: advance by `elapsed` and return
    /// the sprites (and the backdrop, when it changed). Preferred over
    /// [`Composer::frame`] when it returns `Some`.
    fn live_frame(&mut self, _layers: Layers, _elapsed: std::time::Duration) -> Option<LiveFrame> {
        None
    }

    /// Whether [`Composer::frame`] should be called continuously.
    fn is_live(&self) -> bool {
        false
    }
}

impl<F: FnMut(Layers) -> Canvas> Composer for F {
    fn compose(&mut self, layers: Layers) -> Canvas {
        self(layers)
    }
}

/// Open a window on a map and run until it closes.
///
/// # Errors
/// If the window or the GPU can't be set up.
pub fn run(title: &str, layers: Layers, composer: impl Composer) -> Result<(), String> {
    let event_loop = EventLoop::new().map_err(|e| e.to_string())?;
    let mut app = App {
        title: title.to_string(),
        layers,
        composer,
        gpu: None,
        camera: Camera::default(),
        scaling: Scaling::default(),
        drag: None,
        error: None,
        last_frame: None,
    };
    event_loop.run_app(&mut app).map_err(|e| e.to_string())?;
    app.error.map_or(Ok(()), Err)
}

/// The view: which canvas texel sits at the centre, and screen pixels per
/// texel.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Camera {
    pub centre: (f32, f32),
    pub zoom: f32,
}

/// How texels become screen pixels, as the recomp's presenter offers.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Scaling {
    /// Nearest texel at any zoom.
    #[default]
    Nearest,
    /// Nearest texel, with the zoom held to whole numbers (or their
    /// reciprocals below 1), so every texel is the same size.
    Integer,
    /// Flat texels with their edges blended over one screen pixel: sharp
    /// at any zoom without uneven texel sizes.
    SharpBilinear,
}

impl Scaling {
    fn next(self) -> Scaling {
        match self {
            Scaling::Nearest => Scaling::Integer,
            Scaling::Integer => Scaling::SharpBilinear,
            Scaling::SharpBilinear => Scaling::Nearest,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Scaling::Nearest => "nearest",
            Scaling::Integer => "integer",
            Scaling::SharpBilinear => "sharp bilinear",
        }
    }

    /// The zoom this mode shows for a requested one.
    pub fn zoom(self, zoom: f32) -> f32 {
        if self != Scaling::Integer {
            return zoom;
        }
        if zoom >= 1.0 {
            zoom.round().max(1.0)
        } else {
            1.0 / (1.0 / zoom).round()
        }
    }

    fn shader_mode(self) -> f32 {
        if self == Scaling::SharpBilinear {
            1.0
        } else {
            0.0
        }
    }
}

/// Draws an uploaded map canvas into any target of its format.
struct Renderer {
    pipeline: wgpu::RenderPipeline,
    sampler: wgpu::Sampler,
    uniforms: wgpu::Buffer,
    bind_group: Option<wgpu::BindGroup>,
    texture_size: (u32, u32),
    /// The map as composed on the CPU, and the world texture the frame is
    /// built in (the backdrop copied, then sprites drawn over it) and the
    /// camera pass samples.
    backdrop: Option<wgpu::Texture>,
    world: Option<wgpu::Texture>,
    /// The canvas's world-pixel origin, and whether it was uploaded at
    /// full size (sprites need that).
    origin: (i32, i32),
    full_size: bool,
    sprite: wgpu::RenderPipeline,
    shadow: wgpu::RenderPipeline,
    frame_uniforms: wgpu::Buffer,
}

impl Renderer {
    // GPU setup is long but flat: pipelines and buffers, one after another.
    #[allow(clippy::too_many_lines)]
    fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Renderer {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("viewer"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("viewer"),
            layout: None,
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(format.into())],
            }),
            multiview_mask: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("camera"),
            size: 32,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let sprite_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("sprites"),
            source: wgpu::ShaderSource::Wgsl(SPRITE_SHADER.into()),
        });
        let instance = Some(wgpu::VertexBufferLayout {
            array_stride: 32,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &wgpu::vertex_attr_array![0 => Float32x4, 1 => Uint32x2, 2 => Uint32],
        });
        let make = |entry: &str, blend: Option<wgpu::BlendState>| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(entry),
                layout: None,
                vertex: wgpu::VertexState {
                    module: &sprite_module,
                    entry_point: Some("vs"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers: std::slice::from_ref(&instance),
                },
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &sprite_module,
                    entry_point: Some(entry),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: wgpu::TextureFormat::Rgba8Unorm,
                        blend,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let darken = wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::Zero,
            dst_factor: wgpu::BlendFactor::Constant,
            operation: wgpu::BlendOperation::Add,
        };
        let keep = wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::Zero,
            dst_factor: wgpu::BlendFactor::One,
            operation: wgpu::BlendOperation::Add,
        };
        let sprite = make("fs", None);
        let shadow = make(
            "fs_shadow",
            Some(wgpu::BlendState {
                color: darken,
                alpha: keep,
            }),
        );
        let frame_uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("frame"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Renderer {
            pipeline,
            sampler,
            uniforms,
            bind_group: None,
            texture_size: (1, 1),
            backdrop: None,
            world: None,
            origin: (0, 0),
            full_size: false,
            sprite,
            shadow,
            frame_uniforms,
        }
    }

    /// Upload a canvas, shrunk to fit the device's largest texture, as the
    /// backdrop and the starting world texture.
    fn upload(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, canvas: &Canvas) {
        let (pixels, w, h, scale) = fit(canvas, device.limits().max_texture_dimension_2d);
        let size = wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        };
        let make = |label, usage| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage,
                view_formats: &[],
            })
        };
        let backdrop = make(
            "backdrop",
            wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::COPY_DST,
        );
        let world = make(
            "world",
            wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::RENDER_ATTACHMENT,
        );
        for texture in [&backdrop, &world] {
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &pixels,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(w * 4),
                    rows_per_image: Some(h),
                },
                size,
            );
        }
        let view = world.create_view(&wgpu::TextureViewDescriptor::default());
        self.bind_group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("map"),
            layout: &self.pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: self.uniforms.as_entire_binding(),
                },
            ],
        }));
        self.texture_size = (w, h);
        self.backdrop = Some(backdrop);
        self.world = Some(world);
        self.origin = (canvas.origin_x, canvas.origin_y);
        #[allow(clippy::float_cmp)]
        {
            self.full_size = scale == 1.0;
        }
    }

    /// Build the frame in the world texture: the backdrop, then `sprites`
    /// in order on top. Skipped (the backdrop shows alone) if the canvas
    /// had to be shrunk to fit.
    #[allow(clippy::too_many_lines)]
    fn draw_sprites(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        sprites: &crate::sprites::Sprites,
    ) {
        let (Some(backdrop), Some(world)) = (&self.backdrop, &self.world) else {
            return;
        };
        let size = wgpu::Extent3d {
            width: self.texture_size.0,
            height: self.texture_size.1,
            depth_or_array_layers: 1,
        };
        encoder.copy_texture_to_texture(backdrop.as_image_copy(), world.as_image_copy(), size);
        let max = device.limits().max_texture_dimension_2d;
        if !self.full_size
            || sprites.draws.is_empty()
            || sprites.atlas_height == 0
            || sprites.atlas_height > max
        {
            return;
        }
        let texture = |label, format, (w, h): (u32, u32), data: &[u8], bytes: u32| {
            let t = device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            queue.write_texture(
                t.as_image_copy(),
                data,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(w * bytes),
                    rows_per_image: Some(h),
                },
                wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
            );
            t
        };
        let atlas = texture(
            "atlas",
            wgpu::TextureFormat::R8Uint,
            (crate::sprites::ATLAS_WIDTH, sprites.atlas_height),
            &sprites.atlas,
            1,
        );
        let rows = u32::try_from(sprites.palettes.len() / 256)
            .unwrap_or(1)
            .max(1);
        let palette_bytes: Vec<u8> = sprites.palettes.iter().flatten().copied().collect();
        let palettes = texture(
            "palettes",
            wgpu::TextureFormat::Rgba8Unorm,
            (256, rows),
            &palette_bytes,
            4,
        );
        #[allow(clippy::cast_precision_loss)]
        let frame = [
            self.origin.0 as f32,
            self.origin.1 as f32,
            self.texture_size.0 as f32,
            self.texture_size.1 as f32,
        ];
        queue.write_buffer(
            &self.frame_uniforms,
            0,
            &frame
                .iter()
                .flat_map(|v| v.to_le_bytes())
                .collect::<Vec<u8>>(),
        );
        let mut instances = Vec::with_capacity(sprites.draws.len() * 32);
        for d in &sprites.draws {
            #[allow(clippy::cast_precision_loss)]
            for v in [d.x as f32, d.y as f32, d.width as f32, d.height as f32] {
                instances.extend_from_slice(&v.to_le_bytes());
            }
            for v in [d.atlas_x, d.atlas_y, d.palette, 0] {
                instances.extend_from_slice(&v.to_le_bytes());
            }
        }
        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sprites"),
            size: instances.len() as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&instance_buffer, 0, &instances);
        let atlas_view = atlas.create_view(&wgpu::TextureViewDescriptor::default());
        let palette_view = palettes.create_view(&wgpu::TextureViewDescriptor::default());
        let bind = |pipeline: &wgpu::RenderPipeline| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("sprites"),
                layout: &pipeline.get_bind_group_layout(0),
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&atlas_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&palette_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: self.frame_uniforms.as_entire_binding(),
                    },
                ],
            })
        };
        let (sprite_group, shadow_group) = (bind(&self.sprite), bind(&self.shadow));
        let target = world.create_view(&wgpu::TextureViewDescriptor::default());
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("sprites"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_vertex_buffer(0, instance_buffer.slice(..));
        pass.set_blend_constant(wgpu::Color {
            r: 127.0 / 255.0,
            g: 127.0 / 255.0,
            b: 127.0 / 255.0,
            a: 1.0,
        });
        // Runs of shadows and of plain sprites, in drawing order.
        let mut start = 0;
        while start < sprites.draws.len() {
            let shadow = sprites.draws[start].shadow;
            let end = sprites.draws[start..]
                .iter()
                .position(|d| d.shadow != shadow)
                .map_or(sprites.draws.len(), |n| start + n);
            if shadow {
                pass.set_pipeline(&self.shadow);
                pass.set_bind_group(0, &shadow_group, &[]);
            } else {
                pass.set_pipeline(&self.sprite);
                pass.set_bind_group(0, &sprite_group, &[]);
            }
            let (s0, s1) = (
                u32::try_from(start).unwrap_or(u32::MAX),
                u32::try_from(end).unwrap_or(u32::MAX),
            );
            pass.draw(0..6, s0..s1);
            start = end;
        }
    }

    /// The camera that shows the whole texture in a viewport.
    #[allow(clippy::cast_precision_loss)]
    fn fit_camera(&self, viewport: (u32, u32)) -> Camera {
        let (tw, th) = (self.texture_size.0 as f32, self.texture_size.1 as f32);
        Camera {
            centre: (tw / 2.0, th / 2.0),
            zoom: (viewport.0 as f32 / tw).min(viewport.1 as f32 / th),
        }
    }

    fn encode(
        &self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        viewport: (u32, u32),
        camera: Camera,
        scaling: Scaling,
    ) {
        let Some(bind_group) = &self.bind_group else {
            return;
        };
        #[allow(clippy::cast_precision_loss)]
        let u = [
            camera.centre.0,
            camera.centre.1,
            scaling.zoom(camera.zoom),
            scaling.shader_mode(),
            viewport.0 as f32,
            viewport.1 as f32,
            self.texture_size.0 as f32,
            self.texture_size.1 as f32,
        ];
        let bytes: Vec<u8> = u.iter().flat_map(|v| v.to_le_bytes()).collect();
        queue.write_buffer(&self.uniforms, 0, &bytes);
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("map"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
}

/// Draw a canvas through the viewer's GPU path without a window, at
/// `width`×`height` with `camera` (or fitted to the whole canvas), and read
/// the pixels back as RGBA. For checking the GPU path against the canvas.
///
/// # Errors
/// If there's no GPU adapter.
pub fn snapshot(
    canvas: &Canvas,
    width: u32,
    height: u32,
    camera: Option<Camera>,
    scaling: Scaling,
) -> Result<Vec<u8>, String> {
    snapshot_frame(canvas, None, width, height, camera, scaling)
}

/// As [`snapshot`], with `sprites` drawn over the canvas on the GPU first.
///
/// # Errors
/// If there's no GPU adapter.
pub fn snapshot_frame(
    canvas: &Canvas,
    sprites: Option<&crate::sprites::Sprites>,
    width: u32,
    height: u32,
    camera: Option<Camera>,
    scaling: Scaling,
) -> Result<Vec<u8>, String> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .map_err(|e| e.to_string())?;
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
            .map_err(|e| e.to_string())?;
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let mut renderer = Renderer::new(&device, format);
    renderer.upload(&device, &queue, canvas);
    let camera = camera.unwrap_or_else(|| renderer.fit_camera((width, height)));
    let size = wgpu::Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    };
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("snapshot"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let row = (width * 4).div_ceil(256) * 256;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("readback"),
        size: u64::from(row * height),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    if let Some(sprites) = sprites {
        renderer.draw_sprites(&device, &queue, &mut encoder, sprites);
    }
    renderer.encode(
        &queue,
        &mut encoder,
        &target.create_view(&wgpu::TextureViewDescriptor::default()),
        (width, height),
        camera,
        scaling,
    );
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &target,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row),
                rows_per_image: Some(height),
            },
        },
        size,
    );
    queue.submit([encoder.finish()]);
    let slice = buffer.slice(..);
    slice.map_async(wgpu::MapMode::Read, |_| {});
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(|e| e.to_string())?;
    let data = slice.get_mapped_range().map_err(|e| e.to_string())?;
    let mut out = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        let start = (y * row) as usize;
        out.extend_from_slice(&data[start..start + (width * 4) as usize]);
    }
    Ok(out)
}

/// Build a frame in the world texture (the canvas, then `sprites` drawn on
/// the GPU) and read it back at world scale, to check against the CPU.
///
/// # Errors
/// If there's no GPU adapter.
pub fn world_pixels(canvas: &Canvas, sprites: &crate::sprites::Sprites) -> Result<Vec<u8>, String> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .map_err(|e| e.to_string())?;
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
            .map_err(|e| e.to_string())?;
    let mut renderer = Renderer::new(&device, wgpu::TextureFormat::Rgba8Unorm);
    renderer.upload(&device, &queue, canvas);
    let (w, h) = renderer.texture_size;
    let row = (w * 4).div_ceil(256) * 256;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("readback"),
        size: u64::from(row * h),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    renderer.draw_sprites(&device, &queue, &mut encoder, sprites);
    let world = renderer.world.as_ref().ok_or("no world texture")?;
    encoder.copy_texture_to_buffer(
        world.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row),
                rows_per_image: Some(h),
            },
        },
        wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
    );
    queue.submit([encoder.finish()]);
    let slice = buffer.slice(..);
    slice.map_async(wgpu::MapMode::Read, |_| {});
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(|e| e.to_string())?;
    let data = slice.get_mapped_range().map_err(|e| e.to_string())?;
    let mut out = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        let start = (y * row) as usize;
        out.extend_from_slice(&data[start..start + (w * 4) as usize]);
    }
    Ok(out)
}

struct Gpu {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    renderer: Renderer,
    /// The latest live frame's sprites, drawn over the backdrop each frame.
    sprites: Option<crate::sprites::Sprites>,
}

struct App<C> {
    title: String,
    layers: Layers,
    composer: C,
    gpu: Option<Gpu>,
    camera: Camera,
    scaling: Scaling,
    drag: Option<(PhysicalPosition<f64>, Option<PhysicalPosition<f64>>)>,
    error: Option<String>,
    /// When the last live frame was made.
    last_frame: Option<std::time::Instant>,
}

impl<C: Composer> App<C> {
    fn init(&mut self, event_loop: &ActiveEventLoop) -> Result<(), String> {
        let attrs = Window::default_attributes()
            .with_title(self.title.clone())
            .with_inner_size(LogicalSize::new(1280, 800));
        let window = Arc::new(event_loop.create_window(attrs).map_err(|e| e.to_string())?);
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let surface = instance
            .create_surface(window.clone())
            .map_err(|e| e.to_string())?;
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            compatible_surface: Some(&surface),
            ..Default::default()
        }))
        .map_err(|e| e.to_string())?;
        let limits = wgpu::Limits {
            max_texture_dimension_2d: adapter.limits().max_texture_dimension_2d,
            ..wgpu::Limits::default()
        };
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("viewer"),
            required_limits: limits,
            ..Default::default()
        }))
        .map_err(|e| e.to_string())?;
        let size = window.inner_size();
        let mut config = surface
            .get_default_config(&adapter, size.width.max(1), size.height.max(1))
            .ok_or("the surface isn't supported by the adapter")?;
        // The canvas is sRGB already: present it without conversion.
        config.format = config.format.remove_srgb_suffix();
        config.view_formats = vec![config.format];
        surface.configure(&device, &config);
        let renderer = Renderer::new(&device, config.format);
        self.gpu = Some(Gpu {
            window,
            surface,
            device,
            queue,
            config,
            renderer,
            sprites: None,
        });
        self.recompose(true);
        Ok(())
    }

    /// Draw the map again with the current layers and upload it.
    fn recompose(&mut self, reset_camera: bool) {
        let canvas = self.composer.compose(self.layers);
        let Some(gpu) = self.gpu.as_mut() else { return };
        gpu.renderer.upload(&gpu.device, &gpu.queue, &canvas);
        if reset_camera {
            self.camera = gpu
                .renderer
                .fit_camera((gpu.config.width, gpu.config.height));
        }
        self.update_title();
    }

    fn update_title(&self) {
        let Some(gpu) = self.gpu.as_ref() else { return };
        gpu.window.set_title(&format!(
            "{} — objects {} · lighting {} · shroud {} · {}",
            self.title,
            on(self.layers.objects),
            on(self.layers.lighting),
            on(self.layers.shroud),
            self.scaling.name()
        ));
        gpu.window.request_redraw();
    }

    fn toggle_fullscreen(&self) {
        let Some(gpu) = self.gpu.as_ref() else { return };
        let full = gpu.window.fullscreen().is_some();
        gpu.window
            .set_fullscreen((!full).then_some(winit::window::Fullscreen::Borderless(None)));
    }

    fn draw(&self) {
        let Some(gpu) = self.gpu.as_ref() else { return };
        let frame = match gpu.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(f)
            | wgpu::CurrentSurfaceTexture::Suboptimal(f) => f,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                gpu.surface.configure(&gpu.device, &gpu.config);
                gpu.window.request_redraw();
                return;
            }
            _ => return,
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        if let Some(sprites) = &gpu.sprites {
            gpu.renderer
                .draw_sprites(&gpu.device, &gpu.queue, &mut encoder, sprites);
        }
        gpu.renderer.encode(
            &gpu.queue,
            &mut encoder,
            &view,
            (gpu.config.width, gpu.config.height),
            self.camera,
            self.scaling,
        );
        gpu.queue.submit([encoder.finish()]);
        gpu.queue.present(frame);
    }

    fn pan(&mut self, dx: f32, dy: f32) {
        self.camera.centre.0 -= dx / self.camera.zoom;
        self.camera.centre.1 -= dy / self.camera.zoom;
        self.redraw();
    }

    fn zoom(&mut self, factor: f32) {
        self.camera.zoom = (self.camera.zoom * factor).clamp(0.05, 16.0);
        self.redraw();
    }

    fn redraw(&self) {
        if let Some(gpu) = &self.gpu {
            gpu.window.request_redraw();
        }
    }
}

fn on(b: bool) -> &'static str {
    if b { "on" } else { "off" }
}

/// The canvas as texture data no larger than `max` on a side: every n-th
/// pixel when it's too big.
fn fit(canvas: &Canvas, max: u32) -> (Vec<u8>, u32, u32, f32) {
    let step = canvas.width.max(canvas.height).div_ceil(max.max(1)).max(1);
    if step == 1 {
        return (
            canvas.rgba.clone(),
            canvas.width.max(1),
            canvas.height.max(1),
            1.0,
        );
    }
    let (w, h) = (canvas.width / step, canvas.height / step);
    let mut out = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let i = ((y * step * canvas.width + x * step) * 4) as usize;
            out.extend_from_slice(&canvas.rgba[i..i + 4]);
        }
    }
    #[allow(clippy::cast_precision_loss)]
    (out, w, h, 1.0 / step as f32)
}

impl<C: Composer> App<C> {
    /// For a live composer: advance it and show the new frame.
    fn advance(&mut self, event_loop: &ActiveEventLoop) {
        if !self.composer.is_live() {
            return;
        }
        let now = std::time::Instant::now();
        let elapsed = self.last_frame.map_or_else(Default::default, |t| now - t);
        self.last_frame = Some(now);
        if let Some(frame) = self.composer.live_frame(self.layers, elapsed) {
            if let Some(gpu) = self.gpu.as_mut() {
                if let Some(backdrop) = &frame.backdrop {
                    gpu.renderer.upload(&gpu.device, &gpu.queue, backdrop);
                }
                gpu.sprites = Some(frame.sprites);
                gpu.window.request_redraw();
            }
        } else if let Some(canvas) = self.composer.frame(self.layers, elapsed)
            && let Some(gpu) = self.gpu.as_mut()
        {
            gpu.renderer.upload(&gpu.device, &gpu.queue, &canvas);
            gpu.window.request_redraw();
        }
        // About sixty times a second.
        event_loop.set_control_flow(winit::event_loop::ControlFlow::WaitUntil(
            now + std::time::Duration::from_millis(16),
        ));
    }
}

impl<C: Composer> ApplicationHandler for App<C> {
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.advance(event_loop);
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.gpu.is_none()
            && let Err(e) = self.init(event_loop)
        {
            self.error = Some(e);
            event_loop.exit();
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(gpu) = self.gpu.as_mut() {
                    gpu.config.width = size.width.max(1);
                    gpu.config.height = size.height.max(1);
                    gpu.surface.configure(&gpu.device, &gpu.config);
                    gpu.window.request_redraw();
                }
            }
            WindowEvent::RedrawRequested => self.draw(),
            WindowEvent::MouseWheel { delta, .. } => {
                #[allow(clippy::cast_possible_truncation)]
                let lines = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => (p.y / 40.0) as f32,
                };
                self.zoom(1.1f32.powf(lines));
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left | MouseButton::Right,
                ..
            } => {
                self.drag = (state == ElementState::Pressed)
                    .then_some((PhysicalPosition::new(0.0, 0.0), None));
            }
            WindowEvent::CursorMoved { position, .. } => {
                if let Some((_, last)) = self.drag {
                    if let Some(last) = last {
                        #[allow(clippy::cast_possible_truncation)]
                        self.pan((position.x - last.x) as f32, (position.y - last.y) as f32);
                    }
                    self.drag = Some((position, Some(position)));
                }
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                let step = 120.0;
                match event.logical_key.as_ref() {
                    Key::Named(NamedKey::Escape) => event_loop.exit(),
                    Key::Named(NamedKey::F11) => self.toggle_fullscreen(),
                    Key::Named(NamedKey::ArrowLeft) => self.pan(step, 0.0),
                    Key::Named(NamedKey::ArrowRight) => self.pan(-step, 0.0),
                    Key::Named(NamedKey::ArrowUp) => self.pan(0.0, step),
                    Key::Named(NamedKey::ArrowDown) => self.pan(0.0, -step),
                    Key::Character(c) => match c.to_ascii_lowercase().as_str() {
                        "+" | "=" => self.zoom(1.25),
                        "-" => self.zoom(0.8),
                        "0" => self.recompose(true),
                        "o" => {
                            self.layers.objects = !self.layers.objects;
                            self.recompose(false);
                        }
                        "l" => {
                            self.layers.lighting = !self.layers.lighting;
                            self.recompose(false);
                        }
                        "s" => {
                            self.layers.shroud = !self.layers.shroud;
                            self.recompose(false);
                        }
                        "m" => {
                            self.scaling = self.scaling.next();
                            self.update_title();
                        }
                        "f" => self.toggle_fullscreen(),
                        _ => {}
                    },
                    _ => {}
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oversized_canvases_are_subsampled_to_fit() {
        let c = Canvas::new(0, 0, 10, 4, [1, 2, 3, 255]);
        let (px, w, h, s) = fit(&c, 16);
        assert_eq!((w, h, s, px.len()), (10, 4, 1.0, 160));
        let (px, w, h, s) = fit(&c, 5);
        assert_eq!((w, h, s, px.len()), (5, 2, 0.5, 40));
    }

    #[test]
    fn gpu_path_reproduces_the_canvas_at_zoom_one() {
        let mut c = Canvas::new(-10, 5, 54, 37, [0, 0, 0, 255]);
        for y in 5..37 {
            for x in -10..54 {
                let v = |n: i32| u8::try_from(n.rem_euclid(256)).unwrap();
                c.put(x, y, [v(x * 7), v(y * 13), v(x + y)]);
            }
        }
        let camera = Camera {
            centre: (32.0, 16.0),
            zoom: 1.0,
        };
        for scaling in [Scaling::Nearest, Scaling::Integer, Scaling::SharpBilinear] {
            match snapshot(&c, 64, 32, Some(camera), scaling) {
                Ok(px) => assert_eq!(px, c.rgba, "{scaling:?}"),
                // No GPU here (as on CI runners): nothing to check.
                Err(e) => eprintln!("skipped: {e}"),
            }
        }
    }

    /// The GPU sprite path against the CPU canvas: overlapping sprites in
    /// two palettes under two cell lights, and a shadow, over a patterned
    /// background, byte for byte.
    #[test]
    fn gpu_sprites_match_the_cpu_canvas() {
        use crate::lighting::{Lamp, Lighting};
        use crate::scene::{PaletteKind, Piece};
        use crate::sprites::Sprites;
        let mut background = Canvas::new(-20, -10, 60, 40, [0, 0, 0, 255]);
        for y in -10..40 {
            for x in -20..60 {
                let v = |n: i32| u8::try_from(n.rem_euclid(256)).unwrap();
                background.put(x, y, [v(x * 9 + 3), v(y * 7 + 1), v(x * y)]);
            }
        }
        let palette = |kind: PaletteKind| {
            let mut p = [[0u8; 3]; 256];
            for (i, c) in p.iter_mut().enumerate() {
                let i = u8::try_from(i).unwrap();
                *c = match kind {
                    PaletteKind::Iso => [i, i.wrapping_mul(3), 255 - i],
                    _ => [255 - i, i, i.wrapping_mul(7)],
                };
            }
            p
        };
        let light = Lighting {
            ambient: 0.8,
            lamps: vec![Lamp {
                x: 128,
                y: 128,
                range: 2048,
                intensity: 300,
                tint: [80, 0, 20],
            }],
            ..Lighting::default()
        };
        let pattern = |w: usize, h: usize, k: usize| -> Vec<u8> {
            (0..w * h)
                .map(|i| {
                    if (i + k).is_multiple_of(5) {
                        0
                    } else {
                        u8::try_from((i * 7 + k) % 255 + 1).unwrap()
                    }
                })
                .collect()
        };
        let pieces = vec![
            Piece {
                order: (2, 0, 0),
                x: -5,
                y: 0,
                width: 20,
                pixels: pattern(20, 12, 1),
                palette: PaletteKind::Iso,
                shadow: false,
                at: (0, 0, 0),
            },
            Piece {
                order: (1, 0, 0),
                x: 0,
                y: 4,
                width: 18,
                pixels: vec![1; 18 * 9],
                palette: PaletteKind::Theater,
                shadow: true,
                at: (0, 0, 0),
            },
            Piece {
                order: (3, 0, 0),
                x: 8,
                y: 6,
                width: 25,
                pixels: pattern(25, 15, 3),
                palette: PaletteKind::Theater,
                shadow: false,
                at: (5, 2, 1),
            },
        ];
        // The CPU reference, as scene::draw composites.
        let mut cpu = background.clone();
        let mut sorted = pieces.clone();
        sorted.sort_by_key(|p| p.order);
        for p in &sorted {
            let lit = Lighting::lit(&palette(p.palette), light.cell(p.at.0, p.at.1, p.at.2));
            cpu.blit(p.x, p.y, p.width, &p.pixels, &lit, p.shadow);
        }
        let sprites = Sprites::build(pieces, &palette, &light);
        match world_pixels(&background, &sprites) {
            Ok(gpu) => {
                let differ = gpu
                    .chunks(4)
                    .zip(cpu.rgba.chunks(4))
                    .filter(|(a, b)| a != b)
                    .count();
                assert_eq!(differ, 0, "{differ} pixels differ");
            }
            Err(e) => eprintln!("skipped: {e}"),
        }
    }

    #[test]
    fn integer_scaling_holds_zoom_to_whole_steps() {
        assert_eq!(Scaling::Integer.zoom(2.4), 2.0);
        assert_eq!(Scaling::Integer.zoom(0.45), 0.5);
        assert_eq!(Scaling::Integer.zoom(0.9), 1.0);
        assert_eq!(Scaling::Nearest.zoom(2.4), 2.4);
    }

    #[test]
    fn sharp_bilinear_shows_flat_texels_with_soft_edges_when_magnified() {
        let mut c = Canvas::new(0, 0, 4, 1, [0, 0, 0, 255]);
        c.put(2, 0, [255, 255, 255]);
        c.put(3, 0, [255, 255, 255]);
        let camera = Camera {
            centre: (2.0, 0.5),
            zoom: 4.0,
        };
        let Ok(px) = snapshot(&c, 16, 4, Some(camera), Scaling::SharpBilinear) else {
            return;
        };
        let red: Vec<u8> = px[..16 * 4].chunks(4).map(|p| p[0]).collect();
        // Texel 1 (black) then texel 2 (white): flat inside, one blended
        // screen pixel at the boundary.
        assert_eq!(red[5], 0);
        assert_eq!(red[10], 255);
        assert!(red[7] < 255 && red[8] > 0, "{red:?}");
    }
}
