use windows::Foundation::TimeSpan;
use windows::UI::Color;
use windows::UI::Composition::AnimationIterationBehavior;
use windows::UI::Composition::Compositor;
use windows::UI::Composition::Desktop::DesktopWindowTarget;
use windows::Win32::Foundation::HWND;
use windows::Win32::System::WinRT::Composition::ICompositorDesktopInterop;
use windows::core::Interface;
use windows::core::Result as WinResult;
use windows::core::h;
use windows_numerics::Vector2;
use windows_numerics::Vector3;

/// 辅助函数：将毫秒转换为 TimeSpan (100纳秒为单位)
pub fn duration_ms(ms: i64) -> TimeSpan {
    TimeSpan {
        Duration: ms * 10_000,
    }
}

/// 为指定窗口创建 Compositor 和 DesktopWindowTarget
pub fn create_compositor_for_hwnd(
    hwnd: HWND,
    is_topmost: bool,
) -> WinResult<(Compositor, DesktopWindowTarget)> {
    let compositor = Compositor::new()?;
    let interop: ICompositorDesktopInterop = compositor.cast()?;
    let target: DesktopWindowTarget =
        unsafe { interop.CreateDesktopWindowTarget(hwnd, is_topmost)? };
    Ok((compositor, target))
}

/// 播放测试动画 - 彩色弹跳方块
pub fn play_test_animation(hwnd: HWND) -> WinResult<()> {
    let (compositor, target) = create_compositor_for_hwnd(hwnd, false)?;

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
        let brush = compositor.CreateColorBrushWithColor(*color)?;
        let visual = compositor.CreateSpriteVisual()?;
        visual.SetBrush(&brush)?;
        visual.SetSize(Vector2 { X: 80.0, Y: 80.0 })?;

        let start_x = 50.0 + (i as f32) * 100.0;
        let start_y = 200.0;
        visual.SetOffset(Vector3 {
            X: start_x,
            Y: start_y,
            Z: 0.0,
        })?;

        // 创建弹跳动画
        let offset_animation = compositor.CreateVector3KeyFrameAnimation()?;
        offset_animation.SetDuration(duration_ms(1500))?;
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
                Y: start_y - 120.0,
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
                Y: start_y - 60.0,
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
        offset_animation.SetIterationBehavior(AnimationIterationBehavior::Forever)?;
        offset_animation.SetDelayTime(duration_ms((i as i64) * 100))?;

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

        // 创建旋转动画
        let rotation_animation = compositor.CreateScalarKeyFrameAnimation()?;
        rotation_animation.SetDuration(duration_ms(3000))?;
        rotation_animation.InsertKeyFrame(0.0, 0.0)?;
        rotation_animation.InsertKeyFrame(1.0, 360.0)?;
        rotation_animation.SetIterationBehavior(AnimationIterationBehavior::Forever)?;

        visual.SetCenterPoint(Vector3 {
            X: 40.0,
            Y: 40.0,
            Z: 0.0,
        })?;

        visual.StartAnimation(h!("Offset"), &offset_animation)?;
        visual.StartAnimation(h!("Scale"), &scale_animation)?;
        visual.StartAnimation(h!("RotationAngleInDegrees"), &rotation_animation)?;

        root.Children()?.InsertAtTop(&visual)?;
    }

    // 保持 target 和 compositor 存活
    std::mem::forget(target);
    std::mem::forget(compositor);

    Ok(())
}

/// 播放完整演示动画（包含标题）- 用于独立窗口
pub fn play_demo_animation(hwnd: HWND) -> WinResult<()> {
    let (compositor, target) = create_compositor_for_hwnd(hwnd, true)?;

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
        let brush = compositor.CreateColorBrushWithColor(*color)?;
        let visual = compositor.CreateSpriteVisual()?;
        visual.SetBrush(&brush)?;
        visual.SetSize(Vector2 { X: 100.0, Y: 100.0 })?;

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

        let delay = (i as f32) * 0.1;
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

        visual.SetCenterPoint(Vector3 {
            X: 50.0,
            Y: 50.0,
            Z: 0.0,
        })?;

        visual.StartAnimation(h!("Offset"), &offset_animation)?;
        visual.StartAnimation(h!("Scale"), &scale_animation)?;
        visual.StartAnimation(h!("RotationAngleInDegrees"), &rotation_animation)?;

        root.Children()?.InsertAtTop(&visual)?;
    }

    // 添加标题背景
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

    // 保持 target 和 compositor 存活
    std::mem::forget(target);
    std::mem::forget(compositor);

    Ok(())
}
