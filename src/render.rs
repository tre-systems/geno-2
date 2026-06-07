use glam::Vec3;
use web_sys as web;

mod helpers;
mod post;
mod targets;
mod waves;

use targets::RenderTargets;
use waves::{create_waves_resources, VoicePacked, WavesResources, WavesUniforms};

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct PostUniforms {
    resolution: [f32; 2],
    time: f32,
    ambient: f32,
    blur_dir: [f32; 2],
    bloom_strength: f32,
    threshold: f32,
}

// POD layout guard: the Rust side of the uniform contract in shaders/post.wgsl.
const _: () = assert!(std::mem::size_of::<PostUniforms>() == 32);

pub struct GpuState<'a> {
    surface: wgpu::Surface<'a>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    // Waves full-screen layer
    waves: WavesResources,
    // Offscreen HDR scene + bloom ping-pong targets
    targets: RenderTargets,
    linear_sampler: wgpu::Sampler,
    // Post-processing pipelines, layouts, and uniform buffer
    post: post::PostResources,
    // Bind groups selecting the source texture for each post pass
    bg_hdr: wgpu::BindGroup,
    bg_from_bloom_a: wgpu::BindGroup,
    bg_from_bloom_b: wgpu::BindGroup,
    bg_bloom_a_only: wgpu::BindGroup, // group1 for composite, sampling bloom A

    width: u32,
    height: u32,
    clear_color: wgpu::Color,
    cam_eye: Vec3,
    cam_target: Vec3,
    time_accum: f32,
    ambient_energy: f32,
    swirl_uv: [f32; 2],
    swirl_strength: f32,
    swirl_active: f32,
    // Click/tap ripple state
    ripple_uv: [f32; 2],
    ripple_t0: f32,
    ripple_amp: f32,
}

