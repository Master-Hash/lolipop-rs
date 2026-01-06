#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

mod common;

use std::cell::RefCell;
use std::collections::VecDeque;
use std::os::raw;
use std::sync::OnceLock;
use windows::Foundation::TypedEventHandler;
use windows::UI::Color;
use windows::UI::Composition::CompositionBatchTypes;
use windows::UI::Composition::Compositor;
use windows::UI::Composition::Desktop::DesktopWindowTarget;
use windows::UI::Composition::ShapeVisual;
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::COINIT_APARTMENTTHREADED;
use windows::Win32::System::Com::CoInitializeEx;
use windows::Win32::System::WinRT::Composition::ICompositorDesktopInterop;
use windows::Win32::System::WinRT::RO_INIT_SINGLETHREADED;
use windows::Win32::System::WinRT::RoInitialize;
use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
use windows::core::HRESULT;
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
struct Size {
    width: f32,
    height: f32,
}

#[derive(Clone, Copy)]
struct Rect {
    origin: Vector2,
    size: Size,
}

impl Rect {
    fn min_x(&self) -> f32 {
        self.origin.X
    }
    fn max_x(&self) -> f32 {
        self.origin.X + self.size.width
    }
    fn min_y(&self) -> f32 {
        self.origin.Y
    }
    fn max_y(&self) -> f32 {
        self.origin.Y + self.size.height
    }
}

#[allow(dead_code)]
struct AnimationEntry {
    shape: windows::UI::Composition::CompositionSpriteShape,
    batch: windows::UI::Composition::CompositionScopedBatch,
}

