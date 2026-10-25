// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

//! 打印本机探测到的 Java 安装，用于排查「为什么没选中我想要的 Java」。
//!
//! 运行：`cargo run -p qcl-java --example detect`

fn main() {
    let installations = qcl_java::detect(&[], None);

    if installations.is_empty() {
        println!("未探测到任何 Java 安装");
        return;
    }

    println!("探测到 {} 个 Java 安装：", installations.len());
    for installation in &installations {
        println!(
            "  [{}] {:>3}  {}",
            match installation.source {
                qcl_java::JavaSource::Explicit => "显式",
                qcl_java::JavaSource::JavaHome => "JAVA_HOME",
                qcl_java::JavaSource::Path => "PATH",
                qcl_java::JavaSource::Managed => "自管",
                qcl_java::JavaSource::PlatformDirectory => "平台目录",
            },
            installation.version.major,
            installation.home.display()
        );
    }

    // 26.3 需要 Java 25。
    match qcl_java::select(&installations, 25) {
        Some(selected) => println!(
            "\n为 Java 25 选中的是：{}（{}）",
            selected.version.raw,
            selected.home.display()
        ),
        None => println!("\n没有满足 Java 25 的安装"),
    }
}
