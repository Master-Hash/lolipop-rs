#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use std::cell::RefCell;
use std::collections::VecDeque;
use std::os::raw;
use std::sync::OnceLock;
use std::time::Instant;

use windows::UI::Color;
use windows::UI::Composition::CompositionRectangleGeometry;
use windows::UI::Composition::Compositor;
use windows::UI::Composition::Desktop::DesktopWindowTarget;
use windows::UI::Composition::ShapeVisual;
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::COINIT_APARTMENTTHREADED;
use windows::Win32::System::Com::CoInitializeEx;
use windows::Win32::System::WinRT::Composition::ICompositorDesktopInterop;
use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
use windows::core::Interface;
use windows::core::Result as WinResult;
use windows_numerics::Vector2;

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

#[unsafe(no_mangle)]
#[allow(non_upper_case_globals)]
pub static plugin_is_GPL_compatible: libc::c_int = 1;

// Bezier easing function (ease-in-out cubic)
fn bezier(t: f32) -> f32 {
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        let u = 2.0 * t - 2.0;
        0.5 * u * u * u + 1.0
    }
}

fn clamp01(x: f32) -> f32 {
    x.clamp(0.0, 1.0)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

// Helper function to calculate bounding rect from 4 corners
fn corners_to_rect(
    tl: (f32, f32),
    tr: (f32, f32),
    br: (f32, f32),
    bl: (f32, f32),
) -> (f32, f32, f32, f32) {
    let min_x = tl.0.min(tr.0).min(br.0).min(bl.0);
    let max_x = tl.0.max(tr.0).max(br.0).max(bl.0);
    let min_y = tl.1.min(tr.1).min(br.1).min(bl.1);
    let max_y = tl.1.max(tr.1).max(br.1).max(bl.1);
    (min_x, min_y, max_x - min_x, max_y - min_y)
}

#[derive(Clone, Copy)]
struct Point {
    x: f32,
    y: f32,
}

#[derive(Clone, Copy)]
struct Size {
    width: f32,
    height: f32,
}

#[derive(Clone, Copy)]
struct Rect {
    origin: Point,
    size: Size,
}

impl Rect {
    fn min_x(&self) -> f32 {
        self.origin.x
    }
    fn max_x(&self) -> f32 {
        self.origin.x + self.size.width
    }
    fn min_y(&self) -> f32 {
        self.origin.y
    }
    fn max_y(&self) -> f32 {
        self.origin.y + self.size.height
    }
}

#[allow(dead_code)]
struct AnimationFrame {
    shape: windows::UI::Composition::CompositionSpriteShape,
    geometry: CompositionRectangleGeometry,
    start_time: Instant,
    duration: f32,
    paths: Vec<(f32, f32, f32, f32)>, // (x, y, width, height) for each frame
}

struct LolipopState {
    hwnd: HWND,
    compositor: Option<Compositor>,
    target: Option<DesktopWindowTarget>,
    visual: Option<ShapeVisual>,
    color: Color,
    state: bool,
    previous_position: Point,
    previous_geometry: Size,
    animations: VecDeque<AnimationFrame>,
}

impl Default for LolipopState {
    fn default() -> Self {
        Self {
            hwnd: HWND::default(),
            compositor: None,
            target: None,
            visual: None,
            color: Color {
                A: 255,
                R: 255,
                G: 255,
                B: 255,
            },
            state: false,
            previous_position: Point { x: 0.0, y: 0.0 },
            previous_geometry: Size {
                width: 0.0,
                height: 0.0,
            },
            animations: VecDeque::new(),
        }
    }
}

thread_local! {
    static LOLIPOP: RefCell<LolipopState> = RefCell::new(LolipopState::default());
}

static INITIALIZED: OnceLock<bool> = OnceLock::new();

fn initialize_com() {
    INITIALIZED.get_or_init(|| {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        }
        true
    });
}

fn get_monitor_refresh_rate() -> u32 {
    // Default to 60 FPS, could query actual refresh rate if needed
    60
}

fn ensure_compositor(state: &mut LolipopState, hwnd: HWND) -> WinResult<()> {
    if state.hwnd != hwnd || state.compositor.is_none() {
        state.hwnd = hwnd;
        state.animations.clear();

        let compositor = Compositor::new()?;
        let interop: ICompositorDesktopInterop = compositor.cast()?;

        let target: DesktopWindowTarget =
            unsafe { interop.CreateDesktopWindowTarget(hwnd, false)? };

        let visual = compositor.CreateShapeVisual()?;
        visual.SetRelativeSizeAdjustment(Vector2::new(1.0, 1.0))?;

        let root = compositor.CreateContainerVisual()?;
        root.SetRelativeSizeAdjustment(Vector2::new(1.0, 1.0))?;
        root.Children()?.InsertAtTop(&visual)?;

        target.SetRoot(&root)?;

        state.compositor = Some(compositor);
        state.target = Some(target);
        state.visual = Some(visual);
        state.state = false;
    }
    Ok(())
}

