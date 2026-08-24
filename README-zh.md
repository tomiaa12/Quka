[English](./README.md)

<p align="center">
  <img src="screenshots/icon.png" width="88" alt="Quka">
</p>

<h1 align="center">Quka</h1>

<p align="center">
  极简的 Windows / macOS 应用启动器。<br>
  双击修饰键，输入几个字，回车启动。
</p>

<p align="center">
  <a href="https://github.com/tomiaa12/Quka/releases/latest"><img alt="Download" src="https://img.shields.io/github/v/release/tomiaa12/Quka?label=download"></a>
  <img alt="Platforms" src="https://img.shields.io/badge/platform-Windows%20%7C%20macOS-4f6ef7">
</p>

Quka 在 macOS 上待在菜单栏，在 Windows 上待在托盘。它只索引本机已经安装的软件，平时不打扰你——方向接近 Listary、Alfred、Raycast，但只做一件事：**搜到应用，然后打开它**。

## 截图

<p align="center">
  <img src="screenshots/search.png" width="640" alt="搜索窗口，显示最近使用的应用">
</p>

<p align="center">
  <img src="screenshots/search-dark.png" width="320" alt="深色主题搜索窗口">
  &nbsp;
  <img src="screenshots/settings.png" width="320" alt="通用设置">
</p>

<p align="center">
  <img src="screenshots/settings-search.png" width="640" alt="搜索设置">
</p>

## 功能

- **双击呼出** — macOS 默认 Command，Windows 默认 Ctrl。设置里可改成 Alt 或另一个修饰键。
- **本地快速搜索** — 输入名字的一部分即可。中文名同时匹配拼音全拼和首字母。
- **最近使用** — 搜索框为空时，先列出最近启动过的应用。
- **使用频率排序** — 按启动次数和最近启动时间提升常用应用。
- **自动发现已装应用** — 扫描电脑里已安装的软件，并把图标缓存在本地。
- **后台运行** — 菜单栏 / 托盘应用。macOS 安装后不占 Dock。
- **浅色 / 深色 / 跟随系统**
- **简体中文 / English** — 默认跟随系统语言，也可在设置里切换
- **开机启动**
- **应用内更新**，来自 GitHub Releases

## 安装

到 [Releases](https://github.com/tomiaa12/Quka/releases/latest) 下载最新安装包。

| 平台 | 安装包 |
| --- | --- |
| Windows | `.exe` 或 `.msi` |
| macOS（Apple Silicon） | `.dmg` |
| macOS（Intel） | `.dmg` |

Windows 上如果没有 WebView2，安装程序会先下载运行时，再在后台启动 Quka。

macOS 上将 `Quka.app` 拖进「应用程序」。第一次用全局快捷键时，请在 **系统设置 → 隐私与安全性 → 辅助功能** 中允许 Quka。

## 使用

1. 连按两下 **Command**（macOS）或 **Ctrl**（Windows）。
2. 输入应用名的一部分，或从最近使用里选。
3. **Enter** 启动，**Esc** 隐藏窗口。

菜单栏 / 托盘图标也可以打开搜索、进入设置、重新扫描或退出。

| 按键 | 作用 |
| --- | --- |
| `↑` `↓` | 移动选中项 |
| `Enter` | 启动 |
| `Esc` | 隐藏 |
