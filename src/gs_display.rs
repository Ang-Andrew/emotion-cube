// gs_display.rs — wgpu fullscreen-quad texture blit.
// Maps to: PS2 GS display output reading from eDRAM framebuffer.
// The software rasterizer writes a Framebuffer (CPU), this uploads it as a
// wgpu Rgba8Unorm texture and blits it to the canvas via a fullscreen quad.

use wasm_bindgen::JsCast;
use crate::gs_rasterizer::{Framebuffer, FB_W, FB_H};

// ---------------------------------------------------------------------------
// WGSL shader — 6-vertex hardcoded fullscreen quad, nearest-neighbor sample
// ---------------------------------------------------------------------------

const SHADER_SRC: &str = r#"
struct VO {
    @builtin(position) pos: vec4<f32>,
    @location(0)       uv:  vec2<f32>,
}

@group(0) @binding(0) var fb_tex: texture_2d<f32>;
@group(0) @binding(1) var fb_smp: sampler;

@vertex
fn vs(@builtin(vertex_index) vi: u32) -> VO {
    // Two triangles forming a fullscreen quad.
    // positions in NDC, UVs Y-flipped so Rgba8Unorm row-0 = screen top.
    var pos = array<vec2<f32>, 6>(
        vec2(-1.0,  1.0),
        vec2(-1.0, -1.0),
        vec2( 1.0, -1.0),
        vec2(-1.0,  1.0),
        vec2( 1.0, -1.0),
        vec2( 1.0,  1.0),
    );
    var uv = array<vec2<f32>, 6>(
        vec2(0.0, 0.0),
        vec2(0.0, 1.0),
        vec2(1.0, 1.0),
        vec2(0.0, 0.0),
        vec2(1.0, 1.0),
        vec2(1.0, 0.0),
    );
    var out: VO;
    out.pos = vec4(pos[vi], 0.0, 1.0);
    out.uv  = uv[vi];
    return out;
}

@fragment
fn fs(v: VO) -> @location(0) vec4<f32> {
    return textureSample(fb_tex, fb_smp, v.uv);
}
"#;

// ---------------------------------------------------------------------------
// GsDisplay
// ---------------------------------------------------------------------------

pub struct GsDisplay {
    surface:    wgpu::Surface<'static>,
    device:     wgpu::Device,
    queue:      wgpu::Queue,
    config:     wgpu::SurfaceConfiguration,
    pipeline:   wgpu::RenderPipeline,
    fb_texture: wgpu::Texture,
    bind_group: wgpu::BindGroup,
}

impl GsDisplay {
    pub async fn new(canvas_id: &str) -> Result<Self, String> {
        // --- DOM canvas ---
        let window   = web_sys::window().ok_or("no window")?;
        let document = window.document().ok_or("no document")?;
        let canvas: web_sys::HtmlCanvasElement = document
            .get_element_by_id(canvas_id)
            .ok_or_else(|| format!("canvas #{canvas_id} not found"))?
            .dyn_into()
            .map_err(|_| "element is not a canvas")?;

        // --- wgpu instance (WebGL2) ---
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::GL,
            ..Default::default()
        });

        let surface = instance
            .create_surface(wgpu::SurfaceTarget::Canvas(canvas))
            .map_err(|e| format!("create_surface: {e}"))?;

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: Some(&surface),
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: false,
            })
            .await
            .map_err(|e| format!("request_adapter: {e}"))?;

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("GS Device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::downlevel_webgl2_defaults(),
                memory_hints: wgpu::MemoryHints::default(),
                trace: wgpu::Trace::Off,
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
            })
            .await
            .map_err(|e| format!("request_device: {e}"))?;

        // --- surface config ---
        let caps   = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(caps.formats[0]);

        let config = wgpu::SurfaceConfiguration {
            usage:        wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width:        FB_W as u32,
            height:       FB_H as u32,
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode:   caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        // --- framebuffer texture (CPU→GPU upload target) ---
        let fb_texture = device.create_texture(&wgpu::TextureDescriptor {
            label:           Some("FB Texture"),
            size: wgpu::Extent3d {
                width:                 FB_W as u32,
                height:                FB_H as u32,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count:    1,
            dimension:       wgpu::TextureDimension::D2,
            format:          wgpu::TextureFormat::Rgba8Unorm,
            usage:           wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats:    &[],
        });

        // --- nearest sampler ---
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label:             Some("FB Sampler"),
            address_mode_u:    wgpu::AddressMode::ClampToEdge,
            address_mode_v:    wgpu::AddressMode::ClampToEdge,
            address_mode_w:    wgpu::AddressMode::ClampToEdge,
            mag_filter:        wgpu::FilterMode::Nearest,
            min_filter:        wgpu::FilterMode::Nearest,
            mipmap_filter:     wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });

        // --- bind group layout ---
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("FB BGL"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding:    0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled:   false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type:    wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding:    1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let tex_view = fb_texture.create_view(&Default::default());

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label:  Some("FB BindGroup"),
            layout: &bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding:  0,
                    resource: wgpu::BindingResource::TextureView(&tex_view),
                },
                wgpu::BindGroupEntry {
                    binding:  1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });

        // --- render pipeline ---
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label:  Some("Blit Shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER_SRC.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label:              Some("Blit Layout"),
            bind_group_layouts: &[&bgl],
            immediate_size:     0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label:  Some("Blit Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module:              &shader,
                entry_point:         Some("vs"),
                buffers:             &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module:              &shader,
                entry_point:         Some("fs"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend:      Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology:           wgpu::PrimitiveTopology::TriangleList,
                front_face:         wgpu::FrontFace::Ccw,
                cull_mode:          None,
                strip_index_format: None,
                polygon_mode:       wgpu::PolygonMode::Fill,
                unclipped_depth:    false,
                conservative:       false,
            },
            depth_stencil:  None,
            multisample:    wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache:          None,
        });

        Ok(GsDisplay {
            surface,
            device,
            queue,
            config,
            pipeline,
            fb_texture,
            bind_group,
        })
    }

    /// Upload the software framebuffer as a texture, then blit it fullscreen.
    pub fn upload_and_present(&mut self, fb: &Framebuffer) {
        // Reinterpret u32 pixels as raw bytes for write_texture
        let bytes: &[u8] = bytemuck::cast_slice(&fb.pixels);

        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture:   &self.fb_texture,
                mip_level: 0,
                origin:    wgpu::Origin3d::ZERO,
                aspect:    wgpu::TextureAspect::All,
            },
            bytes,
            wgpu::TexelCopyBufferLayout {
                offset:         0,
                bytes_per_row:  Some((FB_W * 4) as u32),
                rows_per_image: Some(FB_H as u32),
            },
            wgpu::Extent3d {
                width:                 FB_W as u32,
                height:                FB_H as u32,
                depth_or_array_layers: 1,
            },
        );

        let frame = match self.surface.get_current_texture() {
            Ok(f) => f,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                self.surface.configure(&self.device, &self.config);
                return;
            }
            Err(_) => return,
        };

        let view = frame.texture.create_view(&Default::default());

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Blit Encoder"),
        });

        {
            let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Blit Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view:           &view,
                    resolve_target: None,
                    depth_slice:    None,
                    ops: wgpu::Operations {
                        load:  wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes:         None,
                occlusion_query_set:      None,
                multiview_mask:           None,
            });

            rp.set_pipeline(&self.pipeline);
            rp.set_bind_group(0, &self.bind_group, &[]);
            rp.draw(0..6, 0..1);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        frame.present();
    }
}
