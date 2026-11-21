# ClashNova Desktop

<!-- i18n-nav -->
[English](../../README.md) · **简体中文** · [Deutsch](README.de.md) · [Français](README.fr.md) · [日本語](README.ja.md) · [한국어](README.ko.md) · [Русский](README.ru.md)

> 译文可能滞后于英文主文档。

基于 [sing-box](https://sing-box.sagernet.org/) 的桌面代理客户端,用 Vue 3 和 Tauri 2 编写,支持 macOS、Windows、Linux。导入订阅即可连接——内核由源码编译、随包内置,订阅解析、路由分流和出站连接都在本机完成。

内置的 sing-box 内核由源码编译(GPL-3.0),因此本项目整体以 **GPL-3.0** 发布。

<p align="center">
  <img src="../screenshots/overview-light.jpg" width="760" alt="概览">
</p>

## 功能

- **概览** — 一键连接或断开,实时显示连接状态、当前节点、上下行速率和本次会话用量。订阅用量、出口 IP 及其归属地就地展示(可隐藏,支持手动或定时刷新),并可在常用节点间快速切换。
- **节点** — 按名称搜索、按延迟排序,或用「测速全部」异步测量所有节点;结果陆续返回,可按最低延迟重新排序。可手动选节点,也可交给自动选择(内核的 `urltest` 分组持续挑选最快的)。
- **订阅** — 从 URL 或本地文件导入 Clash、sing-box 或通用(分享链接)订阅;格式转换在本机完成,不经过在线服务。每条订阅都能刷新、复制、移除,服务商提供时还会显示用量与到期。
- **连接与路由** — 可选**系统代理**、**手动代理**或 **TUN**(虚拟网卡,需系统授权)。在**规则**与**全局**路由间切换,并可配置本地端口、局域网访问等。活跃连接列表、内核实时日志和流量统计一应俱全。
- **系统集成** — 系统托盘、开机自启,以及浅色 / 深色 / 跟随系统主题。界面提供简体中文、English、日本語、한국어、Deutsch、Français、Русский。

## 截图

| 节点 | 订阅 | 网络 | 设置 |
| :-: | :-: | :-: | :-: |
| <img src="../screenshots/nodes.jpg" width="200"> | <img src="../screenshots/subscriptions.jpg" width="200"> | <img src="../screenshots/network.jpg" width="200"> | <img src="../screenshots/settings.jpg" width="200"> |

<p align="center">
  <img src="../screenshots/overview-dark.jpg" width="540" alt="概览(深色)">
</p>

## 构建

需要 Node.js 与 pnpm、Rust、Go、Python 3,以及对应平台的 Tauri 系统依赖。

```sh
cp .env.example .env
pnpm install --frozen-lockfile
python3 scripts/prepare-sidecars.py   # 从源码编译 sing-box sidecar
pnpm tauri dev                         # 开发
pnpm tauri build                       # 打包发布版本
```

`prepare-sidecars` 从源码编译内核和订阅转换器,不下载预编译二进制。Tauri 的 dev 与 build 钩子会自动运行它,改动内核源码后无需手动执行。

若不想安装 Rust 或 Go,可以只跑前端:`pnpm dev` 后在浏览器打开。`browserBridge()` 会桩掉原生侧,因此依赖内核的功能——连接、出口 IP、实时速率——仍需在真实桌面版本中验证。

## 配置

构建期选项由 Vite 从 `.env` 文件读取(已被 `.gitignore` 忽略):

```
VITE_DEFAULT_SERVER=     # 可选的后端地址,留空即作为独立订阅客户端运行
VITE_ENABLE_LOGIN=false  # 为 false 时隐藏账号登录界面
```

保持默认时,应用是一个无需服务端的独立订阅客户端。这两个开关是接入自有后端的扩展点。

## 项目结构

```
src/
  App.vue                 外壳:侧边导航与各页面
  composables/useApp.ts   状态中枢(会话 / 订阅 / 节点 / 连接 / 设置)
  pages/                  概览 / 节点 / 订阅 / 连接 / 日志 / 设置 / 帮助
  i18n/                   多语言消息表
  lib/                    连接状态机、订阅解析、偏好、TUN 服务
  styles/app.css          明暗两套设计令牌
src-tauri/                Tauri(Rust)宿主:窗口、托盘,以及内核与 TUN helper 的生命周期
```

实际出站流量由内置的 sing-box 内核转发。

## 许可证

GPL-3.0,见 [LICENSE](../../LICENSE)。本项目链接 GPL-3.0 的 sing-box 内核,分发时须遵循同一许可证并提供相应源码。
