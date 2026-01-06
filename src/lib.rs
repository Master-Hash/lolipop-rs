#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

mod common;

use std::cell::RefCell;
use std::collections::VecDeque;
use std::os::raw;
use std::sync::OnceLock;
use windows::Foundation::TypedEventHandler;
use windows::Graphics::IGeometrySource2D;
use windows::UI::Color;
use windows::UI::Composition::CompositionBatchTypes;
use windows::UI::Composition::CompositionPath;
use windows::UI::Composition::Compositor;
use windows::UI::Composition::Desktop::DesktopWindowTarget;
use windows::UI::Composition::ShapeVisual;
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Direct2D::Common::D2D1_BEZIER_SEGMENT;
use windows::Win32::Graphics::Direct2D::Common::D2D1_FIGURE_BEGIN_FILLED;
use windows::Win32::Graphics::Direct2D::Common::D2D1_FIGURE_END_CLOSED;
use windows::Win32::Graphics::Direct2D::Common::D2D1_FILL_MODE_WINDING;
use windows::Win32::Graphics::Direct2D::D2D1_FACTORY_OPTIONS;
use windows::Win32::Graphics::Direct2D::D2D1_FACTORY_TYPE_SINGLE_THREADED;
use windows::Win32::Graphics::Direct2D::D2D1CreateFactory;
use windows::Win32::Graphics::Direct2D::ID2D1Factory1;
use windows::Win32::Graphics::Direct2D::ID2D1PathGeometry1;
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

#[allow(dead_code)]
fn clamp01(x: f32) -> f32 {
    x.clamp(0.0, 1.0)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

// Helper function to calculate bounding rect from 4 corners
#[allow(dead_code)]
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

#[allow(dead_code)]
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
    d2d_factory: Option<ID2D1Factory1>,
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
            d2d_factory: None,
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

        // Create Direct2D Factory for path geometry
        let d2d_factory: ID2D1Factory1 = unsafe {
            D2D1CreateFactory(
                D2D1_FACTORY_TYPE_SINGLE_THREADED,
                Some(&D2D1_FACTORY_OPTIONS::default()),
            )?
        };

        state.compositor = Some(compositor);
        state.target = Some(target);
        state.visual = Some(visual);
        state.d2d_factory = Some(d2d_factory);
        state.state = false;
    }
    Ok(())
}

/// Create a ribbon-shaped path geometry using Direct2D
/// The ribbon connects two cursor positions with smooth bezier curves
#[allow(clippy::too_many_arguments)]
fn create_ribbon_path(
    factory: &ID2D1Factory1,
    // Trailing edge (start) position and dimensions
    trail_x: f32,
    trail_y: f32,
    trail_width: f32,
    trail_height: f32,
    // Leading edge (end) position and dimensions
    lead_x: f32,
    lead_y: f32,
    lead_width: f32,
    lead_height: f32,
) -> WinResult<ID2D1PathGeometry1> {
    let path_geometry = unsafe { factory.CreatePathGeometry()? };
    let sink = unsafe { path_geometry.Open()? };

    unsafe {
        sink.SetFillMode(D2D1_FILL_MODE_WINDING);

        // Calculate corner points for the ribbon
        // Trail (start) rectangle corners - right edge
        let trail_tr = Vector2::new(trail_x + trail_width, trail_y);
        let trail_br = Vector2::new(trail_x + trail_width, trail_y + trail_height);

        // Lead (end) rectangle corners - left edge
        let lead_tl = Vector2::new(lead_x, lead_y);
        let lead_bl = Vector2::new(lead_x, lead_y + lead_height);

        // Calculate direction vector from trail to lead
        let dx = (lead_x + lead_width / 2.0) - (trail_x + trail_width / 2.0);
        let dy = (lead_y + lead_height / 2.0) - (trail_y + trail_height / 2.0);
        let dist = dx.hypot(dy).max(1.0);

        // Control point offset for bezier curves (proportional to distance)
        let ctrl_offset = dist * 0.4;
        let dir_x = dx / dist;
        let dir_y = dy / dist;

        // Start the figure from trail top-right corner
        sink.BeginFigure(trail_tr, D2D1_FIGURE_BEGIN_FILLED);

        // Top edge: bezier from trail_tr to lead_tl
        let top_ctrl1 = Vector2::new(
            trail_tr.X + ctrl_offset * dir_x,
            trail_tr.Y + ctrl_offset * dir_y,
        );
        let top_ctrl2 = Vector2::new(
            lead_tl.X - ctrl_offset * dir_x,
            lead_tl.Y - ctrl_offset * dir_y,
        );
        sink.AddBezier(&D2D1_BEZIER_SEGMENT {
            point1: top_ctrl1,
            point2: top_ctrl2,
            point3: lead_tl,
        });

        // Lead left edge: line from lead_tl to lead_bl
        sink.AddLines(&[lead_bl]);

        // Bottom edge: bezier from lead_bl to trail_br
        let bottom_ctrl1 = Vector2::new(
            lead_bl.X - ctrl_offset * dir_x,
            lead_bl.Y - ctrl_offset * dir_y,
        );
        let bottom_ctrl2 = Vector2::new(
            trail_br.X + ctrl_offset * dir_x,
            trail_br.Y + ctrl_offset * dir_y,
        );
        sink.AddBezier(&D2D1_BEZIER_SEGMENT {
            point1: bottom_ctrl1,
            point2: bottom_ctrl2,
            point3: trail_br,
        });

        // Trail right edge closes back to start
        sink.EndFigure(D2D1_FIGURE_END_CLOSED);
        sink.Close()?;
    }

    Ok(path_geometry)
}

