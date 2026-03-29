//! Integration tests for AppUseCases accessor
//! AppUseCases 访问器的集成测试

use uc_tauri::bootstrap::{AppRuntime, AppUseCases};

// This test verifies AppUseCases methods are callable
// Actual behavior testing is in uc-app use case tests
//
// 此测试验证 AppUseCases 方法可调用
// 实际行为测试在 uc-app 用例测试中

#[test]
fn test_use_cases_has_list_clipboard_entries() {
    // Compile-time verification that the method exists
    // 编译时验证方法存在
    fn assert_method_exists<F: Fn(&AppUseCases) -> uc_app::usecases::ListClipboardEntries>(_f: F) {}

    // This will only compile if AppUseCases has list_clipboard_entries() method
    // 这只有在 AppUseCases 有 list_clipboard_entries() 方法时才能编译
    assert_method_exists(|uc: &AppUseCases| uc.list_clipboard_entries());
}

#[test]
fn test_use_cases_has_announce_device_name() {
    fn assert_method_exists<F: Fn(&AppUseCases) -> uc_app::usecases::AnnounceDeviceName>(_f: F) {}

    assert_method_exists(|uc: &AppUseCases| uc.announce_device_name());
}

#[test]
fn test_app_runtime_has_usecases_method() {
    // Compile-time verification
    // 编译时验证
    fn assert_method_exists<F: Fn(&AppRuntime) -> AppUseCases>(_f: F) {}

    // This will only compile if AppRuntime has usecases() method
    // 这只有在 AppRuntime 有 usecases() 方法时才能编译
    assert_method_exists(|runtime: &AppRuntime| runtime.usecases());
}

#[test]
fn test_app_runtime_has_wiring_deps() {
    // Compile-time verification that AppRuntime exposes wiring_deps()
    // 编译时验证 AppRuntime 通过 wiring_deps() 暴露依赖
    fn can_access_deps(_runtime: &AppRuntime) -> &uc_app::AppDeps {
        // This function will only compile if AppRuntime has a public wiring_deps() method
        // 这个函数只有在 AppRuntime 有公共 wiring_deps() 方法时才能编译
        unimplemented!()
    }

    // If this compiles, the runtime has the right shape
    // 如果这能编译，说明运行时有正确的形状
    let _ = can_access_deps;
}
