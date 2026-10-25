// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

// Release 构建下不弹控制台窗口；debug 构建保留控制台，方便看 panic 与日志。
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    qcl_desktop_lib::run();
}
