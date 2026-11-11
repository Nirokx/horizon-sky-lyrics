// Sky Lyrics for Horizon, added to the renderer as `horizon_client_render::sky_lyrics`:
// the song's words drawn in the world's sky pass, before the terrain, so
// hills, trees and clouds pass in front of them.

use sky_lyrics_engine::atlas::{self, Atlas, FontFile};
use sky_lyrics_engine::config::Config;
pub use sky_lyrics_engine::config::Config as Settings;
use sky_lyrics_engine::lrc::Lyrics;
use sky_lyrics_engine::media::{Clock, Found, NowPlaying, Paths, Service};
use sky_lyrics_engine::scene::{self, Motion, Quad, View};
pub use sky_lyrics_engine::ui::Draw;
use sky_lyrics_engine::ui::{Effect, Info, Ui};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, mpsc};
use std::time::Instant;

pub const ID: &str = "sky_lyrics";
pub const NAME: &str = "Sky Lyrics";
pub const VERSION: &str = "1.0.0";
pub const DESCRIPTION: &str = "The words of the song you are playing, floating in the sky: K opens every setting";
pub const KEY_TOGGLE: &str = "toggle";
pub const DEFAULT_TOGGLE: &str = "key.keyboard.j";
pub const KEY_MENU: &str = "menu";
pub const DEFAULT_MENU: &str = "key.keyboard.k";

/// The game's folder: the process runs in it from its first lines on.
fn game_dir() -> PathBuf {
    std::env::current_dir().unwrap_or_default()
}
fn data_dir() -> PathBuf {
    game_dir().join("config").join("sky_lyrics")
}
fn config_path() -> PathBuf {
    game_dir().join("config").join("sky_lyrics.cfg")
}

const SHADER: &str = r#"
struct Uniforms { clip: mat4x4<f32> };
@group(0) @binding(0) var<uniform> uniforms: Uniforms;
@group(0) @binding(1) var field: texture_2d<f32>;
@group(0) @binding(2) var field_sampler: sampler;

struct Out {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) fill: vec4<f32>,
    @location(2) line: vec4<f32>,
    @location(3) glow: vec4<f32>,
    @location(4) params: vec4<f32>,
};

@vertex
fn vs(
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) fill: vec4<f32>,
    @location(3) line: vec4<f32>,
    @location(4) glow: vec4<f32>,
    @location(5) params: vec4<f32>,
) -> Out {
    var out: Out;
    out.position = uniforms.clip * vec4<f32>(position, 1.0);
    out.uv = uv;
    out.fill = fill;
    out.line = line;
    out.glow = glow;
    out.params = params;
    return out;
}

// Over, in premultiplied colour.
fn over(under: vec4<f32>, rgb: vec3<f32>, a: f32) -> vec4<f32> {
    return vec4<f32>(rgb * a + under.rgb * (1.0 - a), a + under.a * (1.0 - a));
}

@fragment
fn fs(in: Out) -> @location(0) vec4<f32> {
    let d = textureSample(field, field_sampler, in.uv).r;
    let soft = max(fwidth(d) * 0.75, 0.004);
    let edge = 0.5 - in.params.x;
    var colour = vec4<f32>(0.0);
    if (in.params.z > 0.0 && in.glow.a > 0.0) {
        let start = max(edge - in.params.z, 0.01);
        let t = clamp((d - start) / max(edge - start, 0.001), 0.0, 1.0);
        let a = clamp(t * t * in.params.w, 0.0, 1.0) * in.glow.a;
        colour = over(colour, in.glow.rgb, a);
    }
    if (in.params.y > 0.0 && in.line.a > 0.0) {
        let outer = max(edge - in.params.y, 0.02);
        colour = over(colour, in.line.rgb, smoothstep(outer - soft, outer + soft, d) * in.line.a);
    }
    colour = over(colour, in.fill.rgb, smoothstep(edge - soft, edge + soft, d) * in.fill.a);
    if (colour.a <= 0.002) {
        discard;
    }
    return colour;
}
"#;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 3],
    uv: [f32; 2],
    fill: [u8; 4],
    line: [u8; 4],
    glow: [u8; 4],
    params: [f32; 4],
}

struct Gpu {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    uniform: wgpu::Buffer,
    sampler: wgpu::Sampler,
    group: Option<wgpu::BindGroup>,
    vertices: wgpu::Buffer,
    capacity: u64,
    atlas_id: u64,
}