impl<'a> GpuState<'a> {
    pub async fn new(canvas: &'a web::HtmlCanvasElement, camera_z: f32) -> anyhow::Result<Self> {
        let width = canvas.width();
        let height = canvas.height();

        let instance = wgpu::Instance::default();
        let surface = instance.create_surface(wgpu::SurfaceTarget::Canvas(canvas.clone()))?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "No WebGPU adapter available. This could be due to:\n\
                     - WebGPU not supported in this browser\n\
                     - WebGPU disabled in browser settings\n\
                     - Running in headless mode without GPU access\n\
                     - Graphics drivers not compatible with WebGPU"
                )
            })?;
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    required_features: wgpu::Features::empty(),
                    // Use default limits on web to avoid passing unknown fields to older WebGPU impls
                    required_limits: wgpu::Limits::default(),
                    memory_hints: wgpu::MemoryHints::Performance,
                    label: None,
                },
                None,
            )
            .await
            .map_err(|e| {
                anyhow::anyhow!(
                    "Failed to create WebGPU device: {:?}\n\
                     This could indicate:\n\
                     - Insufficient GPU memory\n\
                     - Requested features not supported\n\
                     - GPU driver issues",
                    e
                )
            })?;
        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| {
                matches!(
                    f,
                    wgpu::TextureFormat::Bgra8UnormSrgb | wgpu::TextureFormat::Rgba8UnormSrgb
                )
            })
            .unwrap_or(caps.formats[0]);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width,
            height,
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        // Offscreen HDR targets (scene and bloom) at full and half resolution
        let hdr_format = wgpu::TextureFormat::Rgba16Float;
        let (hdr_tex, hdr_view) = helpers::create_color_texture(
            &device,
            "hdr_tex",
            width,
            height,
            hdr_format,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        );
        let bloom_w = (width.max(1) / 2).max(1);
        let bloom_h = (height.max(1) / 2).max(1);
        let bloom_format = wgpu::TextureFormat::Rgba16Float;
        let (bloom_a, bloom_a_view) = helpers::create_color_texture(
            &device,
            "bloom_a",
            bloom_w,
            bloom_h,
            bloom_format,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        );
        let (bloom_b, bloom_b_view) = helpers::create_color_texture(
            &device,
            "bloom_b",
            bloom_w,
            bloom_h,
            bloom_format,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        );

        // Waves fullscreen pass (drawn into HDR before bloom)
        let waves = create_waves_resources(&device, hdr_format);

        // Post shader + pipelines
        let post_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("post_shader"),
            source: wgpu::ShaderSource::Wgsl(crate::core::POST_WGSL.into()),
        });
        let linear_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("linear_sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let post = post::create_post_resources(&device, &post_shader, bloom_format, format);
        let targets = RenderTargets::new(
            hdr_tex,
            hdr_view,
            bloom_a,
            bloom_a_view,
            bloom_b,
            bloom_b_view,
        );
        let (bg_hdr, bg_from_a, bg_from_b, bg_a_only) = post::rebuild_bind_groups(
            &device,
            &post,
            &linear_sampler,
            &targets.hdr_view,
            &targets.bloom_a_view,
            &targets.bloom_b_view,
        );

        Ok(Self {
            surface,
            device,
            queue,
            config,
            waves,
            targets,
            linear_sampler,
            post,
            bg_hdr,
            bg_from_bloom_a: bg_from_a,
            bg_from_bloom_b: bg_from_b,
            bg_bloom_a_only: bg_a_only,
            width,
            height,
            clear_color: wgpu::Color {
                r: 0.014,
                g: 0.018,
                b: 0.023,
                a: 1.0,
            },
            cam_eye: Vec3::new(0.0, 0.0, camera_z),
            cam_target: Vec3::ZERO,
            time_accum: 0.0,
            ambient_energy: 0.0,
            swirl_uv: [0.5, 0.5],
            swirl_strength: 0.0,
            swirl_active: 0.0,
            ripple_uv: [0.5, 0.5],
            ripple_t0: -1.0,
            ripple_amp: 0.0,
        })
    }
    pub fn set_ambient_clear(&mut self, energy01: f32) {
        // Dark slate base that lifts toward teal/amber haze as ambient energy grows.
        let e = energy01.clamp(0.0, 1.0);
        let lift = 0.16 * e;
        self.clear_color = wgpu::Color {
            r: (0.014 + lift * 0.66) as f64,
            g: (0.018 + lift * 0.72) as f64,
            b: (0.023 + lift * 0.58) as f64,
            a: 1.0,
        };
        self.ambient_energy = e;
    }

    pub fn set_camera(&mut self, eye: Vec3, target: Vec3) {
        self.cam_eye = eye;
        self.cam_target = target;
    }

    pub fn set_swirl(&mut self, uv: [f32; 2], strength: f32, active: bool) {
        self.swirl_uv = uv;
        self.swirl_strength = strength;
        self.swirl_active = if active { 1.0 } else { 0.0 };
    }

    pub fn set_ripple(&mut self, uv: [f32; 2], amp: f32) {
        self.ripple_uv = uv;
        self.ripple_amp = amp.clamp(0.0, 2.8);
        // Anchor ripple start to current accumulated time so shader can compute age
        self.ripple_t0 = self.time_accum;
    }

    pub fn resize_if_needed(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        if width != self.width || height != self.height {
            self.width = width;
            self.height = height;
            self.config.width = width;
            self.config.height = height;
            self.surface.configure(&self.device, &self.config);

            // Recreate offscreen render targets and dependent bind groups
            self.targets.recreate(&self.device, width, height);

            // Rebuild bind groups that reference these views
            self.rebuild_post_bind_groups();
        }
    }

    /// Re-apply the surface configuration after the swapchain is lost or outdated
    /// (GPU reset, tab restore). The offscreen targets and bind groups don't
    /// depend on the swapchain, so only the surface needs reconfiguring.
    pub fn reconfigure(&mut self) {
        self.surface.configure(&self.device, &self.config);
    }

    pub fn render(
        &mut self,
        dt_sec: f32,
        voice_positions: &[Vec3],
        pulse_energy: &[f32],
    ) -> Result<(), wgpu::SurfaceError> {
        self.time_accum += dt_sec.max(0.0);
        let frame = self.surface.get_current_texture()?;
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("encoder"),
            });
        {
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("scene_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.targets.hdr_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(self.clear_color),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            let w = WavesUniforms {
                resolution: [self.width as f32, self.height as f32],
                time: self.time_accum,
                ambient: self.ambient_energy,
                voices: [
                    VoicePacked {
                        pos_pulse: [
                            voice_positions[0].x,
                            voice_positions[0].y,
                            voice_positions[0].z,
                            pulse_energy[0],
                        ],
                    },
                    VoicePacked {
                        pos_pulse: [
                            voice_positions[1].x,
                            voice_positions[1].y,
                            voice_positions[1].z,
                            pulse_energy[1],
                        ],
                    },
                    VoicePacked {
                        pos_pulse: [
                            voice_positions[2].x,
                            voice_positions[2].y,
                            voice_positions[2].z,
                            pulse_energy[2],
                        ],
                    },
                ],
                swirl_uv: [
                    self.swirl_uv[0].clamp(0.0, 1.0),
                    self.swirl_uv[1].clamp(0.0, 1.0),
                ],
                swirl_strength: if self.swirl_active > 0.5 {
                    self.swirl_strength
                } else {
                    0.0
                },
                swirl_active: self.swirl_active,
                ripple_uv: self.ripple_uv,
                ripple_t0: self.ripple_t0,
                ripple_amp: self.ripple_amp,
            };
            self.queue
                .write_buffer(&self.waves.uniform_buffer, 0, bytemuck::bytes_of(&w));
            rpass.set_pipeline(&self.waves.pipeline);
            rpass.set_bind_group(0, &self.waves.bind_group, &[]);
            rpass.draw(0..3, 0..1);
        }

        let res = [self.width as f32 / 2.0, self.height as f32 / 2.0];
        post::write_post_uniforms(
            &self.queue,
            &self.post.uniform_buffer,
            res,
            self.time_accum,
            self.ambient_energy,
            [0.0, 0.0],
        );

        // Pass 2: bright pass → bloom_a
        post::blit(
            &mut encoder,
            "bright_pass",
            &self.targets.bloom_a_view,
            wgpu::Color::BLACK,
            &self.post.bright_pipeline,
            &self.bg_hdr,
            None,
        );

        // Pass 3: blur horizontal bloom_a -> bloom_b
        post::write_post_uniforms(
            &self.queue,
            &self.post.uniform_buffer,
            res,
            self.time_accum,
            self.ambient_energy,
            [1.0, 0.0],
        );
        post::blit(
            &mut encoder,
            "blur_h",
            &self.targets.bloom_b_view,
            wgpu::Color::BLACK,
            &self.post.blur_pipeline,
            &self.bg_from_bloom_a,
            None,
        );

        // Pass 4: blur vertical bloom_b -> bloom_a
        post::write_post_uniforms(
            &self.queue,
            &self.post.uniform_buffer,
            res,
            self.time_accum,
            self.ambient_energy,
            [0.0, 1.0],
        );
        post::blit(
            &mut encoder,
            "blur_v",
            &self.targets.bloom_a_view,
            wgpu::Color::BLACK,
            &self.post.blur_pipeline,
            &self.bg_from_bloom_b,
            None,
        );

        // Pass 5: composite to swapchain
        post::write_post_uniforms(
            &self.queue,
            &self.post.uniform_buffer,
            res,
            self.time_accum,
            self.ambient_energy,
            [0.0, 0.0],
        );
        post::blit(
            &mut encoder,
            "composite",
            &view,
            self.clear_color,
            &self.post.composite_pipeline,
            &self.bg_hdr,
            Some(&self.bg_bloom_a_only),
        );

        self.queue.submit(Some(encoder.finish()));
        frame.present();
        Ok(())
    }

    fn rebuild_post_bind_groups(&mut self) {
        let (bg_hdr, bg_from_a, bg_from_b, bg_a_only) = post::rebuild_bind_groups(
            &self.device,
            &self.post,
            &self.linear_sampler,
            &self.targets.hdr_view,
            &self.targets.bloom_a_view,
            &self.targets.bloom_b_view,
        );
        self.bg_hdr = bg_hdr;
        self.bg_from_bloom_a = bg_from_a;
        self.bg_from_bloom_b = bg_from_b;
        self.bg_bloom_a_only = bg_a_only;
    }
}
