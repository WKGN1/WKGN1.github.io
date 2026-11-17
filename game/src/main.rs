//! 方块彩蛋的主程序：输入 → 物理 → 网格 → 渲染 → 存档。
//!
//! 渲染走 macroquad 的自定义材质（不是内置 PBR）：顶点里塞了「朝向明暗 × AO」和
//! 「自发光」两个数，昼夜和雾在片元里算。这样 shading 全在网格生成时算完，
//! 每帧的 GPU 活儿只剩一次采样 + 一次混合，手机才顶得住。
//!
//! 和 JS 的通信刻意做成最蠢的样子：Rust 通过 wasm 导入调 4 个 JS 函数，
//! 参数只有 (指针, 长度)。好处是不用 wasm-bindgen，构建就一条 cargo 命令。

mod blocks;
mod mesh;
mod noise;
mod phys;
mod ray;
mod save;
mod world;

use macroquad::prelude::*;

use blocks as b;
use mesh::ChunkMesh;
use phys::Player;
use world::World;

/// 固定种子：文章截图、读者之间讨论同一座岛都对得上。
const SEED: u32 = 20261117;

/// 一整个昼夜循环的秒数。太长读者感受不到，太短会晕。
const DAY_SECONDS: f32 = 360.0;

const REACH: f32 = 6.5;
const FOG_NEAR: f32 = 26.0;

/// 视距。地图只有 96 宽，远处的山会被雾吃掉，所以这个数字主要影响雾的观感。
const FOG_FAR: f32 = 96.0;

/// 每帧最多重建几个区块网格。启动时 36 个要建，分摊一下免得卡成一帧。
const MESH_BUDGET: usize = 12;

// ---------------------------------------------------------------------------
// 和 JS 的桥（wasm 导入）
// ---------------------------------------------------------------------------

// 必须写明导入模块是 "env"：不写的话 lld 把这四个当成本模块内的未定义符号，
// 链接直接失败；写了才变成 wasm 导入，交给浏览器的胶水脚本提供（miniquad 自己也是这么声明的）。
#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "env")]
extern "C" {
    /// 把 HUD 文本交给页面（Rust 每 0.2 秒推一次）。
    fn mcl_hud(ptr: *const u8, len: u32);
    /// 把整份存档交给 JS 去写 IndexedDB。
    fn mcl_save_request(ptr: *const u8, len: u32);
    /// JS 那边还在排队写入的次数，用来显示「保存中」。
    fn mcl_save_pending() -> u32;
    /// 0=读盘中 1=没有存档 2=有存档可读。
    fn mcl_save_ready() -> u32;
    /// 把存档字节写进 wasm 内存，返回真实长度（0 表示没有）。
    fn mcl_pull_save(ptr: *mut u8, cap: u32) -> u32;
    /// 告诉页面「世界建好了」：门面该收，HUD 该亮。
    fn mcl_booted();
    /// 往页面和 console 里写一句话（启动阶段、panic 原因都走这里）。
    fn mcl_log(ptr: *const u8, len: u32);
}

/// 插件版本握手：JS 侧的 miniquad_add_plugin 会调这个函数核对版本，没有就刷一条警告。
#[cfg(target_arch = "wasm32")]
#[no_mangle]
pub extern "C" fn mcl_bridge_crate_version() -> u32 {
    1
}

mod bridge {
    use super::*;

    #[inline]
    fn send(s: &str) {
        #[cfg(target_arch = "wasm32")]
        unsafe {
            mcl_hud(s.as_ptr(), s.len() as u32);
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = s;
        }
    }

    pub fn hud(text: &str) {
        send(text);
    }

    /// 只在从 Waiting 进 Running 的那一帧叫一次。
    pub fn booted() {
        #[cfg(target_arch = "wasm32")]
        unsafe {
            mcl_booted();
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            println!("世界就绪");
        }
    }

    /// 往页面/console 写一句话；桌面版就走 stderr。
    pub fn log(text: &str) {
        #[cfg(target_arch = "wasm32")]
        unsafe {
            mcl_log(text.as_ptr(), text.len() as u32);
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            eprintln!("{}", text);
        }
    }

