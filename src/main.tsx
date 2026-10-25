// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import App from "./App";
import "./styles.css";

const container = document.getElementById("root");
if (!container) {
  throw new Error("找不到 #root 容器");
}

createRoot(container).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