fn lolipop_crush(
    state: &mut LolipopState,
    current_position: Point,
    current_geometry: Size,
) -> WinResult<()> {
    if (current_position.x - state.previous_position.x).abs() < 0.001
        && (current_position.y - state.previous_position.y).abs() < 0.001
    {
        return Ok(());
    }

    let compositor = match &state.compositor {
        Some(c) => c,
        None => return Ok(()),
    };

    let visual = match &state.visual {
        Some(v) => v,
        None => return Ok(()),
    };

    let previous_cursor = Rect {
        origin: state.previous_position,
        size: state.previous_geometry,
    };
    let current_cursor = Rect {
        origin: current_position,
        size: current_geometry,
    };

    let dx = current_position.x - state.previous_position.x;
    let dy = current_position.y - state.previous_position.y;

    let distance = (dx * dx + dy * dy).sqrt();
    let duration = 0.6 * (distance / 400.0).tanh();

    let fps = get_monitor_refresh_rate();
    let num_frames = ((duration * fps as f32).ceil() as usize).max(1);

    // Pre-calculate all frame positions as bounding rectangles
    let mut paths: Vec<(f32, f32, f32, f32)> = Vec::with_capacity(num_frames + 1);

    for frame in 0..=num_frames {
        let alpha = frame as f32 / num_frames as f32;
        let fast = bezier(clamp01(1.6 * alpha));
        let norm = bezier(clamp01(1.6 * (alpha - 0.2)));
        let slow = bezier(clamp01(1.6 * (alpha - 0.4)));

        let (tl_ease, tr_ease, br_ease, bl_ease) = if dx.abs() < current_geometry.width {
            if dy > 0.0 {
                (slow, slow, fast, fast)
            } else {
                (fast, fast, slow, slow)
            }
        } else if dy.abs() < current_geometry.height {
            if dx > 0.0 {
                (slow, fast, fast, slow)
            } else {
                (fast, slow, slow, fast)
            }
        } else if dx > 0.0 {
            if dy > 0.0 {
                (slow, norm, fast, norm)
            } else {
                (norm, fast, norm, slow)
            }
        } else if dy > 0.0 {
            (norm, slow, norm, fast)
        } else {
            (fast, norm, slow, norm)
        };

        let tl = (
            lerp(previous_cursor.min_x(), current_cursor.min_x(), tl_ease),
            lerp(previous_cursor.min_y(), current_cursor.min_y(), tl_ease),
        );
        let tr = (
            lerp(previous_cursor.max_x(), current_cursor.max_x(), tr_ease),
            lerp(previous_cursor.min_y(), current_cursor.min_y(), tr_ease),
        );
        let br = (
            lerp(previous_cursor.max_x(), current_cursor.max_x(), br_ease),
            lerp(previous_cursor.max_y(), current_cursor.max_y(), br_ease),
        );
        let bl = (
            lerp(previous_cursor.min_x(), current_cursor.min_x(), bl_ease),
            lerp(previous_cursor.max_y(), current_cursor.max_y(), bl_ease),
        );

        paths.push(corners_to_rect(tl, tr, br, bl));
    }

    // Create a sprite shape for the animation
    let shape = compositor.CreateSpriteShape()?;
    let brush = compositor.CreateColorBrush()?;
    brush.SetColor(state.color)?;
    shape.SetFillBrush(&brush)?;

    // Create geometry and set initial state from first frame
    let geometry = compositor.CreateRectangleGeometry()?;
    if let Some(&(x, y, w, h)) = paths.first() {
        geometry.SetOffset(Vector2::new(x, y))?;
        geometry.SetSize(Vector2::new(w, h))?;
    }
    shape.SetGeometry(&geometry)?;

    visual.Shapes()?.Append(&shape)?;

    // Store animation state for manual updates
    state.animations.push_back(AnimationFrame {
        shape,
        geometry,
        start_time: Instant::now(),
        duration,
        paths,
    });

    // Clean up old completed animations
    update_animations(state)?;

    Ok(())
}

