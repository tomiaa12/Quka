# Quka

一个类似 Listary / Alfred / Raycast 的极简跨平台应用启动器。

第一阶段只做一件事：快速搜索并启动 Windows / macOS 上已经安装的软件。

当前进度：**Phase 11 打包**。可构建 Windows `.exe` / `.msi` 与 macOS `.dmg`。启动后进入后台，托盘或 Menu Bar 可呼出搜索。

## 技术栈

- Frontend: Vue 3, TypeScript, Vite, Pinia
- Desktop: Tauri 2, Rust
- Database: SQLite

## 开发环境

- Node.js >= 20
- pnpm >= 9
- Rust stable
- Windows 需要 WebView2 与 MSVC 构建工具
- macOS 需要 Xcode Command Line Tools

## 安装依赖

```bash
pnpm install
```

## 启动项目

前端：

```bash
pnpm dev
```

桌面应用：

```bash
pnpm tauri dev
```

## 开发命令

```bash
pnpm install
pnpm dev
pnpm tauri dev
```

## 构建命令

```bash
pnpm tauri build
```

当前平台会按 `tauri.conf.json` 打出对应安装包：Windows 为 NSIS / MSI，macOS 为 DMG。

### Windows 构建

在 Windows 10/11、目标 `x86_64-pc-windows-msvc`：

```bash
pnpm tauri:build:windows
```

产物：

```text
src-tauri/target/x86_64-pc-windows-msvc/release/bundle/nsis/*.exe
src-tauri/target/x86_64-pc-windows-msvc/release/bundle/msi/*.msi
```

安装后创建开始菜单快捷方式，并自动在后台启动。未预装 WebView2 时，安装程序会下载运行时。

### macOS 构建

Apple Silicon：

```bash
pnpm tauri:build:macos-arm
```

Intel：

```bash
pnpm tauri:build:macos-intel
```

通用二进制（需同时安装两个 Rust target）：

```bash
pnpm tauri:build:macos
```

产物：

```text
src-tauri/target/<triple>/release/bundle/dmg/*.dmg
```

将 `Quka.app` 拖入 Applications。打包后的应用为 Menu Bar 应用（不占 Dock）。

## 项目架构

```text
src/                 Vue 3 UI
src-tauri/src/       Rust 系统能力
  commands/          Tauri Command
  scanner/           应用扫描
  launcher/          应用启动
  shortcut/          全局快捷键
  database/          SQLite
  icon/              图标提取
```

前端只负责 UI 与状态。Windows / macOS 系统能力放在 Rust 中。
