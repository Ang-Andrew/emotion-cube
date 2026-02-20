// Graphics Synthesizer stub.
// Maps to: the PS2 GS — a fixed-function rasterizer ASIC with 4 MB eDRAM.
// Our GS wraps wgpu and is the only module that touches GPU resources.

use wasm_bindgen::JsCast;
use wgpu::vertex_attr_array;

// ---------------------------------------------------------------------------
// GsVertex — output format from VU1, input to the GPU pipeline.
// Maps to: GIF tag vertex data arriving at the GS via PATH1 DMA / XGKICK.
// ---------------------------------------------------------------------------

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GsVertex {
    /// Clip-space XYZW — VU1 already performed the perspective projection.
    pub position: [f32; 4],
    /// Pre-lit Gouraud RGBA in [0, 1].
    pub color: [f32; 4],
}

impl GsVertex {
    const LAYOUT: wgpu::VertexBufferLayout<'static> = wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<GsVertex>() as wgpu::BufferAddress,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &vertex_attr_array![0 => Float32x4, 1 => Float32x4],
    };
}

// ---------------------------------------------------------------------------
// WGSL shader — passthrough (GS is fixed-function; VU1 already lit everything)
// Maps to: the GS rasterizer's fixed function interpolation & framebuffer write.
// ---------------------------------------------------------------------------

const SHADER_SRC: &str = r#"
struct VertexInput {
    @location(0) position: vec4<f32>,
    @location(1) color:    vec4<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0)       color:         vec4<f32>,
}

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    return VertexOutput(in.position, in.color);
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return in.color;
}
"#;

/// Maximum vertices we can submit in one frame (36 = 6 faces × 2 tris × 3 verts).
const MAX_VERTICES: u64 = 36;

// ---------------------------------------------------------------------------
// GraphicsSynthesizer
// ---------------------------------------------------------------------------

pub struct GraphicsSynthesizer {
    surface:        wgpu::Surface<'static>,
    device:         wgpu::Device,
    queue:          wgpu::Queue,
    config:         wgpu::SurfaceConfiguration,
    pipeline:       wgpu::RenderPipeline,
    vertex_buffer:  wgpu::Buffer,
}

impl GraphicsSynthesizer {
    /// Async initialisation — mirrors the PS2 boot sequence that sets up the GS
    /// and configures the display mode register.
    pub async fn new(canvas_id: &str) -> Result<Self, String> {
        // --- 1. Grab the canvas element from the DOM ---
        let window   = web_sys::window().ok_or("no window")?;
        let document = window.document().ok_or("no document")?;
        let canvas: web_sys::HtmlCanvasElement = document
            .get_element_by_id(canvas_id)
            .ok_or_else(|| format!("canvas #{canvas_id} not found"))?
            .dyn_into()
            .map_err(|_| "element is not a canvas")?;

        // --- 2. Create wgpu instance (WebGL2 backend for broadest browser compat) ---
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::GL,
            ..Default::default()
        });

        // --- 3. Create surface from canvas (canvas is moved/owned by the surface) ---
        let surface = instance
            .create_surface(wgpu::SurfaceTarget::Canvas(canvas))
            .map_err(|e| format!("create_surface: {e}"))?;

        // --- 4. Request adapter compatible with our surface ---
        // wgpu 28: request_adapter returns Result<Adapter, RequestAdapterError>
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: Some(&surface),
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: false,
            })
            .await
            .map_err(|e| format!("request_adapter: {e}"))?;

        // --- 5. Request device with WebGL2 limits ---
        // wgpu 28: DeviceDescriptor has experimental_features field
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

        // --- 6. Configure surface (prefer sRGB, maps to GS PSMCT32 framebuffer) ---
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
            width:        640,
            height:       448,
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode:   caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        // --- 7. Build render pipeline ---
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label:  Some("GS Shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER_SRC.into()),
        });

        // wgpu 28: PipelineLayoutDescriptor uses immediate_size instead of push_constant_ranges
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label:          Some("GS Pipeline Layout"),
            bind_group_layouts: &[],
            immediate_size: 0,
        });

        // wgpu 28: RenderPipelineDescriptor uses multiview_mask instead of multiview
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label:  Some("GS Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module:              &shader,
                entry_point:         Some("vs_main"),
                buffers:             &[GsVertex::LAYOUT],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module:              &shader,
                entry_point:         Some("fs_main"),
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
                cull_mode:          Some(wgpu::Face::Back),
                strip_index_format: None,
                polygon_mode:       wgpu::PolygonMode::Fill,
                unclipped_depth:    false,
                conservative:       false,
            },
            // No depth buffer for this PoC — convex cube + back-face cull is sufficient.
            depth_stencil: None,
            multisample:   wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache:          None,
        });

        // --- 8. Pre-allocate vertex buffer (VERTEX | COPY_DST) ---
        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label:              Some("GS Vertex Buffer"),
            size:               MAX_VERTICES * std::mem::size_of::<GsVertex>() as u64,
            usage:              wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Ok(GraphicsSynthesizer {
            surface,
            device,
            queue,
            config,
            pipeline,
            vertex_buffer,
        })
    }

    /// Render one frame of the display list.
    /// Maps to: the GS receiving a GIF packet from VU1 via XGKICK and rasterising it.
    pub fn render(&mut self, vertices: &[GsVertex]) {
        if vertices.is_empty() {
            return;
        }

        // Upload vertices to GPU (maps to: VIF1 DMA writing to GS eDRAM input FIFO)
        self.queue.write_buffer(
            &self.vertex_buffer,
            0,
            bytemuck::cast_slice(vertices),
        );

        // Acquire the next surface texture (maps to: GS double-buffered framebuffer)
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
            label: Some("GS Frame Encoder"),
        });

        {
            // wgpu 28: RenderPassDescriptor needs multiview_mask field
            // wgpu 28: RenderPassColorAttachment needs depth_slice field
            let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("GS Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view:           &view,
                    resolve_target: None,
                    depth_slice:    None,
                    ops: wgpu::Operations {
                        // PS2 BGCOLOR register: dark blue-black
                        load:  wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.03, g: 0.03, b: 0.08, a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes:         None,
                occlusion_query_set:      None,
                multiview_mask:           None,
            });

            rp.set_pipeline(&self.pipeline);
            rp.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            rp.draw(0..vertices.len() as u32, 0..1);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        frame.present();
    }
}