fn update_animations(state: &mut LolipopState) -> WinResult<()> {
    let visual = match &state.visual {
        Some(v) => v,
        None => return Ok(()),
    };

    let shapes = visual.Shapes()?;
    let now = Instant::now();

    // Track indices to remove
    let mut to_remove = Vec::new();

    for (idx, anim) in state.animations.iter_mut().enumerate() {
        let elapsed = now.duration_since(anim.start_time).as_secs_f32();

        if elapsed >= anim.duration {
            to_remove.push(idx);
            continue;
        }

        // Calculate current frame
        let progress = elapsed / anim.duration;
        let frame_idx =
            ((progress * (anim.paths.len() - 1) as f32) as usize).min(anim.paths.len() - 1);

        if let Some(&(x, y, w, h)) = anim.paths.get(frame_idx) {
            let _ = anim.geometry.SetOffset(Vector2::new(x, y));
            let _ = anim.geometry.SetSize(Vector2::new(w, h));
        }
    }

    // Remove completed animations from back to front
    for idx in to_remove.into_iter().rev() {
        // Remove the shape from the visual's shapes collection
        if shapes.Size().is_ok_and(|size| size > 0) {
            // Remove the oldest shape (at index 0) since animations complete in order
            let _ = shapes.RemoveAt(0);
        }
        state.animations.remove(idx);
    }

    Ok(())
}

fn lolipop_chew(x: f32, y: f32, width: f32, height: f32, render: bool) -> WinResult<()> {
    let hwnd = unsafe { GetForegroundWindow() };

    LOLIPOP.with(|lolipop| {
        let mut state = lolipop.borrow_mut();

        ensure_compositor(&mut state, hwnd)?;

        let position = Point { x, y };
        let geometry = Size { width, height };

        // Update any ongoing animations
        let _ = update_animations(&mut state);

        if state.state && render {
            lolipop_crush(&mut state, position, geometry)?;
        }

        state.state = true;
        state.previous_position = position;
        state.previous_geometry = geometry;

        Ok(())
    })
}

unsafe extern "C" fn lolipop_lick(
    env: *mut emacs_env,
    _nargs: isize,
    args: *mut emacs_value,
    _data: *mut raw::c_void,
) -> emacs_value {
    unsafe {
        let is_not_nil = (*env).is_not_nil.unwrap_unchecked();
        let extract_integer = (*env).extract_integer.unwrap_unchecked();
        let extract_float = (*env).extract_float.unwrap_unchecked();
        let intern = (*env).intern.unwrap_unchecked();

        let render = is_not_nil(env, *args.offset(0));

        let x = extract_integer(env, *args.offset(1)) as f32;
        let y = extract_integer(env, *args.offset(2)) as f32;
        let width = extract_integer(env, *args.offset(3)) as f32;
        let height = extract_integer(env, *args.offset(4)) as f32;

        let red = (extract_float(env, *args.offset(5)) * 255.0) as u8;
        let green = (extract_float(env, *args.offset(6)) * 255.0) as u8;
        let blue = (extract_float(env, *args.offset(7)) * 255.0) as u8;

        LOLIPOP.with(|lolipop| {
            lolipop.borrow_mut().color = Color {
                A: 255,
                R: red,
                G: green,
                B: blue,
            };
        });

        let _ = lolipop_chew(x, y, width, height, render);

        intern(env, c"nil".as_ptr())
    }
}

#[unsafe(no_mangle)]
#[allow(clippy::missing_safety_doc)]
pub unsafe extern "C" fn emacs_module_init(runtime: *mut emacs_runtime) -> libc::c_int {
    initialize_com();

    unsafe {
        let env = (*runtime).get_environment.unwrap_unchecked()(runtime);

        let intern = (*env).intern.unwrap_unchecked();
        let funcall = (*env).funcall.unwrap_unchecked();
        let make_function = (*env).make_function.unwrap_unchecked();

        let function_doc = c"Render the cursor animation on a separate window layer.

RENDER controls whether the cursor animation is rendered.  X and Y
specify the cursor position in pixels.  WIDTH and HEIGHT specify the
cursor size.  RED, GREEN and BLUE specify the cursor color channels.

The animation is rendered on a dedicated layer attached to current
frame and does not participate in Emacs redisplay.

If RENDER is nil, only internal cursor state is updated.

(fn RENDER X Y WIDTH HEIGHT RED GREEN BLUE)";

        let function = make_function(
            env,
            8,
            8,
            Some(lolipop_lick),
            function_doc.as_ptr(),
            std::ptr::null_mut(),
        );

        let symbol = intern(env, c"lolipop-lick".as_ptr());
        let defalias = intern(env, c"defalias".as_ptr());

        funcall(env, defalias, 2, [symbol, function].as_mut_ptr());

        0
    }
}
