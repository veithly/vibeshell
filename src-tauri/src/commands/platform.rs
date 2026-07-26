use crate::platform::RuntimeCapabilities;

#[tauri::command]
pub fn get_runtime_capabilities() -> RuntimeCapabilities {
    RuntimeCapabilities::current()
}
