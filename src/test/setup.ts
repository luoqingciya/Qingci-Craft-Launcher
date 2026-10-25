// 引入 jest-dom 的断言扩展（toBeInTheDocument 等），并同时注册类型。
import "@testing-library/jest-dom/vitest";

import { cleanup } from "@testing-library/react";
import { afterEach } from "vitest";

// Testing Library 只在检测到全局 `afterEach` 时才自动清理。本项目关掉了 globals，
// 所以必须显式注册——否则上一个用例渲染的 DOM 会残留，导致「找到多个元素」这类
// 与代码无关的失败。
afterEach(() => {
  cleanup();
});