fn vertex_buffer(device: &wgpu::Device, capacity: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Sky Lyrics letters"),
        size: capacity * std::mem::size_of::<Vertex>() as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

impl Gpu {
    fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Gpu {
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Sky Lyrics view"),
            size: 64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Sky Lyrics bindings"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Sky Lyrics sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Sky Lyrics shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Sky Lyrics layout"),
            bind_group_layouts: &[Some(&layout)],
            ..Default::default()
        });
        let attributes = wgpu::vertex_attr_array![
            0 => Float32x3,
            1 => Float32x2,
            2 => Unorm8x4,
            3 => Unorm8x4,
            4 => Unorm8x4,
            5 => Float32x4,
        ];
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Sky Lyrics"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &attributes,
                })],
            },
            primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
            // As the stars: the terrain, drawn next, covers it.
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::Always),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let capacity = 6 * 1024;
        Gpu {
            pipeline,
            layout,
            uniform,
            sampler,
            group: None,
            vertices: vertex_buffer(device, capacity),
            capacity,
            atlas_id: 0,
        }
    }

    fn upload_atlas(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, atlas: &Atlas, id: u64) {
        let size = wgpu::Extent3d { width: atlas.size, height: atlas.size, depth_or_array_layers: 1 };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Sky Lyrics field"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &atlas.pixels,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(atlas.size), rows_per_image: Some(atlas.size) },
            size,
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Sky Lyrics group"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: self.uniform.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&view) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&self.sampler) },
            ],
        }));
        self.atlas_id = id;
    }
}

struct AtlasJob {
    id: u64,
    font: String,
    text: String,
}

/// Builds atlases off the render thread, keeping loaded fonts.
fn atlas_worker(jobs: mpsc::Receiver<AtlasJob>, done: Arc<Mutex<Option<(u64, Atlas)>>>, list: Arc<Mutex<Arc<Vec<FontFile>>>>) {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let own = data_dir().join("fonts");
    let _ = std::fs::create_dir_all(&own);
    let found = Arc::new(atlas::scan(&own, home.as_deref()));
    // The list shows only fonts that can write text; missing letters may
    // still come from any of them.
    let all = found.clone();
    let _ = std::thread::Builder::new().name("sky-lyrics-fonts".into()).spawn(move || {
        let good: Vec<FontFile> = all.iter().filter(|f| atlas::usable(&f.path)).cloned().collect();
        *list.lock().unwrap_or_else(|e| e.into_inner()) = Arc::new(good);
    });
    let mut loaded: HashMap<PathBuf, Option<atlas::FontArc>> = HashMap::new();
    while let Ok(mut job) = jobs.recv() {
        while let Ok(newer) = jobs.try_recv() {
            job = newer;
        }
        let chars = atlas::letters(&job.text);
        let mut fonts = Vec::new();
        let mut used = Vec::new();
        for path in atlas::resolve(&job.font, &found) {
            let font = loaded.entry(path.clone()).or_insert_with(|| atlas::load_font(&path)).clone();
            let Some(font) = font else {
                continue;
            };
            fonts.push(font);
            used.push(path);
            // Further fonts load only for letters the ones so far lack.
            use atlas::Font as _;
            if chars.iter().all(|c| c.is_whitespace() || fonts.iter().any(|f| f.glyph_id(*c).0 != 0)) {
                break;
            }
        }
        loaded.retain(|path, _| used.contains(path));
        let built = atlas::build(fonts, &chars);
        *done.lock().unwrap_or_else(|e| e.into_inner()) = Some((job.id, built));
    }
}

pub struct State {
    cfg: Config,
    ui: Ui,
    open: bool,
    close_asked: bool,
    active: bool,
    notes: Vec<String>,
    service: Option<Service>,
    clock: Clock,
    motion: Motion,
    started: Instant,
    last_draw: Option<Instant>,
    now_playing: Option<NowPlaying>,
    found: Found,
    players: Vec<String>,
    no_playerctl: bool,
    sample: Arc<Lyrics>,
    atlas: Arc<Atlas>,
    /// The newest atlas asked for.
    atlas_id: u64,
    /// The atlas in `atlas`: only this one may go to the GPU.
    atlas_live: u64,
    atlas_font: Option<String>,
    polled: Option<Instant>,
    found_generation: Option<u64>,
    jobs: Option<mpsc::Sender<AtlasJob>>,
    atlas_done: Arc<Mutex<Option<(u64, Atlas)>>>,
    fonts: Arc<Mutex<Arc<Vec<FontFile>>>>,
    device: Option<wgpu::Device>,
    format: wgpu::TextureFormat,
    gpu: Option<Gpu>,
    scratch: Vec<Vertex>,
    pending: u32,
}

