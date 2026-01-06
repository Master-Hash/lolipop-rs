use windows::Foundation::TimeSpan;
use windows::UI::Color;
use windows::UI::Composition::*;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::WinRT::Composition::ICompositorDesktopInterop;
use windows::Win32::System::WinRT::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::*;
use windows_numerics::{Vector2, Vector3};

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

// 创建 DispatcherQueueController，返回裸指针以保持其存活
fn create_dispatcher_queue_controller() -> Result<*mut std::ffi::c_void> {
    // DQTYPE_THREAD_CURRENT = 2, DQTAT_COM_STA = 2
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

// 辅助函数：将毫秒转换为 TimeSpan (100纳秒为单位)
fn duration_ms(ms: i64) -> TimeSpan {
    TimeSpan {
        Duration: ms * 10_000,
    }
}

fn main() -> Result<()> {
    unsafe {
        // 初始化 WinRT
        RoInitialize(RO_INIT_SINGLETHREADED)?;

        // 创建窗口类
        let instance: HINSTANCE = GetModuleHandleW(None)?.into();
        let window_class = w!("CompositionAnimationWindow");

        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(wndproc),
            hInstance: instance,
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            hbrBackground: HBRUSH(GetStockObject(BLACK_BRUSH).0),
            lpszClassName: window_class,
            ..Default::default()
        };

        RegisterClassExW(&wc);

        // 创建窗口
        let hwnd = CreateWindowExW(
            WS_EX_NOREDIRECTIONBITMAP, // 使用 Composition 需要这个标志
            window_class,
            w!("Windows Composition 动画演示"),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            800,
            600,
            None,
            None,
            Some(instance),
            None,
        )?;

        // 初始化 DispatcherQueue (Composition API 需要)
        let _controller = create_dispatcher_queue_controller()?;

        // 创建 Compositor 和动画
        create_composition_animation(hwnd)?;

        // 消息循环
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).into() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        Ok(())
    }
}