fn lolipop_crush(
    state: &mut LolipopState,
    current_position: Vector2,
    current_geometry: Size,
) -> WinResult<()> {
    // Match NSEqualPoints behavior - exact comparison
    if current_position.X == state.previous_position.X
        && current_position.Y == state.previous_position.Y
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

    let d2d_factory = match &state.d2d_factory {
        Some(f) => f,
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

    // Use hypot like ObjC version
    let distance = dx.hypot(dy);
    let duration = 0.6 * (distance / 400.0).tanh();

    // Query actual refresh rate if available, otherwise use 60fps
    let fps = get_monitor_refresh_rate();
    let num_frames = (duration * fps as f32).ceil() as usize;

    // Skip animation if duration is too short
    if num_frames == 0 {
        return Ok(());
    }

    // Calculate the center points of start and end cursors
    let start_x = previous_cursor.origin.X;
    let start_y = previous_cursor.origin.Y;
    let start_width = previous_cursor.size.width;
    let start_height = previous_cursor.size.height;

    let end_x = current_cursor.origin.X;
    let end_y = current_cursor.origin.Y;
    let end_width = current_cursor.size.width;
    let end_height = current_cursor.size.height;

    // Create initial ribbon path geometry using D2D
    let initial_path = create_ribbon_path(
        d2d_factory,
        start_x,
        start_y,
        start_width,
        start_height,
        start_x,
        start_y,
        start_width,
        start_height,
    )?;

    // Convert D2D geometry to CompositionPath via IGeometrySource2D
    let geometry_source: IGeometrySource2D = initial_path.cast()?;
    let composition_path = CompositionPath::Create(&geometry_source)?;
    let path_geometry = compositor.CreatePathGeometry()?;
    path_geometry.SetPath(&composition_path)?;

    // Create a sprite shape for the animation
    let shape = compositor.CreateSpriteShape()?;
    let brush = compositor.CreateColorBrush()?;
    brush.SetColor(state.color)?;
    shape.SetFillBrush(&brush)?;
    shape.SetGeometry(&path_geometry)?;

    visual.Shapes()?.Append(&shape)?;

    // Create path keyframe animation
    let path_animation = compositor.CreatePathKeyFrameAnimation()?;
    let duration_timespan = windows::Foundation::TimeSpan {
        Duration: (duration * 10_000_000.0) as i64,
    };
    path_animation.SetDuration(duration_timespan)?;

    use windows::UI::Composition::AnimationStopBehavior;
    path_animation.SetStopBehavior(AnimationStopBehavior::SetToFinalValue)?;

    // Animation strategy:
    // - The leading edge (front) moves fast toward the target
    // - The trailing edge (back) follows slower
    // - Creates a ribbon/trail effect

    for frame in 0..=num_frames {
        let t = frame as f32 / num_frames as f32;

        // Leading edge moves faster
        let lead_t = bezier(clamp01(t * 1.5));
        // Trailing edge follows slower
        let trail_t = bezier(clamp01((t - 0.3) * 1.5));

        // Calculate positions of leading and trailing edges
        let trail_x = lerp(start_x, end_x, trail_t);
        let trail_y = lerp(start_y, end_y, trail_t);
        let trail_width = lerp(start_width, end_width, trail_t);
        let trail_height = lerp(start_height, end_height, trail_t);

        let lead_x = lerp(start_x, end_x, lead_t);
        let lead_y = lerp(start_y, end_y, lead_t);
        let lead_width = lerp(start_width, end_width, lead_t);
        let lead_height = lerp(start_height, end_height, lead_t);

        // Create path for this keyframe
        let frame_path = create_ribbon_path(
            d2d_factory,
            trail_x,
            trail_y,
            trail_width,
            trail_height,
            lead_x,
            lead_y,
            lead_width,
            lead_height,
        )?;

        let frame_geometry_source: IGeometrySource2D = frame_path.cast()?;
        let frame_composition_path = CompositionPath::Create(&frame_geometry_source)?;

        path_animation.InsertKeyFrame(t, &frame_composition_path)?;
    }

    // Create a scoped batch to track animation completion
    let batch = compositor.CreateScopedBatch(CompositionBatchTypes::Animation)?;

    // Start the path animation
    path_geometry.StartAnimation(windows::core::h!("Path"), &path_animation)?;

    batch.End()?;

    // Set up completion handler to remove the shape when animation ends
    let shapes = visual.Shapes()?;
    let shape_clone = shape.clone();
    batch.Completed(&TypedEventHandler::new(move |_batch, _args| {
        if let Ok(count) = shapes.Size() {
            for i in 0..count {
                if let Ok(s) = shapes.GetAt(i) {
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

    // Store animation entry
    state.animations.push_back(AnimationEntry { shape, batch });

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