impl Default for State {
    fn default() -> Self {
        let (cfg, bad) = Config::load(&config_path());
        let notes = if bad.is_empty() {
            Vec::new()
        } else {
            vec![format!("Sky Lyrics: ignored in the config file: {}", bad.join(", "))]
        };
        State {
            cfg,
            ui: Ui::default(),
            open: false,
            close_asked: false,
            active: false,
            notes,
            service: None,
            clock: Clock::default(),
            motion: Motion::default(),
            started: Instant::now(),
            last_draw: None,
            now_playing: None,
            found: Found::Nothing,
            players: Vec::new(),
            no_playerctl: false,
            sample: Arc::new(scene::sample()),
            atlas: Arc::new(Atlas::empty()),
            atlas_id: 0,
            atlas_live: 0,
            atlas_font: None,
            polled: None,
            found_generation: None,
            jobs: None,
            atlas_done: Arc::new(Mutex::new(None)),
            fonts: Arc::new(Mutex::new(Arc::new(Vec::new()))),
            device: None,
            format: wgpu::TextureFormat::Bgra8Unorm,
            gpu: None,
            scratch: Vec::new(),
            pending: 0,
        }
    }
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

pub fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    let mut guard = STATE.lock().unwrap_or_else(|e| e.into_inner());
    f(guard.get_or_insert_with(State::default))
}

/// A new renderer's device: pipelines are remade on it.
pub fn renderer_made(device: &wgpu::Device, format: wgpu::TextureFormat) {
    with(|s| {
        s.device = Some(device.clone());
        s.format = format;
        s.gpu = None;
    });
}

/// Before the frame: this frame's letters and view go to the GPU.
pub fn prepare(queue: &wgpu::Queue, clip: [[f32; 4]; 4], yaw: f32, pitch: f32, sky: [f32; 3], title: bool) {
    with(|s| {
        s.pending = 0;
        if !title {
            s.pending = s.prepare_sky(queue, clip, yaw, pitch, sky);
        }
    });
}

/// In the sky pass, at the stars' turn.
pub fn draw(pass: &mut wgpu::RenderPass<'_>) {
    with(|s| {
        let count = std::mem::take(&mut s.pending);
        if count == 0 {
            return;
        }
        let Some(gpu) = s.gpu.as_ref() else {
            return;
        };
        let Some(group) = gpu.group.as_ref() else {
            return;
        };
        pass.set_pipeline(&gpu.pipeline);
        pass.set_bind_group(0, group, &[]);
        pass.set_vertex_buffer(0, gpu.vertices.slice(..));
        pass.draw(0..count, 0..1);
    });
}

/// Whether the sky pass should run its stars step for the lyrics.
pub fn wants_sky() -> bool {
    with(|s| s.active && (s.cfg.shown || s.open))
}

impl State {
    pub fn set_active(&mut self, active: bool) {
        self.active = active;
    }

    pub fn take_notes(&mut self) -> Vec<String> {
        std::mem::take(&mut self.notes)
    }

    pub fn take_close(&mut self) -> bool {
        std::mem::take(&mut self.close_asked)
    }

    pub fn sync_open(&mut self, open: bool) {
        if self.open && !open {
            self.ui.close_views();
            self.save();
        }
        self.open = open;
    }

    pub fn typing(&self) -> bool {
        self.ui.typing()
    }

    pub fn shown(&self) -> bool {
        self.cfg.shown
    }

    pub fn toggle(&mut self) {
        self.cfg.shown = !self.cfg.shown;
        self.save();
    }

    pub fn save(&mut self) {
        if let Err(e) = self.cfg.save(&config_path()) {
            self.notes.push(format!("Sky Lyrics: could not save the config: {e}"));
        }
    }

    pub fn reload(&mut self) {
        let (cfg, bad) = Config::load(&config_path());
        self.cfg = cfg;
        if !bad.is_empty() {
            self.notes.push(format!("Sky Lyrics: ignored in the config file: {}", bad.join(", ")));
        }
    }

    pub fn defaults(&mut self) {
        self.cfg = Config::default();
        self.save();
    }

