# airport-core

桌面客户端的 sing-box 内核 sidecar。它把 sing-box 作为库链接进来，Windows、macOS、Linux 都从这一份源码编译，不下载按平台预编译的官方发行包。

[桌面端 README](../README.md) · [客户端说明](../../docs/clients/客户端说明.md)

## 命令

客户端只用到三条，参数与 sing-box 自带命令行一致：

```sh
airport-core check -c config.json   # 校验配置，失败时把原因写到标准错误并返回 1
airport-core run -c config.json     # 运行；收到中断或终止信号后先归还路由与 DNS 再退出
airport-core version
```

普通代理和测速由 Tauri 进程直接启动它，TUN 模式由特权服务把它安装到服务目录后启动。

## 构建

```sh
python3 ../scripts/prepare-sidecars.py                                  # 本机目标
python3 ../scripts/prepare-sidecars.py --target x86_64-pc-windows-msvc  # 交叉编译
python3 ../scripts/prepare-sidecars.py --test                           # go vet 与单元测试
```

产物写到 `src-tauri/binaries/airport-core-<目标三元组>`，不提交到仓库。构建标签和链接参数集中在脚本里，链接参数取自 sing-box 自己的 `release/LDFLAGS`。不带标签直接执行 `go build` 也能编译，但得到的内核没有流量接口、TUN 协议栈和 QUIC 等能力，会拒绝客户端生成的配置，版本号也显示为 `unknown`，所以请始终用脚本构建。

- **版本**：`go.mod` 固定 sing-box 版本，`go.sum` 校验。升级时改 `go.mod` 里的版本，再对每个目标执行一次带 `GOFLAGS=-mod=mod` 的构建脚本来补全 `go.sum`。
- **不要执行 `go mod tidy`**：`go.mod` 和 `go.sum` 只收录上述构建标签在五个目标上实际用到的依赖，`go mod tidy` 会按全部构建标签重新解析，把没有编入的可选功能的依赖也加进来。
- **编入的能力**：TUN 协议栈、QUIC 类协议（Hysteria、Hysteria2、TUIC）、WireGuard、uTLS、DHCP DNS、本地流量接口。
- **未编入**：naive 出站（要求按平台附带 Chromium 网络库）、Tailscale、ACME、Cloudflare Tunnel、OpenVPN、OpenConnect、USB/IP。需要时在脚本的 `CORE_TAGS` 里加标签。

## 许可

本程序链接了 sing-box（GPL-3.0-or-later）。分发编译产物时，须按 GPL-3.0-or-later 一并提供本目录源码，并保留随包附带的许可证文件。