    /// 崩了别只留一句「这浏览器跑不了」：把原因和行列号送到页面上去。
    pub fn install_panic_hook() {
        std::panic::set_hook(Box::new(|info| {
            let loc = info
                .location()
                .map(|l| format!("（{} 第 {} 行）", l.file().rsplit("/").next().unwrap_or("?"), l.line()))
                .unwrap_or_default();
            let msg = info.payload().downcast_ref::<String>().cloned()
                .or_else(|| info.payload().downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_else(|| "未知原因".to_string());
            log(&format!("彩蛋崩了：{}{}", msg, loc));
        }));
    }

    pub fn save_request(bytes: &[u8]) {
        #[cfg(target_arch = "wasm32")]
        unsafe {
            mcl_save_request(bytes.as_ptr(), bytes.len() as u32);
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            // 桌面调试用：写在本目录，方便直接看/删，也顺便验证了编码。
            if std::fs::write("save.mcl", bytes).is_err() {
                eprintln!("存档写不出去（目录不可写？），继续玩但不会保留");
            }
        }
    }

    /// 返回 Some(已排队次数) 或 None（本地无法得知）。
    pub fn save_pending() -> Option<u32> {
        #[cfg(target_arch = "wasm32")]
        {
            Some(unsafe { mcl_save_pending() })
        }
        #[cfg(not(target_arch = "wasm32"))]
        None
    }

    /// 0=还没读好 1=没有存档 2=有存档。
    pub fn save_ready() -> u32 {
        #[cfg(target_arch = "wasm32")]
        {
            unsafe { mcl_save_ready() }
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            // 桌面版：文件在就读，不在就没有。第一次查询就把结果定下来。
            if std::fs::File::open("save.mcl").is_ok() {
                2
            } else {
                1
            }
        }
    }

    /// 拉存档字节。返回 Some(字节) / None（还没有或读不到）。
    pub fn pull_save() -> Option<Vec<u8>> {
        #[cfg(target_arch = "wasm32")]
        {
            // JS 需要一块落地区。存档实测几十 KB，256 KB 留足余量；
            // wasm 内存只能长不能缩，所以别贪大。
            const CAP: usize = 256 * 1024;
            let mut buf: Vec<u8> = vec![0u8; CAP];
            let n = unsafe { mcl_pull_save(buf.as_mut_ptr(), CAP as u32) } as usize;
            if n == 0 || n > CAP {
                return None;
            }
            buf.truncate(n);
            Some(buf)
        }
        #[cfg(not(target_arch = "wasm32"))]
        std::fs::read("save.mcl").ok()
    }
}

// ---------------------------------------------------------------------------
// 材质与着色器
// ---------------------------------------------------------------------------

const VERT: &str = r#"#version 100
attribute vec3 position;
attribute vec2 texcoord;
attribute vec4 color0;

varying lowp vec2 uv;
varying lowp float shade;
varying lowp float emissive;
varying mediump vec3 wpos;

uniform mat4 Model;
uniform mat4 Projection;

void main() {
    // color0 是 Byte4，不是归一化的，所以要自己除 255。
    // .r = 朝向明暗 × AO，.g = 是否自发光。
    mediump vec4 world = Model * vec4(position, 1);
    gl_Position = Projection * world;
    uv = texcoord;
    shade = color0.r / 255.0;
    emissive = color0.g / 255.0;
    wpos = world.xyz;
}
"#;

const FRAG: &str = r#"#version 100
precision mediump float;

varying lowp vec2 uv;
varying lowp float shade;
varying lowp float emissive;
varying mediump vec3 wpos;

uniform sampler2D Texture;
uniform float u_day;        // 0..1，夜里压暗到 0.3 左右
uniform vec3 u_eye;
uniform vec3 u_fog_color;
uniform float u_fog_near;
uniform float u_fog_far;

void main() {
    vec4 tex = texture2D(Texture, uv);
    float light = mix(0.32, 1.0, u_day);
    vec3 col = tex.rgb * shade * light;
    // 自发光方块不吃昼夜和 AO，夜里能看到自己亮的灯。
    col = mix(col, tex.rgb * 1.12, emissive);

    float d = distance(wpos, u_eye);
    float f = clamp((d - u_fog_near) / max(u_fog_far - u_fog_near, 0.001), 0.0, 1.0);
    col = mix(col, u_fog_color, f * f);

    gl_FragColor = vec4(col, tex.a);
}
"#;

fn make_material() -> Material {
    let params = macroquad::material::MaterialParams {
        pipeline_params: macroquad::miniquad::PipelineParams {
            // 先不背面剔除：这个世界的网格只生成「露在外面的面」，
            // 剔除能省的片元本来就少，先把「面会不会因为绕向反了而消失」这个风险摘掉。
            // 确认绕向没问题后改成 CullFace::Back 更省填充率。
            cull_face: macroquad::miniquad::CullFace::Nothing,
            front_face_order: macroquad::miniquad::FrontFaceOrder::CounterClockwise,
            depth_test: macroquad::miniquad::Comparison::Less,
            depth_write: true,
            ..Default::default()
        },
        uniforms: vec![
            macroquad::miniquad::UniformDesc::new("u_day", macroquad::miniquad::UniformType::Float1),
            macroquad::miniquad::UniformDesc::new("u_eye", macroquad::miniquad::UniformType::Float3),
            macroquad::miniquad::UniformDesc::new(
                "u_fog_color",
                macroquad::miniquad::UniformType::Float3,
            ),
            macroquad::miniquad::UniformDesc::new(
                "u_fog_near",
                macroquad::miniquad::UniformType::Float1,
            ),
            macroquad::miniquad::UniformDesc::new("u_fog_far", macroquad::miniquad::UniformType::Float1),
        ],
        // "Texture" 是 macroquad 的保留槽名：着色器里照写 uniform sampler2D Texture，
        // 但别列进这里——列了 make_pipeline 直接 panic（quad_gl.rs 里那句
        // "you can't use name `Texture` ..."）。绘制时传进来的贴图会自动占住这个槽。
        textures: vec![],
    };

    load_material(
        ShaderSource::Glsl {
            vertex: VERT,
            fragment: FRAG,
        },
        params,
    )
    .expect("材质编译失败：着色器哪里写错了")
}

/// 程序化纹理图集，没有一张外部图片。
fn make_atlas() -> Texture2D {
    let (w, h, bytes) = (b::ATLAS_W, b::ATLAS_H, b::atlas_rgba());
    let tex = Texture2D::from_rgba8(w as u16, h as u16, &bytes);
    // macroquad 0.4.16 只开了 set_filter，没有 set_wrap；
    // 底层默认就是 ClampToEdge，而我给的 UV 已经内缩半像素，不会漏采到邻贴。
    tex.set_filter(FilterMode::Nearest);
    tex
}

// ---------------------------------------------------------------------------
// 天空与昼夜
// ---------------------------------------------------------------------------

/// 三段天空色按时间插值：白天 → 黄昏 → 夜 → 黎明 → 白天。
fn sky_color(t: f32) -> Color {
    const STOPS: [(f32, [f32; 3]); 5] = [
        (0.0, [0.53, 0.72, 0.92]),  // 清晨偏暖的蓝
        (0.25, [0.62, 0.78, 0.95]), // 正午
        (0.50, [0.93, 0.62, 0.35]), // 黄昏橙
        (0.72, [0.05, 0.07, 0.16]), // 夜
        (1.0, [0.53, 0.72, 0.92]),  // 回到清晨
    ];
    let t = t.rem_euclid(1.0);
    for i in 0..STOPS.len() - 1 {
        let (t0, c0) = STOPS[i];
        let (t1, c1) = STOPS[i + 1];
        if t <= t1 {
            let k = ((t - t0) / (t1 - t0)).clamp(0.0, 1.0);
            let mix = |a: f32, bb: f32| a + (bb - a) * k;
            return Color::new(
                mix(c0[0], c1[0]),
                mix(c0[1], c1[1]),
                mix(c0[2], c1[2]),
                1.0,
            );
        }
    }
    Color::new(0.53, 0.72, 0.92, 1.0)
}

/// 光照强度：白天 1，夜里 0，过渡用平滑段。黄昏/黎明各占约 12%。
fn daylight(t: f32) -> f32 {
    let t = t.rem_euclid(1.0);
    if t < 0.38 {
        1.0
    } else if t < 0.52 {
        let k = (t - 0.38) / 0.14;
        1.0 - 0.5 * (k * k * (3.0 - 2.0 * k))
    } else if t < 0.86 {
        0.5
    } else {
        let k = (t - 0.86) / 0.14;
        0.5 + 0.5 * (k * k * (3.0 - 2.0 * k))
    }
    .clamp(0.0, 1.0)
}

fn clock_text(t: f32) -> String {
    let t = t.rem_euclid(1.0);
    let total = (t * 24.0 * 60.0) as i32;
    let h = (total / 60) % 24;
    let m = total % 60;
    format!("{:02}:{:02}", h, m)
}

// ---------------------------------------------------------------------------
// 游戏状态
// ---------------------------------------------------------------------------

enum Phase {
    /// 等 JS 那边的存档读完（IndexedDB 是异步的）。这段时间不动物理、不建网格。
    Waiting,
    Running,
}

struct Game {
    world: World,
    player: Player,
    selected: usize,
    time_of_day: f32,
    phase: Phase,
    dirty_chunks: Vec<bool>,
    chunks: Vec<Option<Mesh>>,
    hud_timer: f32,
    save_timer: f32,
    save_dirty: bool,
    last_fps: f32,
    touch_move: Option<(u64, Vec2, Vec2)>,
    look: Vec<LookTouch>,
    drag: (Vec2, f32, bool),
    wheel_cd: f32,
    /// 程序化图集，建一次，交给每个区块网格带着。
    atlas: Option<Texture2D>,
}

impl Game {
    fn new(world: World) -> Self {
        let spawn = world.spawn();
        Game {
            dirty_chunks: vec![true; world::CHUNK_COUNT],
            world,
            player: Player::new(spawn),
            selected: 0,
            time_of_day: 0.12,
            phase: Phase::Waiting,
            // Mesh 没有 Clone，所以不能用 vec![None; n]
            chunks: (0..world::CHUNK_COUNT).map(|_| None).collect(),
            hud_timer: 0.0,
            save_timer: 0.0,
            save_dirty: false,
            last_fps: 0.0,
            touch_move: None,
            look: Vec::new(),
            drag: (vec2(0.0, 0.0), 0.0, false),
            wheel_cd: 0.0,
            atlas: None,
        }
    }