    /// Background work starts once the lyrics are wanted.
    fn wake(&mut self) {
        if self.jobs.is_none() {
            let (send, receive) = mpsc::channel();
            self.jobs = Some(send);
            let (done, list) = (self.atlas_done.clone(), self.fonts.clone());
            let _ = std::thread::Builder::new()
                .name("sky-lyrics-atlas".into())
                .spawn(move || atlas_worker(receive, done, list));
        }
        if self.service.is_none() {
            let dir = data_dir();
            self.service = Some(Service::start(Paths { lyrics: dir.join("lyrics"), cache: dir.join("cache") }));
        }
    }

    /// Copies what the threads found, only when they found something new, and
    /// asks for an atlas when the text or the font changed.
    fn refresh(&mut self) {
        self.wake();
        let mut new_text = false;
        if let Some(service) = &self.service {
            service.keep_alive(&self.cfg.player, self.cfg.online);
            let shared = service.shared.lock().unwrap_or_else(|e| e.into_inner());
            if shared.polled != self.polled {
                self.polled = shared.polled;
                self.now_playing.clone_from(&shared.now);
                self.players.clone_from(&shared.players);
                self.no_playerctl = shared.no_playerctl;
            }
            if self.found_generation != Some(shared.generation) {
                self.found_generation = Some(shared.generation);
                self.now_playing.clone_from(&shared.now);
                self.found.clone_from(&shared.found);
                // The search's answer brings the letters; until then the last set stands.
                new_text = !matches!(self.found, Found::Searching);
            }
        }
        if let (Some(np), Some(polled)) = (&self.now_playing, self.polled) {
            self.clock.update(np, polled);
        }
        if new_text || self.atlas_font.as_deref() != Some(self.cfg.font.as_str()) {
            let (title, artist) = self.now_playing.as_ref().map_or(("", ""), |n| (n.title.as_str(), n.artist.as_str()));
            let lyric_text = match &self.found {
                Found::Lyrics(l) => l.text(),
                _ => String::new(),
            };
            let text = format!("{title} {artist} {lyric_text} {}", self.sample.text());
            self.atlas_font = Some(self.cfg.font.clone());
            self.atlas_id += 1;
            if let Some(jobs) = &self.jobs {
                let _ = jobs.send(AtlasJob { id: self.atlas_id, font: self.cfg.font.clone(), text });
            }
        }
    }

    /// The lyrics to show now and where in them we are.
    fn current(&self, now: Instant) -> Option<(Arc<Lyrics>, i64, bool)> {
        let live = self.now_playing.as_ref().and_then(|np| {
            let Found::Lyrics(lyrics) = &self.found else {
                return None;
            };
            if !np.playing && self.cfg.when_paused == "hide" {
                return None;
            }
            if !lyrics.synced && self.cfg.unsynced == "hide" {
                return None;
            }
            Some((lyrics.clone(), i64::from(self.clock.now(now)) + self.cfg.offset as i64, true))
        });
        live.or_else(|| {
            self.open.then(|| {
                let at = self.started.elapsed().as_millis() as i64 % scene::SAMPLE_LOOP_MS;
                (self.sample.clone(), at, false)
            })
        })
    }

    /// Lays out and uploads this frame's letters: how many vertices to draw.
    fn prepare_sky(&mut self, queue: &wgpu::Queue, clip: [[f32; 4]; 4], yaw: f32, pitch: f32, sky: [f32; 3]) -> u32 {
        if !self.active || !(self.cfg.shown || self.open) {
            self.last_draw = None;
            return 0;
        }
        let Some(device) = self.device.clone() else {
            return 0;
        };
        let now = Instant::now();
        let dt = self.last_draw.map_or(0.0, |t| now.duration_since(t).as_secs_f32().min(0.25));
        self.last_draw = Some(now);
        self.refresh();
        if let Some((id, built)) = self.atlas_done.lock().unwrap_or_else(|e| e.into_inner()).take() {
            if id == self.atlas_id {
                self.atlas = Arc::new(built);
                self.atlas_live = id;
            }
        }
        let time = self.started.elapsed().as_secs_f32();
        let view = View { yaw, pitch, dt, time, sky };
        let Some((lyrics, at, live)) = self.current(now) else {
            return 0;
        };
        let title = self
            .now_playing
            .as_ref()
            .filter(|_| live)
            .map(|n| (n.title.as_str(), n.artist.as_str()));
        let mut paras = scene::timeline(&self.cfg, &lyrics, at, title);
        let mut at = at;
        // Between lines the settings screen still needs something to preview on.
        let sample = self.sample.clone();
        if paras.is_empty() && self.open {
            at = self.started.elapsed().as_millis() as i64 % scene::SAMPLE_LOOP_MS;
            paras = scene::timeline(&self.cfg, &sample, at, None);
        }
        if paras.is_empty() || self.atlas.glyphs.is_empty() {
            return 0;
        }
        let quads = scene::build(&self.cfg, &self.atlas, &view, &mut self.motion, &paras, at);
        if quads.is_empty() {
            return 0;
        }
        self.scratch.clear();
        for quad in &quads {
            push_quad(&mut self.scratch, quad);
        }
        if self.gpu.is_none() {
            self.gpu = Some(Gpu::new(&device, self.format));
        }
        let Some(gpu) = self.gpu.as_mut() else {
            return 0;
        };
        if gpu.atlas_id != self.atlas_live || gpu.group.is_none() {
            gpu.upload_atlas(&device, queue, &self.atlas, self.atlas_live);
        }
        if self.scratch.len() as u64 > gpu.capacity {
            gpu.capacity = (self.scratch.len() as u64).next_power_of_two();
            gpu.vertices = vertex_buffer(&device, gpu.capacity);
        }
        queue.write_buffer(&gpu.uniform, 0, bytemuck::cast_slice(&clip));
        queue.write_buffer(&gpu.vertices, 0, bytemuck::cast_slice(&self.scratch));
        self.scratch.len() as u32
    }

