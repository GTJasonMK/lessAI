use serde::Deserialize;
use tauri::{AppHandle, Manager};
use tauri_runtime::ResizeDirection;

#[derive(Debug, Clone, Copy, Deserialize)]
pub enum ResizeDirectionPayload {
    East,
    North,
    NorthEast,
    NorthWest,
    South,
    SouthEast,
    SouthWest,
    West,
}

impl From<ResizeDirectionPayload> for ResizeDirection {
    fn from(value: ResizeDirectionPayload) -> Self {
        match value {
            ResizeDirectionPayload::East => Self::East,
            ResizeDirectionPayload::North => Self::North,
            ResizeDirectionPayload::NorthEast => Self::NorthEast,
            ResizeDirectionPayload::NorthWest => Self::NorthWest,
            ResizeDirectionPayload::South => Self::South,
            ResizeDirectionPayload::SouthEast => Self::SouthEast,
            ResizeDirectionPayload::SouthWest => Self::SouthWest,
            ResizeDirectionPayload::West => Self::West,
        }
    }
}

fn main_window(app: &AppHandle) -> Result<tauri::Window, String> {
    Ok(app
        .get_webview_window("main")
        .ok_or_else(|| "主窗口不存在。".to_string())?
        .as_ref()
        .window())
}

#[tauri::command]
pub fn is_main_window_maximized(app: AppHandle) -> Result<bool, String> {
    main_window(&app)?
        .is_maximized()
        .map_err(|error| format!("读取窗口状态失败：{error}"))
}

#[tauri::command]
pub fn minimize_main_window(app: AppHandle) -> Result<(), String> {
    main_window(&app)?
        .minimize()
        .map_err(|error| format!("最小化窗口失败：{error}"))
}

#[tauri::command]
pub fn toggle_maximize_main_window(app: AppHandle) -> Result<(), String> {
    let window = main_window(&app)?;
    if window
        .is_maximized()
        .map_err(|error| format!("读取窗口状态失败：{error}"))?
    {
        window
            .unmaximize()
            .map_err(|error| format!("还原窗口失败：{error}"))
    } else {
        window
            .maximize()
            .map_err(|error| format!("最大化窗口失败：{error}"))
    }
}

#[tauri::command]
pub fn close_main_window(app: AppHandle) -> Result<(), String> {
    main_window(&app)?
        .close()
        .map_err(|error| format!("关闭窗口失败：{error}"))
}

#[tauri::command]
pub fn start_drag_main_window(app: AppHandle) -> Result<(), String> {
    main_window(&app)?
        .start_dragging()
        .map_err(|error| format!("启动窗口拖拽失败：{error}"))
}

#[tauri::command]
pub fn start_resize_main_window(
    app: AppHandle,
    direction: ResizeDirectionPayload,
) -> Result<(), String> {
    main_window(&app)?
        .start_resize_dragging(direction.into())
        .map_err(|error| format!("启动窗口缩放失败：{error}"))
}
