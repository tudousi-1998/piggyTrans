# 小猪翻译（PiggyTrans）

跨平台（macOS / Windows）的轻量划词翻译桌面应用，基于 **Tauri 2 + React + TypeScript**，支持自定义大模型翻译

## 环境要求

- **Rust**：建议使用 **stable**（项目根目录含 `rust-toolchain.toml`）。若曾使用较旧的 nightly，请先执行 `rustup default stable` 或 `rustup update`。
- **Node.js**：18+（用于前端构建）

## 开发

```bash
cd piggytrans
npm install
npm run tauri dev
```

首次在 macOS 上需为终端 / IDE 授予「辅助功能」权限

## 配置

在菜单栏托盘图标中选择「设置」，填写百度翻译 **应用 ID** 与 **密钥**，保存后立即生效。

默认全局快捷键为 **`CommandOrControl+R`**（mac 上为 ⌘R，Windows 上为 Ctrl+R）。可在设置中改为其它 `global-hotkey` 支持的组合（例如 `Alt+Shift+T`）。

## 构建

```bash
npm run tauri build
```

产物位于 `src-tauri/target/release/`（具体以 Tauri 输出为准）。

## 许可

MIT — 见 [LICENSE](./LICENSE)。
