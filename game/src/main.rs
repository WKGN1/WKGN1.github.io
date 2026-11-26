//! 方块彩蛋的主程序：输入 → 物理 → 网格 → 渲染 → 存档。
//!
//! 渲染走 macroquad 的自定义材质（不是内置 PBR）：顶点里塞了「朝向明暗 × AO」和
//! 「自发光」两个数，昼夜和雾在片元里算。这样 shading 全在网格生成时算完，
//! 每帧的 GPU 活儿只剩一次采样 + 一次混合，手机才顶得住。
//!
//! 和 JS 的通信刻意做成最蠢的样子：Rust 通过 wasm 导入调 11 个 JS 函数，
//! 参数只有整数（指针、长度、偏移）。好处是不用 wasm-bindgen，构建就一条 cargo 命令。

mod blocks;
mod inv;
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

/// 本地存档读不出来时写给玩家看的那句话（HUD 第 8 段）。
/// `&'static str` 是因为它要从 Game 的字段里借出去拼 HUD。
const BAD_SAVE_NOTE: &str = "本地存档读不出来，已按种子重建";

/// 备注在 HUD 上挂多久。够看清一遍，又不会一直占着第 8 段挡掉飞行提示。
const NOTE_SECONDS: f32 = 12.0;

/// 等本地存档最多等这么久。JS 那边的 promise 不落地（隐私模式拒了 IndexedDB 之类）
/// 就一直白屏的话，读者连「这彩蛋在干嘛」都看不到。
const SAVE_WAIT_SECONDS: f32 = 4.0;

/// 一整个昼夜循环的秒数。太长读者感受不到，太短会晕。
const DAY_SECONDS: f32 = 360.0;

const REACH: f32 = 6.5;
const FOG_NEAR: f32 = 26.0;

/// 视距。地图只有 96 宽，远处的山会被雾吃掉，所以这个数字主要影响雾的观感。
const FOG_FAR: f32 = 96.0;

/// 每帧最多重建几个区块网格。启动时 36 个要建，分摊一下免得卡成一帧。
/// 多久没继续点同一格，挖掘进度就作废（秒）。不设这条，挖石头挖了一半走开，
/// 回来随手一下那一格就碎了。
/// 多久没继续点同一格，挖掘进度就作废（秒）。1.2 太紧：手机上按节奏点两下、
/// 中间随便停一下就被清了，等于硬方块永远挖不动。判据那边一次点击之间也要一秒多。
const MINE_RESET_SECONDS: f32 = 2.0;
const MESH_BUDGET: usize = 12;

// ---------------------------------------------------------------------------
// 和 JS 的桥（wasm 导入）
// ---------------------------------------------------------------------------

// 必须写明导入模块是 "env"：不写的话 lld 把这些函数当成本模块内的未定义符号，
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
    /// 存档一共有多少字节（0 = 没有存档）。先问长度再分配，才不会被「截断」骗到。
    fn mcl_save_len() -> u32;
    /// 把存档字节写进 wasm 内存：从 offset 起最多拷 cap 字节，返回实际拷贝数（0 = 到底了/没有）。
    /// 必须能分块轮询：只给 (ptr, cap) 时 JS 装不下就截到 cap 并返回 cap，Rust 以为读全了，
    /// decode 报 WrongSize 把整份存档丢掉，HUD 还显示「已存档」。
    fn mcl_pull_save(ptr: *mut u8, cap: u32, offset: u32) -> u32;
    /// 手机端 DOM 按钮：这一帧被点过就返回非 0，取走即清（Rust 每帧各调一次）。
    /// jump/fly 是「按了一下」，slot 是有符号增量（+1 下一个 / -1 上一个）。
    /// 桌面「移动鼠标就转向」：页面累积好的鼠标位移（像素），取走即清。
    fn mcl_take_look_x() -> f32;
    fn mcl_take_look_y() -> f32;
    /// 1 = 这一帧视角由页面推（自由转向）：Rust 别再按拖拽转一遍，
    /// 也别再拿"拖了多远"判点击 —— 自由转向下鼠标本来就一直在飘。
    fn mcl_free_look() -> i32;
    fn mcl_take_jump() -> i32;
    fn mcl_take_fly() -> i32;
    fn mcl_take_slot() -> i32;
    /// 手机摇杆几何（CSS 像素）：轴 0=圆心x 1=圆心y 2=半径。页面没装摇杆时半径给 0，
    /// Rust 就退回"左半屏走路"的老规矩（桌面本来也不走触摸这条路）。
    /// 1 = 手机端 HUD 上那个「潜行」按钮**正被按着**（按住型：按着就一直给 1，松手变 0）。
    /// 跟 jump 那种「取走即清」不一样：潜行是个状态，不是一下。
    fn mcl_sneak_held() -> i32;
    /// 「物品栏」面板发来的命令：取走一条，队列空了返回 0。
    fn mcl_take_ui_cmd() -> i32;
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

// 摇杆几何这个桥函数只在 wasm 目标上声明（放进上面那个 env 块会让
// `cargo test` 也去链接它，非 wasm 目标上压根没有这个导入，直接编译失败）。
// 这儿用 // 不用 ///：extern 块上挂文档注释会警告（文档挂不到东西上）。
// 必须带 #[link(wasm_import_module = "env")]：不带的话 rust-lld 找不到这个导入
// （它在宿主的 "env" 模块里），报 undefined symbol 直接链接失败。
#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "env")]
extern "C" {
    fn mcl_stick_geo(axis: u32) -> f32;
    /// 音效事件：id 见 bridge::sound，mat 是 blocks::sound_class 的材质类。
    /// 事件发生的那一帧直接喊，不走 0.2 秒一次的 HUD 通道（脚步慢了 0.2 秒就出戏了）。
    fn mcl_sound(id: u32, mat: u32);
}

mod bridge {
    use super::*;


    /// 摇杆几何：圆心 + 半径（画布坐标）。页面没给的时候半径 0 = 老规矩。
    pub fn stick_geo() -> (f32, f32, f32) {
        #[cfg(target_arch = "wasm32")]
        unsafe {
            (mcl_stick_geo(0), mcl_stick_geo(1), mcl_stick_geo(2))
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            (0.0, 0.0, 0.0)
        }
    }

    /// 一个音效事件，配方全在 loader.js 的 Sfx 里（WebAudio 现场合成，没有一个音频文件）。
    /// id：1 碎块 2 放块 3 脚步 4 挖掘一下 5 落水 6 拾取 7 落地；mat = b::sound_class(方块)。
    #[inline]
    pub fn sound(id: u32, mat: u32) {
        #[cfg(target_arch = "wasm32")]
        unsafe {
            mcl_sound(id, mat);
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = (id, mat); // 单测不需要听见声音
        }
    }

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

