// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

fn main() {
    // 微软应用（公共客户端）ID 在编译期注入，仓库里没有这个值。
    //
    // 为什么走 rustc-env 而不是让代码直接读环境变量：`option_env!` 不会因为环境变量
    // 变了就触发重新编译，改了 ID 却拿到旧值会非常难查。`rerun-if-env-changed` +
    // `rustc-env` 让 cargo 知道这层依赖，值一变就重编。
    println!("cargo:rerun-if-env-changed=QCL_MS_CLIENT_ID");
    match std::env::var("QCL_MS_CLIENT_ID") {
        Ok(id) if !id.trim().is_empty() => {
            println!("cargo:rustc-env=QCL_MS_CLIENT_ID={}", id.trim());
        }
        // 不失败：贡献者本地没有这个 ID 也应该能编译、能跑其余功能，
        // 只是微软登录不可用（界面上会说明原因）。
        _ => println!(
            "cargo:warning=未设置 QCL_MS_CLIENT_ID，本次构建不含微软登录能力（其余功能不受影响）"
        ),
    }

    tauri_build::build()
}