struct LolipopState {
    hwnd: HWND,
    compositor: Option<Compositor>,
    target: Option<DesktopWindowTarget>,
    visual: Option<ShapeVisual>,
    color: Color,
    state: bool,
    previous_position: Vector2,
    previous_geometry: Size,
    animations: VecDeque<AnimationEntry>,
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
            previous_position: Vector2::new(0.0, 0.0),
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

#[repr(C)]
struct DispatcherQueueOptions {
    dw_size: u32,
    thread_type: i32,
    apartment_type: i32,
}

#[link(name = "coremessaging")]
unsafe extern "system" {
    fn CreateDispatcherQueueController(
        options: DispatcherQueueOptions,
        dispatcher_queue_controller: *mut *mut std::ffi::c_void,
    ) -> HRESULT;
}

fn create_dispatcher_queue_controller() -> WinResult<*mut std::ffi::c_void> {
    let options = DispatcherQueueOptions {
        dw_size: std::mem::size_of::<DispatcherQueueOptions>() as u32,
        thread_type: 2,    // DQTYPE_THREAD_CURRENT
        apartment_type: 2, // DQTAT_COM_STA
    };

    let mut controller: *mut std::ffi::c_void = std::ptr::null_mut();
    unsafe {
        CreateDispatcherQueueController(options, &mut controller).ok()?;
        Ok(controller)
    }
}

fn initialize_com() {
    INITIALIZED.get_or_init(|| {
        unsafe {
            let _ = RoInitialize(RO_INIT_SINGLETHREADED);
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            // 创建 DispatcherQueue，Composition API 需要它来处理动画
            let _ = create_dispatcher_queue_controller();
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
    current_position: Vector2,
    current_geometry: Size,
) -> WinResult<()> {
    if (current_position.X - state.previous_position.X).abs() < 0.001
        && (current_position.Y - state.previous_position.Y).abs() < 0.001
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

    let dx = current_position.X - state.previous_position.X;
    let dy = current_position.Y - state.previous_position.Y;

    let distance = (dx * dx + dy * dy).sqrt();
    let duration = 0.6 * (distance / 400.0).tanh();

    let fps = get_monitor_refresh_rate();
    let num_frames = ((duration * fps as f32).ceil() as usize).max(1);

    // Create a sprite shape for the animation
    let shape = compositor.CreateSpriteShape()?;
    let brush = compositor.CreateColorBrush()?;
    brush.SetColor(state.color)?;
    shape.SetFillBrush(&brush)?;

    // Create geometry with initial position
    let geometry = compositor.CreateRectangleGeometry()?;
    geometry.SetOffset(Vector2::new(
        previous_cursor.min_x(),
        previous_cursor.min_y(),
    ))?;
    geometry.SetSize(Vector2::new(
        previous_cursor.size.width,
        previous_cursor.size.height,
    ))?;
    shape.SetGeometry(&geometry)?;

    visual.Shapes()?.Append(&shape)?;

    // Create keyframe animations for Offset and Size
    let offset_animation = compositor.CreateVector2KeyFrameAnimation()?;
    let size_animation = compositor.CreateVector2KeyFrameAnimation()?;

    let duration_timespan = windows::Foundation::TimeSpan {
        Duration: (duration * 10_000_000.0) as i64, // 100-nanosecond units
    };
    offset_animation.SetDuration(duration_timespan)?;
    size_animation.SetDuration(duration_timespan)?;

    // Add keyframes for each animation frame
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

        let (x, y, w, h) = corners_to_rect(tl, tr, br, bl);

        // Linear progress for keyframe position
        let progress = alpha;
        offset_animation.InsertKeyFrame(progress, Vector2::new(x, y))?;
        size_animation.InsertKeyFrame(progress, Vector2::new(w, h))?;
    }

    // Create a scoped batch to track animation completion
    let batch = compositor.CreateScopedBatch(CompositionBatchTypes::Animation)?;

    // Start the animations on the geometry
    geometry.StartAnimation(windows::core::h!("Offset"), &offset_animation)?;
    geometry.StartAnimation(windows::core::h!("Size"), &size_animation)?;

    batch.End()?;

    // Set up completion handler to remove the shape when animation ends
    let shapes = visual.Shapes()?;
    let shape_clone = shape.clone();
    batch.Completed(&TypedEventHandler::new(move |_batch, _args| {
        // Find and remove the shape by iterating through the collection
        if let Ok(count) = shapes.Size() {
            for i in 0..count {
                if let Ok(s) = shapes.GetAt(i) {
                    // Use IUnknown comparison for COM object identity
                    use windows::core::Interface;
                    if let (Ok(unk1), Ok(unk2)) = (
                        s.cast::<windows::core::IUnknown>(),
                        shape_clone.cast::<windows::core::IUnknown>(),
                    ) && unk1 == unk2
                    {
                        let _ = shapes.RemoveAt(i);
                        break;
                    }
                }
            }
        }
        Ok(())
    }))?;

    // Store animation entry (for tracking, though cleanup is handled by the completion handler)
    state.animations.push_back(AnimationEntry { shape, batch });

    // Clean up old entries from the deque (they should already be removed by completion handlers)
    cleanup_finished_animations(state);

    Ok(())
}

fn cleanup_finished_animations(state: &mut LolipopState) {
    // Remove entries where the batch has already completed
    // Since we use completion handlers, we just limit the queue size
    while state.animations.len() > 100 {
        state.animations.pop_front();
    }
}

fn lolipop_chew(x: f32, y: f32, width: f32, height: f32, render: bool) -> WinResult<()> {
    let hwnd = unsafe { GetForegroundWindow() };

    LOLIPOP.with(|lolipop| {
        let mut state = lolipop.borrow_mut();

        ensure_compositor(&mut state, hwnd)?;

        let position = Vector2::new(x, y);
        let geometry = Size { width, height };

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

unsafe extern "C" fn lolipop_test_play(
    env: *mut emacs_env,
    _nargs: isize,
    _args: *mut emacs_value,
    _data: *mut raw::c_void,
) -> emacs_value {
    unsafe {
        let intern = (*env).intern.unwrap_unchecked();
        let hwnd = GetForegroundWindow();

        match common::play_demo_animation(hwnd) {
            Ok(_) => intern(env, c"t".as_ptr()),
            Err(_) => intern(env, c"nil".as_ptr()),
        }
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

        // 注册 test-play 函数
        let test_play_doc = c"Play a test animation on the current Emacs frame.

This function creates colorful bouncing, scaling and rotating squares
on top of the current Emacs window for demonstration purposes.

(fn)";

        let test_play_function = make_function(
            env,
            0,
            0,
            Some(lolipop_test_play),
            test_play_doc.as_ptr(),
            std::ptr::null_mut(),
        );

        let test_play_symbol = intern(env, c"lolipop-test-play".as_ptr());
        funcall(
            env,
            defalias,
            2,
            [test_play_symbol, test_play_function].as_mut_ptr(),
        );

        0
    }
}