    /// 手机端「潜行」按钮是否正被按着（桌面版永远 false，那边看键盘）。
    pub fn sneak_held() -> bool {
        #[cfg(target_arch = "wasm32")]
        {
            unsafe { mcl_sneak_held() != 0 }
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            false
        }
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

    /// 桌面自由转向：页面攒好的鼠标位移（像素），取走即清。
    pub fn take_look_x() -> f32 {
        #[cfg(target_arch = "wasm32")]
        {
            unsafe { mcl_take_look_x() }
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            0.0
        }
    }

    pub fn take_look_y() -> f32 {
        #[cfg(target_arch = "wasm32")]
        {
            unsafe { mcl_take_look_y() }
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            0.0
        }
    }

    /// 页面说「这一帧视角归我推」。桌面调试版永远是自己算，所以 false。
    pub fn free_look() -> bool {
        #[cfg(target_arch = "wasm32")]
        {
            unsafe { mcl_free_look() != 0 }
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            false
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

    /// 拉存档字节。
    /// `Ok(None)` = JS 那边确实没有存档；`Err(话)` = 有存档但拿不出来（太大 / 只给了一半），
    /// 调用方要按种子重建并且**标记待存**，否则坏档永远躺在 IndexedDB 里反复崩。
    pub fn pull_save() -> Result<Option<Vec<u8>>, &'static str> {
        #[cfg(target_arch = "wasm32")]
        {
            // 落地区仍是一块 256 KB：wasm 内存只能长不能缩，所以窗口反复用同一块，
            // 靠 offset 一段一段把整份存档搬进来（存档几十 KB，一般一轮就够）。
            const CAP: usize = 256 * 1024;
            // 超过这个长度不当存档处理：真有这么大的档，按尺寸算就已经不是我们的格式了。
            const MAX_SAVE: usize = 8 * 1024 * 1024;

            let total = unsafe { mcl_save_len() } as usize;
            if total == 0 {
                return Ok(None);
            }
            if total > MAX_SAVE {
                return Err("本地存档太大，已按种子重建");
            }

            let mut buf: Vec<u8> = vec![0u8; total];
            let mut off: u32 = 0;
            while (off as usize) < total {
                let want = CAP.min(total - off as usize) as u32;
                let n =
                    unsafe { mcl_pull_save(buf.as_mut_ptr().add(off as usize), want, off) } as usize;
                if n == 0 {
                    // JS 说没了，可我还没拿满：半截存档，宁可重来也别照着它建世界
                    return Err("本地存档读不全，已按种子重建");
                }
                if n > want as usize {
                    return Err("本地存档读不全，已按种子重建");
                }
                off += n as u32;
            }
            Ok(Some(buf))
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            // 桌面调试用：文件在就是存档，不在就没有。
            match std::fs::read("save.mcl") {
                Ok(bytes) => Ok(Some(bytes)),
                Err(_) => Ok(None),
            }
        }
    }

    /// 手机端按钮：这一帧有没有按过跳 / 切飞行 / 换方块。桌面版一律 0（键盘自己走）。
    #[inline]
    pub fn take_jump() -> i32 {
        #[cfg(target_arch = "wasm32")]
        {
            unsafe { mcl_take_jump() }
        }
        #[cfg(not(target_arch = "wasm32"))]
        0
    }

    #[inline]
    pub fn take_fly() -> i32 {
        #[cfg(target_arch = "wasm32")]
        {
            unsafe { mcl_take_fly() }
        }
        #[cfg(not(target_arch = "wasm32"))]
        0
    }

    #[inline]
    pub fn take_slot() -> i32 {
        #[cfg(target_arch = "wasm32")]
        {
            unsafe { mcl_take_slot() }
        }
        #[cfg(not(target_arch = "wasm32"))]
        0
    }

    /// 「物品栏」面板发来的命令：取走一条，队列空了返回 0（非 wasm 永远是 0）。
    #[inline]
    pub fn take_ui_cmd() -> i32 {
        #[cfg(target_arch = "wasm32")]
        {
            unsafe { mcl_take_ui_cmd() }
        }
        #[cfg(not(target_arch = "wasm32"))]
        0
    }
}

// ---------------------------------------------------------------------------
// 材质与着色器
// ---------------------------------------------------------------------------

const VERT: &str = r#"#version 100
attribute vec3 position;
attribute vec2 texcoord;
attribute vec4 color0;

// uv 不能是 lowp：lowp 只有 8 位有效位，在 224 像素宽的图集上误差快够到 1 像素，
// 采过去就是「一根棍子画成饼干」那类串贴片（OpusMC 的注释里为半精度栽过同一个跟头）。
// shade / emissive 是 0..1 的乘数，留 lowp 没有意见。
varying mediump vec2 uv;
varying lowp float shade;
varying lowp float emissive;
varying mediump vec3 wpos;
varying lowp float skyl;
varying lowp float blk;

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
    // .b/.a 是天光、方块光等级（0..15 摊到 0..255）。颜色不在顶点里，去那张色表取。
    skyl = color0.b / 255.0;
    blk = color0.a / 255.0;
    wpos = world.xyz;
}
"#;

/// 亮度三档对应的曲线混合量（`u_bright`）。0 = 今天原来的样子。
const BRIGHT: [f32; 3] = [0.0, 0.45, 0.78];

const FRAG: &str = r#"#version 100
precision mediump float;

varying mediump vec2 uv;
varying lowp float shade;
varying lowp float emissive;
varying mediump vec3 wpos;
varying lowp float skyl;
varying lowp float blk;

uniform sampler2D Texture;
// 16×16 的光照色表：横轴天光等级、纵轴方块光等级，每格是这两种光混一起该是什么颜色。
// 昼夜只换这张 1 KB 的表，不动顶点、也不动网格。
uniform sampler2D u_light;
uniform vec3 u_eye;
uniform vec3 u_fog_color;
uniform float u_fog_near;
uniform float u_fog_far;
uniform float u_bright;     // 0 = 保持现在这样，越大越亮（夜里手机上真的有用）

void main() {
    vec4 tex = texture2D(Texture, uv);
    // 等级 0..15 要落在 texel 中心上：中心是 (等级 + 0.5) / 16，所以偏 1/32。
    // 不偏这半格，Nearest 采样会让 0 级采到 1 级的颜色、15 级采到表外面去。
    vec2 lv = vec2(skyl * 0.9375 + 0.03125, blk * 0.9375 + 0.03125);
    vec3 col = tex.rgb * shade * texture2D(u_light, lv).rgb;
    // 自发光方块不吃昼夜和 AO，夜里能看到自己亮的灯。
    col = mix(col, tex.rgb * 1.12, emissive);

    // 亮度：把中间调抬起来、两端不动的一条曲线（OpusMC 用的也是这类：它在一条保守曲线和
    // 一条 1-(1-x)^3 之间插值）。这边直接用「和原样的自己」插，u_bright=0 就是今天这个样子，
    // 不会因为加了这条曲线先把画面整体压暗一截。高光（自发光那种 >1 的）留在曲线外面不被掐掉。
    if (u_bright > 0.0) {
        vec3 lift = 1.0 - pow(max(vec3(0.0), 1.0 - min(col, vec3(1.0))), vec3(3.0));
        col = mix(col, lift, u_bright);
    }

    // 距离取「水平距离」和「竖直距离的一半」里大的那个（OpusMC 那条）：低头看脚底不该起雾 ——
    // 竖直方向本来就只差几个方块，按三维直线距离算会把脚下也一起糊进雾里。
    vec3 rel = wpos - u_eye;
    float d = max(length(rel.xz), abs(rel.y) * 0.5);
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
            macroquad::miniquad::UniformDesc::new(
                "u_bright",
                macroquad::miniquad::UniformType::Float1,
            ),
        ],
        // "Texture" 是 macroquad 的保留槽名：着色器里照写 uniform sampler2D Texture，
        // 但别列进这里——列了 make_pipeline 直接 panic（quad_gl.rs 里那句
        // "you can't use name `Texture` ..."）。绘制时传进来的贴图会自动占住这个槽。
        // 采样器名字要写在 textures 里，而且是**字符串名字**（miniquad 0.4.11 的
        // UniformType 里根本没有 Texture 这一项，textures 的类型是 Vec<String>）。
        // 别写错名字：这里漏一个，着色器里那个 sampler 就永远采不到东西。
        textures: vec!["u_light".to_string()],
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
    // 颜色的档位跟着钟面对齐过：黄昏橙只占 17:00-21:00 那一小段，之后到日出前都是深夜蓝。
    // （上一版夜里那档发棕，是因为橙色到蓝色的过渡带拉得太长，钟面 20:12 还在橙色那头 —— 地面
    // 已经按光照算黑了，天空还一片橙，看着就像着火了而不是晚上了。）
    const STOPS: [(f32, [f32; 3]); 6] = [
        (0.00, [0.53, 0.72, 0.92]),   // 清晨偏暖的蓝
        (0.25, [0.62, 0.78, 0.95]),   // 正午
        (0.42, [0.93, 0.62, 0.35]),   // 黄昏橙（钟面 17:07）
        (0.56, [0.035, 0.055, 0.13]), // 入夜：深夜蓝（钟面 20:22）
        (0.84, [0.035, 0.055, 0.13]), // 一整夜都是这一档，日出前才回暖
        (1.00, [0.53, 0.72, 0.92]),   // 回到清晨
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
/// 16×16 的光照色表（1 KB）：横轴 = 天光等级 0..15，纵轴 = 方块光等级 0..15。
/// 顶点上只存等级、不存颜色，所以昼夜变化只要重传这 1 KB，不用碰任何网格或顶点。
/// 颜色是我自己调的：白天天光接近白（跟今天画面对得上，不然一上线就先换个样给人看），
/// 夜里降成很暗的蓝灰；方块光是暖橙（萤石灯的调子）。两路光用 1-(1-a)(1-b) 合，
/// 不是直接相加 —— 相加会让「白天露天、旁边还点着灯」那种地方糊成一片死白。
/// 最暗处也不给纯黑，留一点点蓝：全黑在手机上就是「啥也看不见」，容易被当成游戏卡住了。
fn lightmap_rgba(t: f32) -> [u8; 16 * 16 * 4] {
    let d = daylight(t);
    // daylight 是 1.0（白天）..0.5（夜里），拉回 0..1 当"白天程度"
    let k = ((d - 0.5) * 2.0).clamp(0.0, 1.0);
    let mix = |a: f32, b: f32| a + (b - a) * k;
    // 天光颜色：夜里很暗的蓝灰 → 白天接近白（略偏冷）
    let sky_t = [mix(0.30, 1.00), mix(0.38, 1.01), mix(0.62, 1.06)];
    // 方块光颜色：暖橙，红通道给满、蓝通道压住，看着才像火光不像日光
    let blk_t = [1.00f32, 0.74, 0.44];
    // 全黑时的底光：一点点蓝，别让洞穴和夜里成纯黑一块
    let floor = [0.055f32, 0.062, 0.078];
    let mut out = [0u8; 16 * 16 * 4];
    for by in 0..16usize {
        for sx in 0..16usize {
            let s = sx as f32 / 15.0;
            let b = by as f32 / 15.0;
            let i = (by * 16 + sx) * 4;
            for ch in 0..3 {
                let sc = sky_t[ch] * s;
                let bc = blk_t[ch] * b;
                // 两条光合成（不是相加）：任一条满了，另一条再加也几乎看不见了
                let c = 1.0 - (1.0 - sc) * (1.0 - bc);
                let c = c.max(floor[ch]);
                out[i + ch] = (c.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            }
            out[i + 3] = 255;
        }
    }
    out
}

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

/// HUD 上那个钟。本来 t=0 就当 00:00，但明暗那条曲线不是这么定的：满亮在 t<0.38、
/// 天从 t=0.38 起转暗、0.52..0.86 是夜、0.86 之后才回亮 —— 也就是"正午"落在 t≈0.2、
/// "半夜"落在 t≈0.7，整套比钟面晚 7 小时。以前夜并不真黑，这个差没人看得出；
/// 现在夜是黑的，钟写「13:30」而外面一片黑就是明摆着的自相矛盾，所以给钟加 7 小时。
fn clock_text(t: f32) -> String {
    let t = t.rem_euclid(1.0);
    let total = (t * 24.0 * 60.0) as i32 + 7 * 60;
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
    /// 背包 + 快捷栏 + 手里第几格。创造模式不计数，格子只记住"手里是哪种方块"。
    inv: inv::Inv,
    /// true = 创造（随便挖随便放，物品栏里显示 ∞）；false = 生存（挖了进背包、放下扣一个、
    /// 工作台放地上、站旁边才解锁高阶配方）。
    creative: bool,
    /// 「物品栏」面板开着：不许挖、不许放、不许走。
    paused: bool,
    /// 亮度档位 0/1/2（页面里那个「亮度」按钮切的，存在浏览器 localStorage，不在存档里：
    /// 这是显示偏好不是游戏状态，换设备该跟着浏览器走）。
    bright: u8,
    /// 当前水帧 + 计时器。每 0.5 秒换一帧，只覆写图集里水那一格 16×16。
    /// 16×16 的光照色表 + 它是按哪个"白天程度"建的（变了才重传那 1 KB）。
    lightmap: Texture2D,
    lightmap_bucket: i32,
    water_frame: usize,
    /// 生存挖掘进度：(正盯着哪一格, 还差几下点碎)。None = 没有进行中的挖掘。
    mine: Option<([i32; 3], u8)>,
    /// 那一格一共要几下。进度条要用它，存着是为了别让同一格的总下数在画面上跳来跳去。
    mine_total: u8,
    /// 距离「这进度该作废了」还剩几秒。
    mine_clock: f32,
    water_clock: f32,
    /// 音效的帧间记忆：上一帧在水里吗 / 站在地上吗（进出水、落地靠状态翻转判）。
    snd_in_water: bool,
    snd_on_ground: bool,
    /// 这趟腾空里最低的下落速度（负值）。落地响不响、多响看它。
    snd_air_vel: f32,
    /// 脚步按**距离**不按时间：攒够 1.3 块走一步，疾跑自然变密、潜行变疏。
    snd_step_dist: f32,
    time_of_day: f32,
    phase: Phase,
    chunks: Vec<Option<Mesh>>,
    hud_timer: f32,
    save_timer: f32,
    save_dirty: bool,
    last_fps: f32,
    touch_move: Option<(u64, Vec2, Vec2)>,
    look: Vec<LookTouch>,
    drag: (Vec2, f32, bool),
    wheel_cd: f32,
    /// 已经跑过的游戏秒数（只在 Running 里累加）。双击判定拿它当钟：
    /// 真实时钟会被切后台拉长，用被钳过的 dt 累加才不会把「上次抬手」记成几分钟前。
    uptime: f32,
    /// 上一次「没怎么动的轻点」的 (抬手时刻, 抬手位置)。连着两下 = 双击 → 切飞行。
    double_tap: Option<(f32, Vec2)>,
    /// HUD 第 8 段的备注（坏存档、等太久这类必须让玩家看见的话）。空 = 不显示。
    note: &'static str,
    /// 备注自动消失的剩余秒数。备注只占第 8 段，飞行提示一响就该让位，所以别长挂。
    note_timer: f32,
    /// 程序化图集，建一次，交给每个区块网格带着。
    atlas: Option<Texture2D>,
}

impl Game {
    fn new(world: World) -> Self {
        let spawn = world.spawn();
        Game {
            world,
            player: Player::new(spawn),
            inv: inv::Inv::new_creative(),
            creative: true,
            paused: false,
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
            uptime: 0.0,
            double_tap: None,
            note: "",
            note_timer: 0.0,
            atlas: None,
            bright: 0,
            lightmap: Texture2D::from_rgba8(16, 16, &lightmap_rgba(0.0)),
            lightmap_bucket: -1,
            water_frame: 0,
            mine: None,
            mine_total: 0,
            mine_clock: 0.0,
            water_clock: 0.0,
            snd_in_water: false,
            snd_on_ground: true,
            snd_air_vel: 0.0,
            snd_step_dist: 0.0,
        }
    }

    /// 在 HUD 第 8 段挂一句话，secs 秒后自己消失。
    fn set_note(&mut self, text: &'static str, secs: f32) {
        self.note = text;
        self.note_timer = secs;
    }

    /// 消费世界那份脏表：一次取走、就地清零。
    /// 脏标记只有 World 一份（挖/放时 set() 标脏），以前 Game 里另存一份没人读的，
    /// 于是挖掉一格只改了数据和那份死表，网格永远不重建。
    fn rebuild_dirty_chunks(&mut self) {
        // 取走而不是边读边清：build_chunk 只借 &self.world，而超预算的格子回头还要标回去。
        let todo = std::mem::replace(&mut self.world.dirty, vec![false; world::CHUNK_COUNT]);
        let mut built = 0usize;
        for i in 0..world::CHUNK_COUNT {
            if !todo[i] {
                continue;
            }
            if built >= MESH_BUDGET {
                // 这一帧建不完：剩下的标回世界上，下一帧接着建。漏标就等于永远缺一块网格。
                self.world.dirty[i] = true;
                continue;
            }
            let cx = i as i32 % world::CHUNKS_X;
            let cz = i as i32 / world::CHUNKS_X;

            let mut m = ChunkMesh::default();
            mesh::build_chunk(&self.world, cx, cz, &mut m);

            let mut verts: Vec<Vertex> = Vec::with_capacity(m.vert_count());
            for v in m.verts.chunks_exact(mesh::STRIDE_F32) {
                verts.push(Vertex {
                    position: vec3(v[0], v[1], v[2]),
                    uv: vec2(v[3], v[4]),
                    // .r = 明暗，.g = 自发光标记，.b/.a = 天光、方块光等级（0..15 → 0..255）。
                    // 这两槽原来是写死的 0 和 255，谁都没用，正好拿来带光照：
                    // 顶点布局宽度、属性个数、索引全没变，只是把空着的字节填上。
                    color: [
                        (v[5] * 255.0) as u8,
                        if v[6] > 0.5 { 255 } else { 0 },
                        ((v[7] * 17.0) as u8).min(255),
                        ((v[8] * 17.0) as u8).min(255),
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
        }
    }

    fn mark_all_dirty(&mut self) {
        for d in self.world.dirty.iter_mut() {
            *d = true;
        }
    }

    /// 挖掉瞄准到的方块。**生存**模式下按硬度算「还要点几下」：石头三下、草一下。
    /// 手势一条没改 —— 电脑单击、手机轻点都还是「挖一下」，硬的东西多点几下，按住仍然是放；
    /// 硬度只体现在要点几下上，不体现在按住不放上，所以手机上不会出现「轻点永远挖不动」。
    /// **创造**一下就碎（真 MC 的创造也是这样，不然创造模式就没意义了）。
    fn mine(&mut self) {
        let eye = self.player.eye();
        let dir = self.player.look_dir();
        let Some(hit) = ray::cast(&self.world, eye, dir, REACH) else {
            self.mine = None;
            return;
        };
        let [x, y, z] = hit.cell;
        if y <= 0 {
            return; // 第 0 层是世界底壳，挖穿就掉出去了
        }
        let id = self.world.get(x, y, z);
        if b::is_air(id) {
            self.mine = None;
            return;
        }
        let total = b::taps_to_break(id);
        if self.creative {
            self.break_cell(x, y, z, id);
            return;
        }
        // 同一格、而且那一格还是原来那种方块，才接着数；换了目标或被人换了方块就从头数
        let left = match self.mine {
            Some((cell, n)) if cell == [x, y, z] && self.world.get(x, y, z) == id => n,
            _ => total,
        };
        if left <= 1 {
            self.break_cell(x, y, z, id);
            return;
        }
        self.mine = Some(([x, y, z], left - 1));
        self.mine_total = total;
        self.mine_clock = MINE_RESET_SECONDS;
        bridge::sound(4, b::sound_class(id)); // 每一下都响：手机点按挖掘全靠这个反馈
    }

    /// 真把这一格清掉：清成空气、标脏（在 world.set 里）、生存掉料、进度抹平。
    fn break_cell(&mut self, x: i32, y: i32, z: i32, id: u8) {
        self.mine = None;
        self.mine_total = 0;
        self.world.set(x, y, z, b::AIR);
        // 挖开一格会改变光照（洞里的光要从洞口往里摊）。整张图才 44 万格、局部那一坨
        // （半径 15 = 光的射程）release 下实测 2 毫秒，不卡手。
        self.world.relight_around(x, y, z);
        self.save_dirty = true;
        bridge::sound(1, b::sound_class(id)); // 碎块：材质决定是脆响还是闷响
        // 生存才掉料。创造挖了就是挖了，背包里那些格子永远是 0 个。
        if !self.creative {
            // add 返回没装下的余数：0 = 接住了（响一声），>0 = 背包满了
            let dropped = self.inv.add(id, 1);
            if dropped == 0 {
                bridge::sound(6, 0); // 拾取：短促上挑的「啵」
            }
            if dropped > 0 {
                self.set_note("背包满了，掉出来的没接住", NOTE_SECONDS);
            }
        }
    }

    /// 在瞄准到的面上放一个当前选中的方块。
    fn place(&mut self) {
        if !self.inv.can_place(self.creative) {
            if !self.creative {
                self.set_note("手里这种方块用完了，去物品栏换一格", NOTE_SECONDS);
            }
            return;
        }
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
        let Some(id) = self.inv.consume_selected(self.creative) else {
            return; // can_place 上头已经说过了，这儿只是兜底
        };
        self.world.set(cell[0], cell[1], cell[2], id);
        // 放下方块同样要重算光照（它挡光，也可能自己发光）
        self.world.relight_around(cell[0], cell[1], cell[2]);
        self.save_dirty = true;
        bridge::sound(2, b::sound_class(id)); // 放块：比碎块闷一点
    }

    /// 每帧一次的音效事件：拿物理前后的状态翻转判入水/落地，脚步按距离攒。
    /// 必须在 player.update 之后调（用的都是它刚写进去的新状态）。
    fn play_frame_sounds(&mut self, dt: f32) {
        let p = &self.player;
        if p.in_water && !self.snd_in_water {
            bridge::sound(5, 4); // 入水：泼
        }
        self.snd_in_water = p.in_water;

        if p.on_ground {
            if !self.snd_on_ground && self.snd_air_vel < -11.0 {
                // 落地：下落够快（约 2.5 块以上）才响；轻轻落地不该有声音
                bridge::sound(7, 0);
            }
            let h_speed = (p.vel[0] * p.vel[0] + p.vel[2] * p.vel[2]).sqrt();
            if !p.fly && !p.in_water && !p.sneak && h_speed > 1.2 {
                self.snd_step_dist += h_speed * dt;
                if self.snd_step_dist >= 1.3 {
                    self.snd_step_dist -= 1.3;
                    // 脚步音色由**脚下那块**决定
                    let gx = p.pos[0].floor() as i32;
                    let gy = (p.pos[1] - 0.05).floor() as i32;
                    let gz = p.pos[2].floor() as i32;
                    let id = if world::in_bounds(gx, gy, gz) {
                        self.world.get(gx, gy, gz)
                    } else {
                        b::GRASS
                    };
                    bridge::sound(3, b::sound_class(id));
                }
            } else {
                self.snd_step_dist = 0.0;
            }
            self.snd_air_vel = 0.0;
        } else {
            self.snd_step_dist = 0.0;
            if p.vel[1] < self.snd_air_vel {
                self.snd_air_vel = p.vel[1];
            }
        }
        self.snd_on_ground = p.on_ground;
    }

    fn pick_target(&self) -> Option<([i32; 3], [i32; 3])> {
        let eye = self.player.eye();
        let dir = self.player.look_dir();
        let hit = ray::cast(&self.world, eye, dir, REACH)?;
        Some((hit.cell, hit.normal))
    }

    /// 玩家附近（水平 4 格、上下 2 格）有没有工作台：站旁边才解锁高阶配方。
    fn bench_near(&self) -> bool {
        let p = self.player.pos;
        let px = p[0].floor() as i32;
        let py = p[1].floor() as i32;
        let pz = p[2].floor() as i32;
        for dx in -4..=4 {
            for dz in -4..=4 {
                for dy in -2..=2 {
                    let (x, y, z) = (px + dx, py + dy, pz + dz);
                    if world::in_bounds(x, y, z) && self.world.get(x, y, z) == b::WORKBENCH {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// 存档里带的背包段：模式(1) + 选中格(1) + 每格 (方块, 数量小端) = 定长 110 字节。
    fn inv_bytes(&self) -> Vec<u8> {
        let mut v = Vec::with_capacity(2 + inv::SLOTS * 3);
        v.push(if self.creative { 1 } else { 0 });
        v.push(self.inv.selected_index() as u8);
        for s in self.inv.slots.iter() {
            v.push(s.id);
            v.extend_from_slice(&s.n.to_le_bytes());
        }
        v
    }

    /// 读回来的背包段。长度不对、方块 id 不认识、数量超标，一律当没有（保住开局那套），
    /// 宁可丢背包也别把一个能读的存档搞成读不了。
    fn load_inv(&mut self, bytes: &[u8]) -> bool {
        const LEN: usize = 2 + inv::SLOTS * 3;
        if bytes.len() != LEN || bytes[1] as usize >= inv::HOTBAR_SLOTS {
            return false;
        }
        let creative = bytes[0] == 1;
        let mut slots = [inv::EMPTY; inv::SLOTS];
        for i in 0..inv::SLOTS {
            let o = 2 + i * 3;
            let id = bytes[o];
            let n = u16::from_le_bytes([bytes[o + 1], bytes[o + 2]]);
            if id > b::WORKBENCH || n > inv::STACK_MAX || (id == b::AIR && n != 0) {
                return false;
            }
            if id != b::AIR && n == 0 && !creative {
                return false; // 生存里 0 个的格子该清成空气，留着是脏数据
            }
            slots[i] = inv::Slot { id, n };
        }
        self.inv = inv::Inv {
            slots,
            selected: bytes[1] as usize,
        };
        self.creative = creative;
        true
    }

    /// 切创造 / 生存。切过去就把背包换成那个模式该有的一套：创造的数量永远是 0，
    /// 混着带进生存就成了「手里有 0 个的方块」，看着跟 bug 一样。
    /// 飞行开关。**生存模式飞不了**（跟真 MC 一样：飞行是创造的能力，不是通用技巧）。
    /// 三个入口都走这儿：电脑上的 F、HUD 上那个「飞」按钮、手机端的双击。
    /// 双击那条不走这儿 —— 它在生存里要退化成「正常再挖一下」，见 read_touch。
    fn toggle_fly(&mut self) {
        if !self.creative {
            self.set_note("生存模式飞不了，按 G 或开物品栏切到创造", NOTE_SECONDS);
            return;
        }
        self.player.fly = !self.player.fly;
        self.player.vel[1] = 0.0; // 切换瞬间别带着旧竖直速度乱窜
    }

    fn toggle_creative(&mut self) {
        let was_flying = self.player.fly;
        self.creative = !self.creative;
        if !self.creative && was_flying {
            // 切到生存还悬在半空会很怪：飞行是创造的能力，切过去就该掉下来。
            self.player.fly = false;
            self.player.vel[1] = 0.0;
        }
        self.inv = if self.creative {
            inv::Inv::new_creative()
        } else {
            inv::Inv::new_survival()
        };
        self.save_dirty = true;
        self.set_note(
            if self.creative {
                "切到创造：随便挖随便放"
            } else if was_flying {
                "切到生存：挖了进背包，飞行也停了"
            } else {
                "切到生存：挖了进背包，放下要扣"
            },
            NOTE_SECONDS,
        );
    }

    /// 「物品栏」面板发来的命令。命令号那张表在 loader.js 里的 uiCmd() 注释上。
    fn ui_cmd(&mut self, cmd: i32) {
        match cmd {
            1..=9 => {
                self.inv.selected = (cmd - 1) as usize;
            }
            100..=126 => {
                // 背包第 n 格 ↔ 手上那格（面板里点背包格）
                self.inv.swap_with_selected(inv::HOTBAR_SLOTS + (cmd - 100) as usize);
                self.save_dirty = true;
            }
            200..=299 => {
                let bench = self.bench_near();
                if let Some(r) = inv::RECIPES.get((cmd - 200) as usize) {
                    if inv::craft(&mut self.inv, r, bench) {
                        self.set_note(r.note, NOTE_SECONDS);
                        self.save_dirty = true;
                    } else {
                        self.set_note("合不成：料不够，或者没站在工作台旁边", NOTE_SECONDS);
                    }
                }
            }
            300 => self.toggle_creative(),
            410..=412 => {
                // 亮度档位。数值表在页面那边（BRIGHT[u_bright] 那档曲线），这边只收档位。
                self.bright = (cmd - 410) as u8;
            }
            400 => self.paused = true,
            401 => self.paused = false,
            6000..=7999 => {
                // 自动化/调试用的给料：6000 + 方块 id * 100 + 数量。创造不计数量，忽略。
                let raw = cmd - 6000;
                let id = (raw / 100) as u8;
                let n = (raw % 100) as u16;
                if !self.creative && id <= b::WORKBENCH && n > 0 && self.inv.add(id, n) > 0 {
                    self.set_note("背包满了，掉出来的没接住", NOTE_SECONDS);
                }
            }
            _ => {}
        }
    }

    fn save_now(&mut self) {
        let bytes = save::encode(&self.world, &self.inv_bytes());
        bridge::save_request(&bytes);
        self.save_dirty = false;
        self.save_timer = 0.0;
    }

    fn push_hud(&mut self) {
        let mode = if self.player.fly {
            "飞行"
        } else if self.player.in_water {
            "游泳"
        } else if self.player.sneak {
            "潜行"
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
        // 第 8 段是「玩家此刻该知道的一句话」：飞行提示优先（它只在飞行那几秒有用），
        // 其次是在等存档时说明画面为什么还空着，其它时候留给备注（坏存档、读档超时）。
        let note = if self.player.fly {
            "飞行中，空格上升 Shift 下降"
        } else if matches!(self.phase, Phase::Waiting) {
            "正在读本地存档…"
        } else {
            self.note
        };
        // 第 9 段是位姿读数（位置 + 朝向），界面上只映射前 8 段所以不会显示，
        // 留给外部自动化测试用：有了 yaw/pitch 才能验「往右拖是不是真往右转」这种方向问题。
        // 眼睛那一格的光照读数：站在露天 sk 该是 15，钻进洞里要掉下来，点一盏灯 bl 要升到 15。
        // 光照这种事不靠我截图猜，判据直接读这两个数。
        let le = self.player.eye();
        // eye() 给的是 [f32;3]，不是 Vec3 —— 索引别写成字段
        let (lex, ley, lez) = (le[0].floor() as i32, le[1].floor() as i32, le[2].floor() as i32);
        let (lsx, lblk) = (
            self.world.sky_light(lex, ley, lez),
            self.world.block_light(lex, ley, lez),
        );
        // 尾巴上的自动化通道（和 yaw/pitch 一样是给判据读的，不是调试残留）：
        // g = 页面给 Rust 的圆盘（圆心,圆心,半径），判据照它算按下点、核对落点；
        // t = 摇杆正记着的手指 id（-1 = 没记着），判据靠它分辨"没认下"和"认下了推得不够"。
        let (g2s, g2sy, g2r) = bridge::stick_geo();
        let tm_dbg: i64 = self.touch_move.as_ref().map_or(-1, |m| m.0 as i64);
        // 再加一条：Rust 这帧**真的**看见了几根手指、各在哪个坐标、什么阶段。
        // 判据发的触摸是画布逻辑坐标，若 macroquad 交给我们的是另一套尺，这儿一比就露馅。
        let raw_dbg: String = touches()
            .iter()
            .take(3)
            .map(|t| {
                let ph = match t.phase {
                    TouchPhase::Started => 'b',
                    TouchPhase::Moved => 'm',
                    TouchPhase::Stationary => 's',
                    TouchPhase::Ended => 'e',
                    TouchPhase::Cancelled => 'c',
                };
                format!(
                    " {}:{}({:.0},{:.0}){}",
                    t.id,
                    "",
                    t.position.x,
                    t.position.y,
                    ph
                )
            })
            .collect::<String>();
        let scr_dbg = format!("{:.0}x{:.0}", screen_width(), screen_height());
        let pos = format!(
            // 尾巴上挂一组自动化通道（g=页面给的圆盘 t=摇杆记着的手指，[宽x高 触摸们]=原始转储）。
            "x{:.2} y{:.2} z{:.2} yaw{:.3} pitch{:.3} wf{} br{} sk{} bl{} g{:.0},{:.0},{:.0} t{} [{} {}]",
            self.player.pos[0],
            self.player.pos[1],
            self.player.pos[2],
            self.player.yaw,
            self.player.pitch,
            self.water_frame,
            self.bright, lsx, lblk, g2s, g2sy, g2r, tm_dbg,
            scr_dbg, raw_dbg
        );
        let sel = self.inv.selected_slot();
        let held = if self.creative {
            "∞".to_string()
        } else {
            format!("×{}", sel.n)
        };
        let mode = format!("{}·{}", mode, if self.creative { "创造" } else { "生存" });
        // 第 11 段：挖掘进度「哪种方块:点了几下:共需几下」。空串 = 没在挖（界面把准星下面
        // 那条小横杠收掉）。新段永远加在最后，前 10 段一个字都没动，老页面照旧能读。
        let mine_text = match self.mine {
            Some((cell, left)) => {
                let total = self.mine_total.max(1) as u32;
                let done = total.saturating_sub(left as u32);
                format!(
                    "{}:{}:{}",
                    self.world.get(cell[0], cell[1], cell[2]),
                    done,
                    total
                )
            }
            None => String::new(),
        };
        let text = format!(
            "{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
            self.inv.selected_index() + 1,
            format!("{} {}", b::block_name(sel.id), held),
            mode,
            clock_text(self.time_of_day),
            self.world.changed,
            fps,
            state,
            note,
            pos,
            // 第 10 段：物品栏面板要的整份数据（里面一个 | 都没有，管子按 | 切段）
            inv::inv_text(&self.inv, self.creative, self.bench_near()),
            mine_text
        );
        bridge::hud(&text);
    }
}

// ---------------------------------------------------------------------------
// 输入
// ---------------------------------------------------------------------------

/// 整屏转 180° 大约需要的鼠标像素（按住拖动和自由转向共用这一档）。
const MOUSE_SENS: f32 = std::f32::consts::PI / 700.0;

/// 桌面：页面上鼠标一移动就转向（free_look 时位移由 loader.js 攒着递进来）；
/// 页面不推的时候退回按住拖动转向。按下再松手算一次点击。
fn read_mouse(g: &mut Game, dt: f32) {
    // 触屏这一帧有手指就别走鼠标路径：胶水的 simulate_mouse_with_touch 默认把触摸同时点一份
    // 鼠标事件，main 开头已经关了；这里再兜一道，免得哪台设备没关掉时一次轻点被
    // read_touch 和这里各挖一次（一下变两下）、按住放完又挖。
    if !touches().is_empty() {
        return;
    }

    let (mx, my) = mouse_position();
    let cur = vec2(mx, my);

    let held = is_mouse_button_down(MouseButton::Left)
        || is_mouse_button_down(MouseButton::Right)
        || is_mouse_button_down(MouseButton::Middle);
    let free = bridge::free_look();

    // 自由转向：位移是页面攒好递过来的，符号（往右拖 = yaw 减）在 phys::turn_yaw_pitch 里
    if free {
        let (lx, ly) = (bridge::take_look_x(), bridge::take_look_y());
        if lx != 0.0 || ly != 0.0 {
            phys::turn_yaw_pitch(&mut g.player.yaw, &mut g.player.pitch, lx, ly, MOUSE_SENS);
        }
    }

    if held {
        if !g.drag.2 {
            // 刚按下：记下起点
            g.drag = (cur, 0.0, true);
        } else {
            let d = cur - g.drag.0;
            g.drag.1 += d.length();
            g.drag.0 = cur;
            // 页面在推视角了就别再按拖拽转一遍，否则一次移动算两回
            if !free && g.drag.1 > 6.0 {
                g.drag.2 = true;
                phys::turn_yaw_pitch(&mut g.player.yaw, &mut g.player.pitch, d.x, d.y, MOUSE_SENS);
            }
        }
    } else if g.drag.2 {
        // 松手：没怎么移动过的话当成一次点击。自由转向下鼠标一直在飘，"拖了多远"不再是依据 ——
        // 按下再松手就算一下（loader.js 那边按着鼠标时不推视角，手抖不会把视角带歪）。
        let tapped = free || g.drag.1 <= 6.0;
        g.drag = (cur, 0.0, false);
        if tapped {
            if is_mouse_button_released(MouseButton::Right) {
                g.place();
            } else if is_mouse_button_released(MouseButton::Middle) {
                if let Some((cell, _)) = g.pick_target() {
                    let id = g.world.get(cell[0], cell[1], cell[2]);
                    if let Some(i) = g.inv.find(id, g.creative) {
                        if i < inv::HOTBAR_SLOTS {
                            g.inv.selected = i;
                        } else {
                            g.inv.swap_with_selected(i);
                        }
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
        g.inv.cycle_selected(-(dy.signum() as i32));
        g.wheel_cd = 0.12;
    }
}

/// 摇杆命中判定（几何来自页面）。半径 <= 0 表示页面没装摇杆（桌面、或者胶水没更新）：
/// 退回老规矩 —— 左半屏算摇杆。测试只盖半径为正的那条路（无窗口时 screen_width 不可靠）。
fn stick_contains(scx: f32, scy: f32, sr: f32, p: Vec2) -> bool {
    if sr <= 0.0 {
        return p.x < screen_width() * 0.45;
    }
    let dx = p.x - scx;
    let dy = p.y - scy;
    dx * dx + dy * dy <= sr * sr
}

/// 右半屏的一根手指。held 是按住秒数，moved 是累计位移（像素）。
/// dragging：位移已经越过「还在拖」的门槛，这次按放置永久取消（见下面的 aged 循环）。
#[derive(Clone, Copy)]
struct LookTouch {
    id: u64,
    last: Vec2,
    held: f32,
    moved: f32,
    placed: bool,
    dragging: bool,
}

/// 触屏：左下那个画出来的摇杆走路，屏幕其余地方全都转向，轻点挖、按住放。
/// 每个角色只认一根手指：多指同时按着时第二根只是重复记账，会把视角转成两倍快。
/// 摇杆的归属按"这根手指一开始是不是按在摇杆上"算（跟着手指走，手指划出圈也不换角色），
/// 不然拖到圈外那一刻就会变成转视角、松手还停不下来。
/// 返回摇杆给出的移动意图（已按当前朝向转过，和键盘的 wish_from_keys 同一套基）。
fn read_touch(g: &mut Game, dt: f32) -> Option<[f32; 3]> {
    let (scx, scy, sr) = bridge::stick_geo();

    // 每帧核对：记着的那根手指要是已经不在屏上了（手机浏览器漏发一次抬手是常事），
    // 就直接忘掉它。以前是死等 —— 后来真按着的那根被当成"第二根"不理，
    // 玩家拖着拖着突然转不动，抬起来重按才好，就是这么来的。
    if !g.look.is_empty() {
        let live: Vec<u64> = touches().iter().map(|t| t.id).collect();
        g.look.retain(|l| live.contains(&l.id));
    }

    for t in touches() {
        // 归属先看"是不是这根手指正被摇杆记着"，再看它现在落在哪：摇杆那根划出圈也还是摇杆
        let owned = g.touch_move.map_or(false, |m| m.0 == t.id);
        // 「屏上还有没有活着的转向手指」：按下和拖动两支都要用，所以在 match 之前先算好。
        // 判的是**现在还在屏上的**那些，不是"游戏记着的那些"：记着的可能是一根已经被
        // 浏览器吞掉抬手的死手指，信它会白占位置、把接管挡死。
        let look_busy = touches()
            .iter()
            .any(|l| g.look.iter().any(|k| k.id == l.id));
        // 圆盘压在快捷栏上面是**故意的**（手机底下那一排手指够得着，圆盘也得够得着）。
        // 所以手指按在圆盘上就先算走路摇杆，哪怕它同时也压在格子上：
        // 真人要点格子的时候浏览器自己会把那一下交给格子 DOM，根本到不了画布；
        // 到得了画布的按下都是"想走路"。真机上这条重叠也极小（实测只有圆边上指甲盖大一块）。
        // 底栏命中判定留着是给格子附近那些擦边的按下做次要参考的（见下条 look 的判据）。
        let left = owned || stick_contains(scx, scy, sr, t.position);
        match t.phase {
            TouchPhase::Started => {
                // 新按下的手指按哪儿都不重要，先看有没有"活着的转向手指"（look_busy）：
                // 有就不抢（否则双指同时转，视角两倍快 —— 这条老判据就是抓这个的）。
                if left {
                    if g.touch_move.is_none() {
                        // 圆心用画出来的那个固定圆心，不是手指按下的位置：这样"拖多远 = 走多快"
                        // 和屏幕上那颗珠子对得上，玩家看珠子就知道自己推了多少。
                        let c = if sr > 0.0 { vec2(scx, scy) } else { t.position };
                        g.touch_move = Some((t.id, c, t.position));
                    }
                } else if !look_busy {
                    // 只收第一根（look_busy 已经保证屏上没有活着的转向手指）。
                    // 后面几根手指是玩家手滑或多指按住，再 push 一份就会各算各的位移，视角转两倍快。
                    g.look.push(LookTouch {
                        id: t.id,
                        last: t.position,
                        held: 0.0,
                        moved: 0.0,
                        placed: false,
                        dragging: false,
                    });
                }
            }
            TouchPhase::Moved | TouchPhase::Stationary => {
                // Stationary 也按移动那一档处理：Firefox 的触摸合成路径只会报 started / stationary
                // （真机浏览器才会报 moved），少了这一档的话——摇杆永远抓不到手指、
                // 转向也只认得按下不认得拖动。真机上这档几乎不出现，出现也只是"没动的新帧"，
                // 处理成"更新到当前点"两者都一样正确。
                if owned {
                    // 摇杆那根手指划到哪儿都继续算摇杆（跟手，不按当前落点换角色）
                    if let Some((_, _, cur)) = &mut g.touch_move {
                        *cur = t.position;
                    }
                } else if let Some(l) = g.look.iter_mut().find(|l| l.id == t.id) {
                    let d = t.position - l.last;
                    l.moved += d.length();
                    l.last = t.position;
                    // 只要真在拖，就把这次的放置取消掉。以前的判据是「0.45 秒时累计位移 < 24」，
                    // 于是慢慢拖（每帧两三个像素）一路都能熬到超时，视角也转了、方块也放出去了。
                    if l.moved > 12.0 {
                        l.dragging = true;
                    }
                    // 手指往哪儿拖，视角就往哪儿转：和桌面同一套符号（phys::turn_yaw_pitch）。
                    let sens = std::f32::consts::PI / 420.0;
                    phys::turn_yaw_pitch(&mut g.player.yaw, &mut g.player.pitch, d.x, d.y, sens);
                } else if !look_busy {
                    // 接管：原来记着的那根手指已经不在屏上了（手机浏览器漏发一次抬手是常事），
                    // 这根还按着在拖 —— 直接改认这根，玩家不用整根抬起来重按。
                    // 起点必须落在**这根手指现在的位置**上：拿它按下时的旧点算位移，
                    // 会瞬间倾泻一长串陈年位移，视角猛抽一下（修好一个卡、又造出一个乱跳）。
                    // 摇杆那根不算（它有自己的角色，见上面 owned）。
                    g.look.push(LookTouch {
                        id: t.id,
                        last: t.position,
                        held: 0.0,
                        moved: 0.0,
                        placed: false,
                        dragging: false,
                    });
                }
            }
            TouchPhase::Ended | TouchPhase::Cancelled => {
                if owned {
                    // 摇杆那根抬手：不管手最后停在圈里还是圈外，都必须清掉，不然走路停不下来
                    g.touch_move = None;
                } else {
                    // 抬手时既没长按触发放置、也没怎么移动，就当一次「挖」。
                    let hit = g.look.iter().find(|l| l.id == t.id).copied();
                    g.look.retain(|l| l.id != t.id);
                    if let Some(l) = hit {
                        if !l.placed && l.held < 0.45 && l.moved < 14.0 {
                            // 双击切飞行：手机上没有 F 键，光靠 DOM 按钮还不够顺手。
                            // 两下都算「没怎么动的轻点」、位置挨着、间隔不到 0.35 秒 → 当成双击。
                            // 第二下不再挖，不然双击变成「挖两下 + 切飞行」。
                            let near_prev = g.double_tap.map_or(false, |(t0, p0)| {
                                g.uptime - t0 < 0.35 && (t.position - p0).length() < 60.0
                            });
                            if near_prev && g.creative {
                                g.double_tap = None;
                                g.toggle_fly();
                            } else if near_prev {
                                // 生存里没有双击这个手势（飞行是创造的能力）。那就让它退化成
                                // 正常再挖一下：手指连点两下就该挖两下，别给一次「什么也没发生」。
                                g.double_tap = None;
                                g.mine();
                            } else {
                                g.double_tap = Some((g.uptime, t.position));
                                g.mine();
                            }
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
        if l.held > 0.45 && l.moved < 24.0 && !l.placed && !l.dragging {
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
        // 死区/满程都跟着画出来的半径走：珠子推到圈边 = 满速，推一点点 = 慢走
        let full = if sr > 0.0 { sr } else { 52.0 };
        let dead = full * 0.15;
        if d.length() >= dead {
            let len = d.length();
            let k = len.min(full) / full / len;
            // 摇杆量出来的是「屏幕方向」，得按当前朝向换成世界方向，和键盘那套基一模一样：
            // 屏幕向上 = 往前，屏幕向右 = 往右（yaw=0 时前向是 +Z，见 phys::look_dir）。
            // 原来直接写 [d.x, 0, d.y]：既没转朝向，又因为屏幕 y 向下为正，
            // 往上推摇杆被当成往后退，玩家想前进却朝前退。
            let fwd = phys::look_dir_of(g.player.yaw, 0.0); // 前向（水平）
            let right = phys::right_dir_of(g.player.yaw); // 屏幕右边对应的世界方向
            let f = -d.y * k; // 往上推 d.y 为负，取反才是「前进为正」
            let r = d.x * k;
            return Some([fwd[0] * f + right[0] * r, 0.0, fwd[2] * f + right[2] * r]);
        }
    }
    None
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
    let fwd = phys::look_dir_of(p.yaw, 0.0); // 前向（水平）
    let right = phys::right_dir_of(p.yaw); // 屏幕右边 —— 符号有单元测试钉着
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

/// 把 JS 递过来的存档灌进世界。读不懂 / 太大 / 只给了一半，一律当坏档：按种子重来。
/// 每一条失败分支都必须 `save_dirty = true`：不然那份坏档永远躺在 IndexedDB 里，
/// 读者刷新一次就崩一次（panic=abort，主循环连一帧都不续）。
fn apply_loaded_save(game: &mut Game) {
    let bytes = match bridge::pull_save() {
        // JS 那边确实没有存档：按种子开跑就行，不该顺手覆盖人家的档
        Ok(None) => return,
        Err(why) => {
            game.world = World::new(SEED);
            game.player = Player::new(game.world.spawn());
            game.save_dirty = true;
            game.set_note(why, NOTE_SECONDS);
            return;
        }
        Ok(Some(bytes)) => bytes,
    };
    match save::decode(&bytes) {
        Ok((seed, raw, saved_inv)) => {
            let Some(w) = World::from_raw(seed, raw) else {
                // 长度对不上等：世界建不起来，和坏档一样得按种子重来并覆盖掉它
                game.world = World::new(SEED);
                game.player = Player::new(game.world.spawn());
                game.save_dirty = true;
                game.set_note(BAD_SAVE_NOTE, NOTE_SECONDS);
                return;
            };
            game.world = w;
            game.player = Player::new(game.world.spawn());
            // v2 才有的背包段。老档（v1）没有这段，照旧用创造那套；有段但不合格也给开局那套。
            if let Some(saved) = saved_inv {
                if !game.load_inv(&saved) {
                    game.inv = inv::Inv::new_creative();
                    game.creative = true;
                }
            }
            // 拿到旧存档就不再自动存一份，除非读者真的改动过
        }
        Err(_) => {
            game.world = World::new(SEED);
            game.player = Player::new(game.world.spawn());
            game.save_dirty = true;
            game.set_note(BAD_SAVE_NOTE, NOTE_SECONDS);
        }
    }
}

#[macroquad::main("方块彩蛋 · Rust + wasm")]
async fn main() {
    bridge::install_panic_hook();

    // 触摸默认还会顺带点一份鼠标事件（一次轻点挖两下的根源）。这里鼠标和触屏各写各的，
    // 所以关掉；read_mouse 里还兜了一道「这一帧有手指就不走鼠标」。
    simulate_mouse_with_touch(false);

    // macroquad 每个 draw call 的容量默认 10000 顶点 / 5000 索引，而一个区块的网格最多
    // 5272 顶点 / 7908 索引：超出的部分被 clamp 掉，只 warn 一句「exceeded max drawcall size」
    // （实测每帧 31 条），丢掉的是每个区块 +Z 那头最后几行的面 —— 画面上每隔 16 格
    // 就有一条横着能看穿天空的缺面。索引是 u16，单个网格顶点上限 65536，这里离它还远。
    // 必须在窗口/GL 起来之后调（它要重建 GPU 缓冲），所以放在建材质之前第一件事。
    macroquad::window::gl_set_drawcall_buffer_capacity(16000, 12000);

    bridge::log("rust 起来了，开始建材质");
    let atlas = make_atlas();
    bridge::log("纹理图集好了");
    let material = make_material();
    bridge::log("材质编译通过");
    // 图集交给每个区块网格带着：保留槽 "Texture" 在每次 draw 时自动绑成这张。
    // （Texture2D 的 Clone 只是个句柄拷贝，36 个网格各拿一份不贵。）

    let mut game = Game::new(World::new(SEED));
    game.atlas = Some(atlas);
    // 帧率从 60 起步而不是从 0：0 起步做一次 0.08 混合，首帧 dt 只有几毫秒，
    // 1/dt 就是几百，HUD 上会先闪一个 800 之类的离谱数字再慢慢掉回真值。
    let mut fps_smooth = 60.0f32;
    let mut fps_report = 0.0f32;
    // 等存档的余量，每帧减 dt；花完就当「没有存档」直接开跑（见下面 1) 那段）。
    let mut wait_left = SAVE_WAIT_SECONDS;

    loop {
        let dt = get_frame_time().min(0.1);
        fps_smooth = fps_smooth * 0.92 + (1.0 / dt.max(0.0001)) * 0.08;
        fps_report -= dt;
        if fps_report <= 0.0 {
            game.last_fps = fps_smooth;
            fps_report = 0.5;
        }

        // 备注是自己会消失的话，别让它永远赖在第 8 段上
        if game.note_timer > 0.0 {
            game.note_timer -= dt;
            if game.note_timer <= 0.0 {
                game.note = "";
            }
        }

        // 1) 等存档。IndexedDB 读盘是异步的，读好之前不动物理、不建网格，
        //     免得先按种子建一遍、拿到存档再建一遍。
        if matches!(game.phase, Phase::Waiting) {
            match bridge::save_ready() {
                0 => {
                    wait_left -= dt;
                    if wait_left > 0.0 {
                        // 等存档期间每帧推一次 HUD：第 8 段就是「正在读本地存档…」，
                        // 这句说明只有这一处来源（以前底下还写死一句 HUD，那是死代码）。
                        game.push_hud();
                        next_frame().await;
                        continue;
                    }
                    bridge::log("等本地存档等太久，先按种子开跑");
                    game.set_note("读存档等太久，先按种子开了", NOTE_SECONDS);
                    game.mark_all_dirty();
                    game.phase = Phase::Running;
                    bridge::booted();
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
            // 双击判定要一个「游戏内的钟」，切后台那段不计入
            game.uptime += dt;

            // 手机端按钮是「取走即清」的，所以每帧都得各问一次、而且不能写在 || 的右边：
            // 短路跳过一次，JS 那个标志就留到下一帧再触发一遍（切飞机会连切两次）。
            let btn_jump = bridge::take_jump() != 0;
            let btn_fly = bridge::take_fly() != 0;
            let btn_slot = bridge::take_slot();

            // 「物品栏」面板发来的命令，一条一条吃到底（0 = 队列空）。
            loop {
                let cmd = bridge::take_ui_cmd();
                if cmd == 0 {
                    break;
                }
                game.ui_cmd(cmd);
            }

            // 面板开着：不许挖、不许放、不许走（手机上面板挡着屏，电脑上怕你边看边跑）
            if !game.paused {
                read_mouse(&mut game, dt);
            }
            let mut wish = if game.paused {
                [0.0f32; 3]
            } else {
                wish_from_keys(&game.player)
            };
            if !game.paused {
                if let Some(w) = read_touch(&mut game, dt) {
                    wish = w;
                }
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
                    game.inv.selected = i;
                }
            }

            // slot 是有符号增量（+1 下一个 / -1 上一个）：rem_euclid 让两头都能绕回栏里。
            if btn_slot != 0 {
                game.inv.cycle_selected(btn_slot);
            }

            // G：切创造 / 生存（物品栏里那个按钮走的是同一条命令 300）
            if is_key_pressed(KeyCode::G) && !game.paused {
                game.toggle_creative();
            }

            if (is_key_pressed(KeyCode::F) || btn_fly) && !game.paused {
                game.toggle_fly();
            }

            // 手机端没有键盘，跳全靠那排 DOM 按钮（触屏双击也能切飞行，见 read_touch）
            let jump = (is_key_down(KeyCode::Space) || btn_jump) && !game.paused;
            // Shift 是**潜行**，Ctrl 才是疾跑 —— 以前 Shift 当加速键是我的偷懒写法。
            // 真 MC 里 Shift 一直是潜行，而潜行有个要紧用处：走到边缘自己停住，
            // 边走边挖不会一步步挪下悬崖（手机上没 Shift，所以 HUD 上那个按钮按住就算）。
            let sneak = (is_key_down(KeyCode::LeftShift)
                || is_key_down(KeyCode::RightShift)
                || bridge::sneak_held())
                && !game.paused;
            let sprint =
                (is_key_down(KeyCode::LeftControl) || is_key_down(KeyCode::RightControl))
                    && !game.paused
                    && !game.player.fly;
            // 飞行的时候这个键是下降（真 MC 里下降用的也正是潜行键），所以照搬，不额外加键。
            let descend = sneak && game.player.fly;

            game.time_of_day = (game.time_of_day + dt / DAY_SECONDS).rem_euclid(1.0);
            game.player
                .update(&game.world, dt, wish, jump, descend, sprint, sneak);
            // 入水/落地/脚步：全靠物理前后的状态翻转，紧跟在 update 后面判
            game.play_frame_sounds(dt);

            // 挖掘进度：一会儿不点同一格就作废
            if game.mine.is_some() {
                game.mine_clock -= dt;
                if game.mine_clock <= 0.0 {
                    game.mine = None;
                    game.mine_total = 0;
                }
            }

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
        }
        // 这里原来挂着一个 else 分支，写死一句「正在读本地 IndexedDB」的 HUD：
        // 能走到这儿 phase 早就是 Running 了（Waiting 那条要么 continue 掉、要么刚切成 Running），
        // 那句永远不执行。这类说明话统一走 push_hud 的第 8 段，别再单独拼一份。

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
            // 水的动画：每 0.5 秒换一帧，覆写图集里 TILE_WATER 那一格（16×16，1024 字节）。
            // 只改纹理不改网格，所以挖开放水都不会因为动画多算一遍。暂停时不换 = 水也冻住，
            // 和「暂停就是停住」这条一致。图集是 Nearest 采样，半像素内缩的 UV 不会漏到邻贴。
            game.water_clock += dt;
            if game.water_clock >= 0.5 {
                game.water_clock -= 0.5;
                game.water_frame = (game.water_frame + 1) % b::WATER_FRAMES;
                if let Some(ref atlas) = game.atlas {
                    // macroquad 0.4.16 的 Image 没有 from_rgba，字段倒是全 public，直接拼。
                    let img = macroquad::texture::Image {
                        bytes: b::water_frame_rgba(game.water_frame),
                        width: b::TILE_PX as u16,
                        height: b::TILE_PX as u16,
                    };
                    atlas.update_part(
                        &img,
                        (b::TILE_WATER as i32) * (b::TILE_PX as i32),
                        0,
                        b::TILE_PX as i32,
                        b::TILE_PX as i32,
                    );
                }
            }
            // 光照色表：白天程度（量化成 64 档）一动就重传那 1 KB。顶点里存的是等级不是颜色，
            // 所以换昼夜既不重建网格也不重传顶点 —— 这是把光照塞进顶点颜色空槽换来的便宜。
            material.set_texture("u_light", game.lightmap.clone());
            let bucket = (daylight(game.time_of_day) * 64.0) as i32;
            if bucket != game.lightmap_bucket {
                game.lightmap_bucket = bucket;
                let bytes = lightmap_rgba(game.time_of_day).to_vec();
                let img = macroquad::texture::Image {
                    bytes,
                    width: 16,
                    height: 16,
                };
                game.lightmap.update_part(&img, 0, 0, 16, 16);
            }
            material.set_uniform("u_eye", eye_v);
            material.set_uniform(
                "u_fog_color",
                vec3(sky.r, sky.g, sky.b),
            );
            material.set_uniform("u_fog_near", FOG_NEAR);
            material.set_uniform("u_fog_far", FOG_FAR);
            material.set_uniform("u_bright", BRIGHT[game.bright as usize % BRIGHT.len()]);

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

// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 摇杆判定认圆心那一圈不认半屏() {
        let c = vec2(100.0, 200.0);
        assert!(stick_contains(c.x, c.y, 50.0, vec2(100.0, 200.0)), "正圆心当然算摇杆");
        assert!(stick_contains(c.x, c.y, 50.0, vec2(145.0, 200.0)), "圈边内 5 像素算摇杆");
        // 30-40-50 是勾股数：斜着到圈边在浮点上也是精确的 50，不会因为我多点少点一个像素翻面
        assert!(stick_contains(c.x, c.y, 50.0, vec2(130.0, 240.0)), "斜着到圈边也算（半径是各向同性的）");
        assert!(!stick_contains(c.x, c.y, 50.0, vec2(151.0, 200.0)), "出圈 1 像素就该算转向（半径是硬边，不吞手指）");
        assert!(!stick_contains(c.x, c.y, 50.0, vec2(100.0, 251.0)), "正下方出圈同样算转向");
        assert!(!stick_contains(c.x, c.y, 50.0, vec2(10.0, 200.0)), "圆心左边 90 像素早出圈了");
        assert!(stick_contains(c.x, c.y, 50.0, vec2(135.35, 235.35)), "差一点点到圈边，仍在圈内");
    }

    #[test]
    fn 摇杆判定对负坐标和极大半径也不能算错() {
        assert!(stick_contains(-500.0, -900.0, 10.0, vec2(-505.0, -908.0)), "负坐标照样按距离算");
        assert!(!stick_contains(-500.0, -900.0, 10.0, vec2(-489.0, -900.0)), "负坐标出圈也要出圈");
        assert!(
            stick_contains(0.0, 0.0, 100000.0, vec2(99999.0, 0.0)),
            "半径极大时圈内在数学上还是圈内（不能因为数大就漏判）"
        );
        assert!(
            !stick_contains(0.0, 0.0, 100000.0, vec2(100001.0, 0.0)),
            "同理出圈就是出圈，数大不能把它拉回圈内"
        );
    }

    /// HUD 那个钟得跟明暗对得上。以前夜不真黑，钟和光各说各话没人看得出来；
    /// 现在夜是黑的，钟写「13:30」而外面一片黑就是明显的自相矛盾，所以钉住几个点。
    #[test]
    fn 显示的钟要和明暗对得上不能各说各话() {
        // daylight 的相位：t<0.38 满亮，0.38..0.52 转暗，0.52..0.86 是夜，0.86 之后回亮
        assert_eq!(clock_text(0.20), "11:48", "最亮的时候该在正午前后");
        assert_eq!(clock_text(0.52), "19:28", "天刚黑该是傍晚，不该是下午");
        assert_eq!(clock_text(0.68), "23:19", "最黑的时候该在半夜前后");
        assert_eq!(clock_text(0.06), "08:26", "早上该是早上");
    }
}
