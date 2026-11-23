<div align="center">

<img src="../../app-icon.png" width="116" alt="ClashNova" />

<h1>ClashNova Desktop</h1>

<p>
  <b>基于 <a href="https://sing-box.sagernet.org/">sing-box</a> 的本地优先代理客户端。</b><br/>
  导入订阅即可连接 —— 解析、分流和出站连接都在本机完成。
</p>

<p>
  <a href="../../LICENSE"><img alt="License" src="https://img.shields.io/badge/License-GPL--3.0-2f6fed?style=flat-square"></a>
  <img alt="Platforms" src="https://img.shields.io/badge/macOS%20·%20Windows%20·%20Linux-5b6472?style=flat-square">
  <img alt="Vue 3" src="https://img.shields.io/badge/Vue_3-42b883?style=flat-square&logo=vuedotjs&logoColor=white">
  <img alt="Tauri 2" src="https://img.shields.io/badge/Tauri_2-24c8db?style=flat-square&logo=tauri&logoColor=white">
  <img alt="sing-box core" src="https://img.shields.io/badge/core-sing--box-7d56f4?style=flat-square">
</p>

<p>
  <a href="../../README.md">English</a> ·
  <b>简体中文</b> ·
  <a href="README.de.md">Deutsch</a> ·
  <a href="README.fr.md">Français</a> ·
  <a href="README.ja.md">日本語</a> ·
  <a href="README.ko.md">한국어</a> ·
  <a href="README.ru.md">Русский</a>
</p>

<img src="../screenshots/overview-light.jpg" width="840" alt="ClashNova Desktop" />

<p><a href="https://github.com/clashnova-dev">clashnova-dev</a></p>

</div>

> 译文可能滞后于英文主文档。
## ✨ 功能

- **概览** — 一键连接或断开,实时显示连接状态、当前节点、上下行速率和本次会话用量。订阅用量、出口 IP 及其归属地就地展示(可隐藏,支持手动或定时刷新),并可在常用节点间快速切换。
- **节点** — 按名称搜索、按延迟排序,或用「测速全部」异步测量所有节点;结果陆续返回,可按最低延迟重新排序。可手动选节点,也可交给自动选择(内核的 `urltest` 分组持续挑选最快的)。
- **订阅** — 从 URL 或本地文件导入 Clash、sing-box 或通用(分享链接)订阅;格式转换在本机完成,不经过在线服务。每条订阅都能刷新、复制、移除,服务商提供时还会显示用量与到期。
- **连接与路由** — 可选**系统代理**、**手动代理**或 **TUN**(虚拟网卡,需系统授权)。在**规则**与**全局**路由间切换,并可配置本地端口、局域网访问等。活跃连接列表、内核实时日志和流量统计一应俱全。
- **系统集成** — 系统托盘、开机自启,以及浅色 / 深色 / 跟随系统主题。界面提供简体中文、English、日本語、한국어、Deutsch、Français、Русский。

## 📸 截图

| 概览 | 节点 | 订阅 |
| :-: | :-: | :-: |
| <img src="../screenshots/overview-dark.jpg" width="240"> | <img src="../screenshots/nodes.jpg" width="240"> | <img src="../screenshots/subscriptions.jpg" width="240"> |

| 连接 | 日志 | 设置 |
| :-: | :-: | :-: |
| <img src="../screenshots/connections.jpg" width="240"> | <img src="../screenshots/logs.jpg" width="240"> | <img src="../screenshots/settings.jpg" width="240"> |

**网络**页集中了连接防护策略、健康检查和流媒体 / AI 解锁检测:

<p align="center">
  <img src="../screenshots/network-full.jpg" width="680" alt="网络">
</p>

## 🛠️ 构建

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

## 🍎 在 macOS 上运行

发布版未做代码签名与公证(没有 Apple 开发者证书),下载后 macOS 可能拒绝打开(提示「ClashNova 已损坏」或开发者无法验证)。先解除隔离标记再正常打开,若安装在其它位置请相应修改路径:

```sh
sudo xattr -rd com.apple.quarantine "/Applications/ClashNova.app"
```

## ⚙️ 配置

规则模式仅内置局域网直连和 500 个国内常用服务及必要静态资源/CDN 域名，其余默认走所选代理，订阅规则和应用策略仍可单独指定去向。域名包含子域名，名单为人工维护的精简集合，不是实时流量排名；不再携带全国域名和 IP 段大表。TUN DNS 使用同一份直连域名名单，全局模式不应用这些内置分流。可在**规则**页面查看生成结果。

构建期选项由 Vite 从 `.env` 文件读取(已被 `.gitignore` 忽略):

```
VITE_DEFAULT_SERVER=     # 可选的后端地址,留空即作为独立订阅客户端运行
VITE_ENABLE_LOGIN=false  # 为 false 时隐藏账号登录界面
```

保持默认时,应用是一个无需服务端的独立订阅客户端。这两个开关是接入自有后端的扩展点。

## 🗂️ 项目结构

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

## 📄 许可证

GPL-3.0,见 [LICENSE](../../LICENSE)。本项目链接 GPL-3.0 的 sing-box 内核,分发时须遵循同一许可证并提供相应源码。