    fn rebuild_dirty_chunks(&mut self) {
        let mut built = 0usize;
        for cz in 0..world::CHUNKS_Z {
            for cx in 0..world::CHUNKS_X {
                let i = (cz * world::CHUNKS_X + cx) as usize;
                if !self.dirty_chunks[i] {
                    continue;
                }
                self.dirty_chunks[i] = false;
                let mut m = ChunkMesh::default();
                mesh::build_chunk(&self.world, cx, cz, &mut m);

                let mut verts: Vec<Vertex> = Vec::with_capacity(m.vert_count());
                for v in m.verts.chunks_exact(mesh::STRIDE_F32) {
                    verts.push(Vertex {
                        position: vec3(v[0], v[1], v[2]),
                        uv: vec2(v[3], v[4]),
                        // .r = 明暗，.g = 自发光标记
                        color: [
                            (v[5] * 255.0) as u8,
                            if v[6] > 0.5 { 255 } else { 0 },
                            0,
                            255,
                        ],
                        normal: vec4(0.0, 0.0, 0.0, 0.0),
                    });
                }
                self.chunks[i] = Some(Mesh {
                    vertices: verts,
                    indices: m.idx,
                    texture: self.atlas.clone(),
                });

                built += 1;
                if built >= MESH_BUDGET {
                    return;
                }
            }
        }
    }

