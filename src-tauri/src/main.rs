// Release 构建下不弹控制台窗口；debug 构建保留控制台，方便看 panic 与日志。
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    qcl_desktop_lib::run();
}
