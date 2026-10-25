// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

//! 申请一次设备码并打印给用户看，不轮询、不登录。
//!
//! 用途是验证「我们的请求格式被微软接受」——它会在真实端点上跑通
//! `client_id`、作用域、表单编码与响应解析。排查登录问题时应先跑这个：
//! 如果连设备码都申请不到，问题一定在应用注册或客户端 ID，而不是后续的认证链。
//!
//! ```bash
//! cargo run -p qcl-auth --example device-code -- <client-id>
//! ```

use qcl_auth::request_device_code;
use qcl_net::HttpDownloader;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client_id = std::env::args()
        .nth(1)
        .ok_or("用法：device-code <client-id>")?;

    // 下载器需要一个目录，这里用临时目录：本示例不落盘任何东西。
    let dir = std::env::temp_dir().join("qcl-auth-example");
    std::fs::create_dir_all(&dir)?;
    let downloader = HttpDownloader::new(&dir)?;

    let device = request_device_code(&downloader, &client_id).await?;

    println!("请在浏览器中打开：{}", device.verification_uri);
    println!("输入代码：{}", device.user_code);
    println!(
        "有效期：{} 秒，轮询间隔：{} 秒",
        device.expires_in, device.interval
    );
    println!();
    println!("（本示例只申请设备码，不完成登录）");

    Ok(())
}