    fn mark_all_dirty(&mut self) {
        for d in self.dirty_chunks.iter_mut() {
            *d = true;
        }
    }

    /// 挖掉瞄准到的方块。
    fn mine(&mut self) {
        let eye = self.player.eye();
        let dir = self.player.look_dir();
        let Some(hit) = ray::cast(&self.world, eye, dir, REACH) else {
            return;
        };
        let [x, y, z] = hit.cell;
        if y <= 0 {
            return; // 第 0 层是世界底壳，挖穿就掉出去了
        }
        self.world.set(x, y, z, b::AIR);
        self.save_dirty = true;
    }

    /// 在瞄准到的面上放一个当前选中的方块。
    fn place(&mut self) {
        let eye = self.player.eye();
        let dir = self.player.look_dir();
        let Some(hit) = ray::cast(&self.world, eye, dir, REACH) else {
            return;
        };
        let cell = [
            hit.cell[0] + hit.normal[0],
            hit.cell[1] + hit.normal[1],
            hit.cell[2] + hit.normal[2],
        ];
        if !world::in_bounds(cell[0], cell[1], cell[2]) {
            return;
        }
        let cur = self.world.get(cell[0], cell[1], cell[2]);
        if !b::is_air(cur) && !b::is_water(cur) {
            return; // 已经有东西（水可以被替换，填海是正经玩法）
        }
        if self.player.overlaps_cell(cell) {
            return; // 别把自己砌进墙里
        }
        self.world
            .set(cell[0], cell[1], cell[2], b::HOTBAR[self.selected]);
        self.save_dirty = true;
    }

