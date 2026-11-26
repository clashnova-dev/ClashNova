export const desktopHelp = [
  {
    id: "webrtc",
    category: "连接与 TUN",
    categoryEn: "Connection & TUN",
    title: "WebRTC 检测显示本地 IP 怎么办？",
    titleEn: "Why does WebRTC show my local public IP?",
    body: "系统代理和手动代理无法接管浏览器直接发出的 UDP。请先断开其它 VPN，在设置选择 TUN，确认服务就绪后重新连接，再关闭检测页并重新打开测试。TUN 中识别出的 STUN 探测优先经所选节点转发，避免命中直连规则；节点不支持 UDP 时不会改走直连。仅安装 TUN 服务不等于已经接管网络。检测未发现 UDP 地址也可能是请求被阻断，不能视为 UDP 连接成功。",
    bodyEn:
      "System and manual proxies do not capture UDP sent directly by browsers. Disconnect other VPNs, select TUN, verify service readiness, reconnect and reopen the test page. Detected STUN traffic takes priority over direct rules and uses the selected node; unsupported UDP does not fall back to direct. Installing the service alone does not activate TUN. No detected UDP address may mean the request was blocked, not that UDP connectivity succeeded.",
  },
  {
    id: "runtime-logs",
    category: "设置与隐私",
    categoryEn: "Settings & privacy",
    title: "运行日志保存在哪里？",
    titleEn: "Where are runtime logs stored?",
    body: "进入日志页会定位到最新记录；向上滚动阅读时暂停自动跟随。运行日志保存在应用日志目录。macOS 位于 ~/Library/Logs/dev.airport.client；Windows 位于本地应用数据目录的 dev.airport.client/logs；Linux 位于应用数据目录的 dev.airport.client/logs。TUN 服务单独保留内核日志，更新客户端后需要修复服务才能启用新版服务日志采集。日志可能包含访问域名和 IP，分享前请检查。",
    bodyEn:
      "The log page opens at the latest record; scrolling up pauses following. Logs are saved in the application log folder. macOS uses ~/Library/Logs/dev.airport.client; Windows and Linux use dev.airport.client/logs in the local application data directory. TUN keeps separate core logs; repair the service after updating the client to enable the new collector. Logs may contain destinations and IPs; review before sharing.",
  },
  {
    id: "start",
    category: "订阅与节点",
    categoryEn: "Subscriptions & nodes",
    title: "怎样开始使用？",
    titleEn: "How do I get connected?",
    body: "在“订阅 → 添加订阅”填写名称和链接，或导入本地配置；启用订阅后进入“节点”选择节点，再到概览连接。导入配置不会自动开通套餐，请先确认订阅有效期及剩余流量。",
    bodyEn:
      "Open Subscriptions → Add subscription and enter a name and URL, or import a local configuration. Activate the subscription, choose a node and connect from Overview. Importing a configuration does not activate a paid plan; check expiry and remaining quota first.",
  },
  {
    id: "refresh",
    category: "订阅与节点",
    categoryEn: "Subscriptions & nodes",
    title: "刷新失败或卡片没有流量信息？",
    titleEn: "Why does refresh fail or usage remain unknown?",
    body: "先检查订阅地址、网络及套餐状态，再使用卡片上的更新按钮。本地文件订阅需重新导入更新文件。服务商未提供流量或到期数据时会显示未提供，不代表额度为零。不要分享包含凭据的订阅链接。",
    bodyEn:
      "Check the subscription URL, network and plan status, then use the refresh button on the card. For local subscriptions, import an updated file. Missing usage or expiry metadata means the provider did not supply it, not a zero quota. Keep subscription URLs private.",
  },
  {
    id: "latency",
    category: "订阅与节点",
    categoryEn: "Subscriptions & nodes",
    title: "节点检测与自动选择怎么用？",
    titleEn: "How do tests and automatic selection work?",
    body: "在节点页检测全部节点，结果异步显示，可按延迟排序。超时可重试；延迟是响应时间，不代表下载带宽。自动选择根据内核定期检测选择候选节点，不保证固定地区或最高下载速度。需要固定出口时请手动选节点。",
    bodyEn:
      "Test nodes on the Nodes page; results appear progressively and can be sorted by latency. Retry timeouts. Latency measures response time, not download bandwidth. Automatic selection uses periodic core tests and does not guarantee a fixed region or the highest throughput. Choose a node manually when you need a fixed exit.",
  },
  {
    id: "mode",
    category: "连接与 TUN",
    categoryEn: "Connection & TUN",
    title: "系统代理、手动代理和 TUN 有什么区别？",
    titleEn: "Which connection mode should I use?",
    body: "系统代理让遵循系统设置的应用使用本地代理；Linux 当前支持 GNOME。手动代理只启动本地端口，需要在目标应用配置设置页显示的地址。TUN 使用虚拟网卡接管流量，需要系统管理员授权。不要同时让多个 VPN 或代理软件接管网络。",
    bodyEn:
      "System proxy routes applications that follow OS proxy settings; Linux currently supports GNOME. Manual proxy starts a local port that you must configure in the target application. TUN routes traffic through a virtual interface and requires administrator authorization. Avoid simultaneous network takeover by multiple VPN or proxy apps.",
  },
  {
    id: "tun",
    category: "连接与 TUN",
    categoryEn: "Connection & TUN",
    title: "怎样安装、修复和卸载 TUN 服务？",
    titleEn: "How do I install, repair or uninstall the TUN service?",
    body: "在设置中选择 TUN 模式，再进入“TUN 服务 → 授权安装”，在操作系统弹窗中确认，看到“已就绪”后再连接。安装服务不会立即接管网络；连接时才创建虚拟网卡。macOS 使用系统 utun，无需单独安装第三方内核驱动。\n服务异常时先断开，再检测或修复。卸载前先断开，点击“卸载服务”并确认系统授权；卸载不删除订阅。取消授权后可重试或使用普通代理。Windows、Linux 的服务及联网效果仍需对应系统实测。",
    bodyEn:
      "Select TUN mode in Settings, then open TUN service → Authorize installation and approve the OS prompt. Wait for Ready before connecting. Installation alone does not take over the network; connecting creates the virtual interface. macOS uses the built-in utun interface without a separate third-party kernel driver.\nDisconnect before checking, repairing or uninstalling the service. Uninstall requires confirmation and OS authorization and keeps your subscriptions. If authorization is canceled, retry or use a regular proxy mode. Windows and Linux service operation and connectivity still require validation on those systems.",
  },
  {
    id: "routes",
    category: "连接与 TUN",
    categoryEn: "Connection & TUN",
    title: "规则模式与全局模式如何选择？",
    titleEn: "What do Rule and Global modes do?",
    body: "当前规则模式让局域网和 .cn 域名直连，其余经所选节点；全局模式让代理接管的请求经所选节点。“全局”不代表系统代理模式能接管所有应用。无法访问时先检查当前模式、节点与其他代理软件。",
    bodyEn:
      "The current Rule mode sends local-network and .cn traffic directly and other traffic through the selected node. Global mode uses the selected node for proxied requests. Global routing does not make System Proxy capture every application. Check mode, node and other proxy apps when a site fails.",
  },
  {
    id: "connections",
    category: "连接与 TUN",
    categoryEn: "Connection & TUN",
    title: "连接页、网速与 IP 信息怎么看？",
    titleEn: "How do I read connections, speed and IP information?",
    body: "连接页查看活跃/已关闭请求、上传下载、速率与链路，可筛选和关闭连接。关闭连接可能打断下载，应用也可能自动重连。统计仅用于本机排查，不是计费账单。\n概览底部显示出口 IP、服务商、位置等；已连接时通过代理查询，未连接时显示直连出口。地理信息是近似结果；缺失时不会自动补造。可隐藏 IP、手动刷新或开启定时刷新。",
    bodyEn:
      "Connections shows active/closed requests, traffic, speeds and routing chains, with filtering and close actions. Closing a connection can interrupt downloads, and applications may reconnect. These local statistics are not billing records.\nThe bottom of Overview shows the exit IP, provider and approximate location. Queries use the proxy while connected and the direct network otherwise. Missing fields remain unknown. You can hide the IP, refresh manually or enable periodic refresh.",
  },
  {
    id: "network",
    category: "设置与隐私",
    categoryEn: "Settings & privacy",
    title: "端口、局域网代理和快捷键怎么设置？",
    titleEn: "How do I configure ports, LAN sharing and shortcuts?",
    body: "在设置中填写 1024–65535 范围内的空闲代理端口并保存。开启局域网代理后，同一网络设备使用本机局域网 IP 和该端口；仅在可信网络启用。\n快捷键需先开启总开关，录入组合并保存；冲突时换一个组合。开机自启只在登录系统后启动客户端，不自动连接。",
    bodyEn:
      "Save an unused proxy port between 1024 and 65535 in Settings. LAN sharing lets devices on the same network use this computer’s LAN IP and that port; enable it only on trusted networks.\nEnable global shortcuts, record key combinations and save. Change conflicting combinations. Start at login launches the application without automatically connecting.",
  },
  {
    id: "notices",
    category: "设置与隐私",
    categoryEn: "Settings & privacy",
    title: "没有系统通知，关闭窗口后还在运行？",
    titleEn: "Why are notifications missing, or the app still running after closing?",
    body: "系统通知由操作系统管理。未看到通知时，请检查系统通知权限与勿扰模式；应用内仍会显示操作提示。\n关闭窗口后客户端保留在托盘，连接会继续。需要停止连接请先断开；彻底退出请使用托盘菜单的退出。重复启动只打开已有窗口。",
    bodyEn:
      "System notifications are managed by your operating system. If no notification appears, check OS permissions and Do Not Disturb. In-app messages remain available.\nClosing the window leaves ClashNova in the tray and keeps the connection running. Disconnect to stop proxying, or quit using the tray menu. Starting ClashNova again opens the existing window.",
  },
  {
    id: "privacy",
    category: "设置与隐私",
    categoryEn: "Settings & privacy",
    title: "缓存保存什么？如何反馈问题？",
    titleEn: "What is saved, and how do I report a problem?",
    body: "订阅配置、节点选择、连接模式、语言、主题、窗口位置和设置会保存在本机；重启后不会自动连接。密码和登录令牌不作为本地缓存保存。订阅配置可能包含密钥，请保护本机账号。\n遇到问题先查看日志，反馈发生时间、节点名称、模式和错误提示。截图或日志需遮盖订阅链接、密码、令牌与个人信息。更新入口位于设置，安装前核对来源、平台、架构及文件摘要。",
    bodyEn:
      "Subscriptions, node selection, connection mode, language, theme, window placement and preferences are stored locally. Restarting does not automatically connect. Passwords and sign-in tokens are not persisted as a local cache. Subscription configurations may contain secrets; protect your OS account.\nCheck Logs and report the time, node name, mode and error. Redact subscription URLs, passwords, tokens and personal data in screenshots or logs. Updates are listed in Settings; verify source, platform, architecture and file checksum before installing.",
  },
];