fn create_composition_animation(hwnd: HWND) -> Result<()> {
    unsafe {
        // 创建 Compositor
        let compositor = Compositor::new()?;

        // 获取桌面互操作接口，创建目标
        let interop: ICompositorDesktopInterop = compositor.cast()?;
        let target = interop.CreateDesktopWindowTarget(hwnd, true)?;

        // 创建根容器
        let root = compositor.CreateContainerVisual()?;
        root.SetRelativeSizeAdjustment(Vector2 { X: 1.0, Y: 1.0 })?;
        target.SetRoot(&root)?;

        // 创建多个彩色方块并添加动画
        let colors = [
            Color {
                A: 255,
                R: 255,
                G: 100,
                B: 100,
            }, // 红色
            Color {
                A: 255,
                R: 100,
                G: 255,
                B: 100,
            }, // 绿色
            Color {
                A: 255,
                R: 100,
                G: 100,
                B: 255,
            }, // 蓝色
            Color {
                A: 255,
                R: 255,
                G: 255,
                B: 100,
            }, // 黄色
            Color {
                A: 255,
                R: 255,
                G: 100,
                B: 255,
            }, // 紫色
        ];

        for (i, color) in colors.iter().enumerate() {
            // 创建带颜色的方块
            let brush = compositor.CreateColorBrushWithColor(*color)?;
            let visual = compositor.CreateSpriteVisual()?;
            visual.SetBrush(&brush)?;
            visual.SetSize(Vector2 { X: 100.0, Y: 100.0 })?;

            // 设置初始位置
            let start_x = 100.0 + (i as f32) * 120.0;
            let start_y = 250.0;
            visual.SetOffset(Vector3 {
                X: start_x,
                Y: start_y,
                Z: 0.0,
            })?;

            // 创建位置动画 - 上下弹跳效果
            let offset_animation = compositor.CreateVector3KeyFrameAnimation()?;
            offset_animation.SetDuration(duration_ms(1500))?;

            // 添加关键帧，创建弹跳效果
            let delay = (i as f32) * 0.1; // 每个方块延迟一点
            offset_animation.InsertKeyFrame(
                0.0,
                Vector3 {
                    X: start_x,
                    Y: start_y,
                    Z: 0.0,
                },
            )?;
            offset_animation.InsertKeyFrame(
                0.25,
                Vector3 {
                    X: start_x,
                    Y: start_y - 150.0,
                    Z: 0.0,
                },
            )?;
            offset_animation.InsertKeyFrame(
                0.5,
                Vector3 {
                    X: start_x,
                    Y: start_y,
                    Z: 0.0,
                },
            )?;
            offset_animation.InsertKeyFrame(
                0.75,
                Vector3 {
                    X: start_x,
                    Y: start_y - 75.0,
                    Z: 0.0,
                },
            )?;
            offset_animation.InsertKeyFrame(
                1.0,
                Vector3 {
                    X: start_x,
                    Y: start_y,
                    Z: 0.0,
                },
            )?;

            // 设置缓动函数
            let easing = compositor.CreateCubicBezierEasingFunction(
                Vector2 { X: 0.42, Y: 0.0 },
                Vector2 { X: 0.58, Y: 1.0 },
            )?;
            offset_animation.InsertKeyFrameWithEasingFunction(
                0.25,
                Vector3 {
                    X: start_x,
                    Y: start_y - 150.0,
                    Z: 0.0,
                },
                &easing,
            )?;

            // 设置动画循环
            offset_animation.SetIterationBehavior(AnimationIterationBehavior::Forever)?;
            offset_animation.SetDelayTime(duration_ms((delay * 200.0) as i64))?;

            // 创建缩放动画
            let scale_animation = compositor.CreateVector3KeyFrameAnimation()?;
            scale_animation.SetDuration(duration_ms(1000))?;
            scale_animation.InsertKeyFrame(
                0.0,
                Vector3 {
                    X: 1.0,
                    Y: 1.0,
                    Z: 1.0,
                },
            )?;
            scale_animation.InsertKeyFrame(
                0.5,
                Vector3 {
                    X: 1.2,
                    Y: 1.2,
                    Z: 1.0,
                },
            )?;
            scale_animation.InsertKeyFrame(
                1.0,
                Vector3 {
                    X: 1.0,
                    Y: 1.0,
                    Z: 1.0,
                },
            )?;
            scale_animation.SetIterationBehavior(AnimationIterationBehavior::Forever)?;
            scale_animation.SetDelayTime(duration_ms((delay * 200.0) as i64))?;

            // 创建旋转动画
            let rotation_animation = compositor.CreateScalarKeyFrameAnimation()?;
            rotation_animation.SetDuration(duration_ms(3000))?;
            rotation_animation.InsertKeyFrame(0.0, 0.0)?;
            rotation_animation.InsertKeyFrame(1.0, 360.0)?;
            rotation_animation.SetIterationBehavior(AnimationIterationBehavior::Forever)?;

            // 设置旋转中心点
            visual.SetCenterPoint(Vector3 {
                X: 50.0,
                Y: 50.0,
                Z: 0.0,
            })?;

            // 启动动画
            visual.StartAnimation(h!("Offset"), &offset_animation)?;
            visual.StartAnimation(h!("Scale"), &scale_animation)?;
            visual.StartAnimation(h!("RotationAngleInDegrees"), &rotation_animation)?;

            // 添加到根容器
            root.Children()?.InsertAtTop(&visual)?;
        }

        // 添加一个标题文字背景
        let title_brush = compositor.CreateColorBrushWithColor(Color {
            A: 200,
            R: 50,
            G: 50,
            B: 50,
        })?;
        let title_visual = compositor.CreateSpriteVisual()?;
        title_visual.SetBrush(&title_brush)?;
        title_visual.SetSize(Vector2 { X: 300.0, Y: 50.0 })?;
        title_visual.SetOffset(Vector3 {
            X: 250.0,
            Y: 50.0,
            Z: 0.0,
        })?;

        // 标题淡入淡出动画
        let opacity_animation = compositor.CreateScalarKeyFrameAnimation()?;
        opacity_animation.SetDuration(duration_ms(2000))?;
        opacity_animation.InsertKeyFrame(0.0, 0.3)?;
        opacity_animation.InsertKeyFrame(0.5, 1.0)?;
        opacity_animation.InsertKeyFrame(1.0, 0.3)?;
        opacity_animation.SetIterationBehavior(AnimationIterationBehavior::Forever)?;
        title_visual.StartAnimation(h!("Opacity"), &opacity_animation)?;

        root.Children()?.InsertAtBottom(&title_visual)?;

        // 保持 target 存活（存储在窗口用户数据中）
        std::mem::forget(target);
        std::mem::forget(compositor);

        Ok(())
    }
}

extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_DESTROY => {
                PostQuitMessage(0);
                LRESULT(0)
            }
            WM_KEYDOWN => {
                // 按 ESC 键关闭窗口
                if wparam.0 as u32 == 0x1B {
                    PostQuitMessage(0);
                }
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}