    fn pick_target(&self) -> Option<([i32; 3], [i32; 3])> {
        let eye = self.player.eye();
        let dir = self.player.look_dir();
        let hit = ray::cast(&self.world, eye, dir, REACH)?;
        Some((hit.cell, hit.normal))
    }

    fn save_now(&mut self) {
        let bytes = save::encode(&self.world);
        bridge::save_request(&bytes);
        self.save_dirty = false;
        self.save_timer = 0.0;
    }

    fn push_hud(&mut self) {
        let mode = if self.player.fly {
            "飞行"
        } else if self.player.in_water {
            "游泳"
        } else {
            "行走"
        };
        let pending = bridge::save_pending().unwrap_or(0);
        let state = match bridge::save_ready() {
            0 => "读取存档",
            1 => if self.save_dirty || pending > 0 {
                "保存中"
            } else {
                "已存档"
            },
            _ => if pending > 0 || self.save_dirty {
                "保存中"
            } else {
                "已存档"
            },
        };
        let fps = if self.last_fps > 0.5 {
            format!("{:.0}", self.last_fps)
        } else {
            "--".to_string()
        };
        let text = format!(
            "{}|{}|{}|{}|{}|{}|{}|{}",
            self.selected + 1,
            b::block_name(b::HOTBAR[self.selected]),
            mode,
            clock_text(self.time_of_day),
            self.world.changed,
            fps,
            state,
            if self.player.fly { "飞行中，空格上升 Shift 下降" } else { "" }
        );
        bridge::hud(&text);
    }
}

// ---------------------------------------------------------------------------
// 输入
// ---------------------------------------------------------------------------

/// 桌面：按住拖动转向；不移动就松手算点击。
fn read_mouse(g: &mut Game, dt: f32) {
    let (mx, my) = mouse_position();
    let cur = vec2(mx, my);

    let held = is_mouse_button_down(MouseButton::Left)
        || is_mouse_button_down(MouseButton::Right)
        || is_mouse_button_down(MouseButton::Middle);

    if held {
        if !g.drag.2 {
            // 刚按下：记下起点
            g.drag = (cur, 0.0, true);
        } else {
            let d = cur - g.drag.0;
            g.drag.1 += d.length();
            g.drag.0 = cur;
            if g.drag.1 > 6.0 {
                g.drag.2 = true;
                // 拖动即转向：整屏转 180° 大约需要 700 像素
                let sens = std::f32::consts::PI / 700.0;
                g.player.yaw += d.x * sens;
                g.player.pitch -= d.y * sens;
                clamp_pitch(&mut g.player);
            }
        }
    } else if g.drag.2 {
        // 松手：没怎么移动过的话当成一次点击
        let tapped = g.drag.1 <= 6.0;
        g.drag = (cur, 0.0, false);
        if tapped {
            if is_mouse_button_released(MouseButton::Right) {
                g.place();
            } else if is_mouse_button_released(MouseButton::Middle) {
                if let Some((cell, _)) = g.pick_target() {
                    let id = g.world.get(cell[0], cell[1], cell[2]);
                    if let Some(i) = b::HOTBAR.iter().position(|&x| x == id) {
                        g.selected = i;
                    }
                }
            } else {
                g.mine();
            }
        }
    }

    g.wheel_cd -= dt;
    let (_, dy) = mouse_wheel();
    if dy.abs() > 0.01 && g.wheel_cd <= 0.0 {
        let n = b::HOTBAR.len() as i32;
        let mut s = g.selected as i32 - dy.signum() as i32;
        if s < 0 {
            s += n;
        }
        if s >= n {
            s -= n;
        }
        g.selected = s as usize;
        g.wheel_cd = 0.12;
    }
}

/// 右半屏的一根手指。held 是按住秒数，moved 是累计位移（像素）。
#[derive(Clone, Copy)]
struct LookTouch {
    id: u64,
    last: Vec2,
    held: f32,
    moved: f32,
    placed: bool,
}

/// 触屏：左半屏摇杆走路，右半屏拖动转向，轻点挖、按住放。
/// 返回摇杆给出的移动意图（已经按屏幕方向算好，不需要键盘的朝向旋转）。
fn read_touch(g: &mut Game, dt: f32) -> Option<[f32; 3]> {
    let w = screen_width();

    for t in touches() {
        let left = t.position.x < w * 0.45;
        match t.phase {
            TouchPhase::Started => {
                if left {
                    if g.touch_move.is_none() {
                        g.touch_move = Some((t.id, t.position, t.position));
                    }
                } else {
                    g.look.retain(|l| l.id != t.id);
                    g.look.push(LookTouch {
                        id: t.id,
                        last: t.position,
                        held: 0.0,
                        moved: 0.0,
                        placed: false,
                    });
                }
            }
            TouchPhase::Moved => {
                if left {
                    if let Some((id, origin, cur)) = g.touch_move.as_mut() {
                        if *id == t.id {
                            *cur = t.position;
                            let _ = origin;
                        }
                    }
                } else if let Some(l) = g.look.iter_mut().find(|l| l.id == t.id) {
                    let d = t.position - l.last;
                    l.moved += d.length();
                    l.last = t.position;
                    let sens = std::f32::consts::PI / 420.0;
                    g.player.yaw += d.x * sens;
                    g.player.pitch -= d.y * sens;
                    clamp_pitch(&mut g.player);
                }
            }
            // 手指按着没动：保持现状就行，摇杆和视角都不需要更新
            TouchPhase::Stationary => {}
            TouchPhase::Ended | TouchPhase::Cancelled => {
                if left {
                    if let Some((id, _, _)) = g.touch_move {
                        if id == t.id {
                            g.touch_move = None;
                        }
                    }
                } else {
                    // 抬手时既没长按触发放置、也没怎么移动，就当一次「挖」。
                    let hit = g.look.iter().find(|l| l.id == t.id).copied();
                    g.look.retain(|l| l.id != t.id);
                    if let Some(l) = hit {
                        if !l.placed && l.held < 0.45 && l.moved < 14.0 {
                            g.mine();
                        }
                    }
                }
            }
        }
    }

    // 按住不放：超过 0.45 秒且没怎么移动就放一个方块，一次按住只放一次。
    // 先只记「该放几个」，再统一执行：place() 要 &mut Game，不能压在 iter_mut 上。
    let mut aged = 0usize;
    for l in g.look.iter_mut() {
        l.held += dt;
        if l.held > 0.45 && l.moved < 24.0 && !l.placed {
            l.placed = true;
            aged += 1;
        }
    }
    for _ in 0..aged {
        g.place();
    }
    g.look.retain(|l| l.held < 30.0);

    if let Some((_, origin, cur)) = g.touch_move {
        let d = cur - origin;
        if d.length() >= 8.0 {
            let len = d.length();
            let k = len.min(52.0) / 52.0 / len;
            return Some([d.x * k, 0.0, d.y * k]);
        }
    }
    None
}

#[inline]
fn clamp_pitch(p: &mut Player) {
    let lim = std::f32::consts::FRAC_PI_2 - 0.02;
    p.pitch = p.pitch.clamp(-lim, lim);
}

/// 把 WASD 的按键意图按当前朝向转成世界坐标方向。
fn wish_from_keys(p: &Player) -> [f32; 3] {
    let mut f = 0.0f32;
    let mut r = 0.0f32;
    if is_key_down(KeyCode::W) {
        f += 1.0;
    }
    if is_key_down(KeyCode::S) {
        f -= 1.0;
    }
    if is_key_down(KeyCode::D) {
        r += 1.0;
    }
    if is_key_down(KeyCode::A) {
        r -= 1.0;
    }
    if f == 0.0 && r == 0.0 {
        return [0.0, 0.0, 0.0];
    }
    let (sy, cy) = (p.yaw.sin(), p.yaw.cos());
    let fwd = [sy, 0.0, cy];
    let right = [cy, 0.0, -sy];
    let mut d = [fwd[0] * f + right[0] * r, 0.0, fwd[2] * f + right[2] * r];
    let len = (d[0] * d[0] + d[2] * d[2]).sqrt();
    if len > 1.0 {
        d[0] /= len;
        d[2] /= len;
    }
    d
}

// ---------------------------------------------------------------------------
// 入口
// ---------------------------------------------------------------------------

/// 把 JS 递过来的存档灌进世界。读不懂就当坏档：按种子重来，并标记待存覆盖掉它。
fn apply_loaded_save(game: &mut Game) {
    let Some(bytes) = bridge::pull_save() else {
        return;
    };
    match save::decode(&bytes) {
        Ok((seed, raw)) => {
            let Some(w) = World::from_raw(seed, raw) else {
                return;
            };
            game.world = w;
            game.player = Player::new(game.world.spawn());
            // 拿到旧存档就不再自动存一份，除非读者真的改动过
        }
        Err(_) => {
            game.world = World::new(SEED);
            game.player = Player::new(game.world.spawn());
            game.save_dirty = true;
        }
    }
}

#[macroquad::main("方块彩蛋 · Rust + wasm")]
async fn main() {
    bridge::install_panic_hook();
    bridge::log("rust 起来了，开始建材质");
    let atlas = make_atlas();
    bridge::log("纹理图集好了");
    let material = make_material();
    bridge::log("材质编译通过");
    // 图集交给每个区块网格带着：保留槽 "Texture" 在每次 draw 时自动绑成这张。
    // （Texture2D 的 Clone 只是个句柄拷贝，36 个网格各拿一份不贵。）

    let mut game = Game::new(World::new(SEED));
    game.atlas = Some(atlas);
    let mut fps_smooth = 0.0f32;
    let mut fps_report = 0.0f32;

    loop {
        let dt = get_frame_time().min(0.1);
        fps_smooth = fps_smooth * 0.92 + (1.0 / dt.max(0.0001)) * 0.08;
        fps_report -= dt;
        if fps_report <= 0.0 {
            game.last_fps = fps_smooth;
            fps_report = 0.5;
        }

        // 1) 等存档。IndexedDB 读盘是异步的，读好之前不动物理、不建网格，
        //     免得先按种子建一遍、拿到存档再建一遍。
        if matches!(game.phase, Phase::Waiting) {
            match bridge::save_ready() {
                0 => {
                    game.push_hud();
                    next_frame().await;
                    continue;
                }
                st => {
                    if st == 2 {
                        apply_loaded_save(&mut game);
                    }
                    game.mark_all_dirty();
                    game.phase = Phase::Running;
                    bridge::booted();
                }
            }
        }

        // 2) 输入
        if matches!(game.phase, Phase::Running) {
            read_mouse(&mut game, dt);
            let mut wish = wish_from_keys(&game.player);
            if let Some(w) = read_touch(&mut game, dt) {
                wish = w;
            }

            for (i, key) in [
                (0usize, KeyCode::Key1),
                (1, KeyCode::Key2),
                (2, KeyCode::Key3),
                (3, KeyCode::Key4),
                (4, KeyCode::Key5),
                (5, KeyCode::Key6),
                (6, KeyCode::Key7),
                (7, KeyCode::Key8),
                (8, KeyCode::Key9),
            ] {
                if is_key_pressed(key) {
                    game.selected = i;
                }
            }

            if is_key_pressed(KeyCode::F) {
                game.player.fly = !game.player.fly;
                game.player.vel[1] = 0.0;
            }

            let jump = is_key_down(KeyCode::Space);
            let down_key = is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift);
            // 行走时 Shift 是加速，飞行时 Shift 是下降，两者不能同时生效。
            let sprint = down_key && !game.player.fly;

            game.time_of_day = (game.time_of_day + dt / DAY_SECONDS).rem_euclid(1.0);
            game.player
                .update(&game.world, dt, wish, jump, down_key && !sprint, sprint);

            // 3) 自动存档：改动之后 3 秒写一次
            game.save_timer += dt;
            if game.save_dirty && game.save_timer > 3.0 {
                game.save_now();
            }

            game.hud_timer -= dt;
            if game.hud_timer <= 0.0 {
                game.hud_timer = 0.2;
                game.push_hud();
            }
        } else {
            bridge::hud("1|草方块|等待|06:00|0|--|读取存档|正在读本地 IndexedDB");
        }

        // 4) 网格
        if matches!(game.phase, Phase::Running) {
            game.rebuild_dirty_chunks();
        }

        // 5) 渲染
        let sky = sky_color(game.time_of_day);
        clear_background(sky);

        let eye = game.player.eye();
        let look = game.player.look_dir();
        let eye_v = vec3(eye[0], eye[1], eye[2]);
        let target = eye_v + vec3(look[0], look[1], look[2]);
        set_camera(&Camera3D {
            position: eye_v,
            target,
            up: vec3(0.0, 1.0, 0.0),
            fovy: 70f32.to_radians(),
            aspect: None,
            projection: Projection::Perspective,
            render_target: None,
            viewport: None,
            z_near: 0.08,
            z_far: 260.0,
        });

        if matches!(game.phase, Phase::Running) {
            material.set_uniform("u_day", daylight(game.time_of_day));
            material.set_uniform("u_eye", eye_v);
            material.set_uniform(
                "u_fog_color",
                vec3(sky.r, sky.g, sky.b),
            );
            material.set_uniform("u_fog_near", FOG_NEAR);
            material.set_uniform("u_fog_far", FOG_FAR);

            gl_use_material(&material);
            for chunk in game.chunks.iter().flatten() {
                draw_mesh(chunk);
            }
            gl_use_default_material();

            // 瞄准框：用 macroquad 现成的线框立方体，省一段着色器
            if let Some((cell, _)) = game.pick_target() {
                draw_cube_wires(
                    vec3(
                        cell[0] as f32 + 0.5,
                        cell[1] as f32 + 0.5,
                        cell[2] as f32 + 0.5,
                    ),
                    vec3(1.01, 1.01, 1.01),
                    Color::new(0.06, 0.06, 0.06, 1.0),
                );
            }
        }

        next_frame().await
    }
}
