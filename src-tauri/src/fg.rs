use windows::core::PWSTR;
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId};

/// 取当前前台窗口的「标题 — 进程名」。
/// 全程原生 API，微秒级返回；任何一步失败都只是少一段文字，不阻塞任何流程。
pub fn foreground_title() -> String {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0.is_null() {
            return String::new();
        }

        let mut buf = [0u16; 512];
        let len = GetWindowTextW(hwnd, &mut buf);
        let title = if len > 0 {
            String::from_utf16_lossy(&buf[..len as usize])
        } else {
            String::new()
        };

        let mut pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid as *mut u32));

        let proc_name = if pid > 0 {
            process_name(pid).unwrap_or_default()
        } else {
            String::new()
        };

        // 标题和进程名一样时只显示一个，避免「WorkBuddy — WorkBuddy」
        if !title.is_empty() && title.eq_ignore_ascii_case(&proc_name) {
            return title;
        }

        match (title.is_empty(), proc_name.is_empty()) {
            (false, false) => format!("{} — {}", title, proc_name),
            (false, true) => title,
            (true, false) => proc_name,
            (true, true) => String::new(),
        }
    }
}

unsafe fn process_name(pid: u32) -> Option<String> {
    let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
    let mut buf = [0u16; 1024];
    let mut size = buf.len() as u32;
    let res = QueryFullProcessImageNameW(
        handle,
        PROCESS_NAME_WIN32,
        PWSTR(buf.as_mut_ptr()),
        &mut size,
    );
    let _ = CloseHandle(handle);
    res.ok()?;

    let full = String::from_utf16_lossy(&buf[..size as usize]);
    let name = full.rsplit(['\\', '/']).next().unwrap_or(&full).to_string();
    Some(name.replace(".exe", ""))
}