    /// The line for the HUD, when that is on: (text, corner index).
    pub fn hud_line(&self) -> Option<(String, usize)> {
        let corner = ["top_left", "top_right", "bottom_left", "bottom_right"]
            .iter()
            .position(|c| *c == self.cfg.hud)?;
        if !self.active || !self.cfg.shown {
            return None;
        }
        let (lyrics, at, live) = self.current(Instant::now())?;
        if !live {
            return None;
        }
        Some((scene::current_text(&self.cfg, &lyrics, at)?, corner))
    }

    fn status(&self) -> String {
        if self.no_playerctl {
            "playerctl is not installed".to_owned()
        } else {
            self.found.describe().to_owned()
        }
    }

    fn song(&self) -> String {
        self.now_playing.as_ref().map_or_else(
            || "nothing".to_owned(),
            |n| if n.artist.is_empty() { n.title.clone() } else { format!("{} - {}", n.artist, n.title) },
        )
    }

    /// The settings screen's drawing.
    pub fn screen(&mut self, size: [f32; 2], mouse: [f32; 2], measure: &dyn Fn(&str) -> f32) -> Vec<Draw> {
        let (status, song) = (self.status(), self.song());
        let fonts = self.fonts.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let players = self.players.clone();
        let info = Info { song: &song, status: &status, players: &players, fonts: &fonts };
        self.ui.render(&self.cfg, size, mouse, &info, measure)
    }

    pub fn press(&mut self, mouse: [f32; 2]) {
        let players = self.players.clone();
        let effect = self.ui.press(&mut self.cfg, mouse, &players);
        self.apply(effect);
    }

    fn apply(&mut self, effect: Effect) {
        match effect {
            Effect::None => {}
            // A held slider writes the file when it is let go.
            Effect::Changed => {
                if !self.ui.sliding() {
                    self.save();
                }
            }
            Effect::LookUpAgain => {
                if let Some(service) = &self.service {
                    service.refetch();
                }
            }
            Effect::ReloadFile => self.reload(),
            Effect::Close => self.close_asked = true,
        }
    }

    pub fn dragged(&mut self, mouse: [f32; 2]) {
        self.ui.dragged(&mut self.cfg, mouse);
    }

    pub fn released(&mut self) {
        if self.ui.released() {
            self.save();
        }
    }

    pub fn wheel(&mut self, steps: f64) {
        self.ui.wheel(steps);
    }

    /// True when the screen used the key.
    pub fn key(&mut self, key: &str) -> bool {
        let (used, effect) = self.ui.key(&mut self.cfg, key);
        self.apply(effect);
        used
    }

    pub fn typed(&mut self, c: char) {
        self.ui.typed(c);
    }

    pub fn close(&mut self) {
        self.close_asked = true;
    }
}

fn push_quad(out: &mut Vec<Vertex>, quad: &Quad) {
    let [u0, v0, u1, v1] = quad.uv;
    let uvs = [[u0, v0], [u1, v0], [u1, v1], [u0, v1]];
    for corner in [0usize, 1, 2, 0, 2, 3] {
        out.push(Vertex {
            position: quad.corners[corner],
            uv: uvs[corner],
            fill: quad.fill,
            line: quad.line,
            glow: quad.glow,
            params: quad.params,
        });
    }
}
