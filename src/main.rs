mod common;

use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::WinRT::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::*;

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

        // 使用 common 模块的动画函数
        common::play_demo_animation(hwnd)?;

        // 消息循环
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).into() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

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
