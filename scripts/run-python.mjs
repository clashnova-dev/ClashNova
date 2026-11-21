#!/usr/bin/env node
import { spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import { delimiter, dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { normalizeToolPath } from "./tool-env.mjs";

function withToolBins(env) {
  const next = normalizeToolPath(env);
  const home = process.env.USERPROFILE || process.env.HOME || "";
  const extras = [];

  if (process.platform === "win32") {
    const goDirs = [
      process.env.GOROOT ? join(process.env.GOROOT, "bin") : "",
      join(home, "sdk", "go1.27.4", "go", "bin"),
      join(home, "sdk", "go", "bin"),
      join(home, "go", "bin"),
      "C:\\Program Files\\Go\\bin",
      "C:\\Go\\bin",
    ].filter(Boolean);
    for (const dir of goDirs) {
      if (existsSync(join(dir, "go.exe"))) {
        extras.push(dir);
        break;
      }
    }

    const cargoBin = process.env.CARGO_HOME
      ? join(process.env.CARGO_HOME, "bin")
      : join(home, ".cargo", "bin");
    if (existsSync(join(cargoBin, "rustc.exe")) || existsSync(join(cargoBin, "cargo.exe"))) {
      extras.push(cargoBin);
    }

    const pythonDirs = [
      env.pythonLocation || "",
      "D:\\Program Files\\Python",
      "C:\\Program Files\\Python312",
      "C:\\Program Files\\Python311",
      "C:\\Program Files\\Python310",
      join(home, "AppData", "Local", "Programs", "Python", "Python312"),
      join(home, "AppData", "Local", "Programs", "Python", "Python311"),
    ].filter(Boolean);
    // If Scripts\ is on PATH, also add its parent (where python.exe usually lives).
    for (const part of (next.PATH || "").split(delimiter)) {
      if (/[\\/]Python(?:\d+)?[\\/]Scripts[\\/]?$/i.test(part)) {
        pythonDirs.unshift(dirname(part.replace(/[\\/]$/, "")));
      }
    }
    for (const dir of pythonDirs) {
      if (existsSync(join(dir, "python.exe"))) {
        extras.push(dir);
        break;
      }
    }
  }

  if (extras.length) {
    next.PATH = `${extras.join(delimiter)}${delimiter}${next.PATH || ""}`;
  }
  return next;
}

const args = process.argv.slice(2);
if (!args.length) {
  console.error("usage: node scripts/run-python.mjs <script.py> [args...]");
  process.exit(2);
}

const scriptDir = dirname(fileURLToPath(import.meta.url));
const script = args[0].includes("/") || args[0].includes("\\")
  ? args[0]
  : join(scriptDir, args[0]);
const scriptArgs = args.slice(1);

if (!existsSync(script)) {
  console.error(`python script not found: ${script}`);
  process.exit(1);
}

const candidates = process.platform === "win32"
  ? [
      { cmd: "python", prefix: [], shell: false },
      { cmd: "python3", prefix: [], shell: false },
      { cmd: "py", prefix: ["-3"], shell: true },
    ]
  : [
      { cmd: "python3", prefix: [], shell: false },
      { cmd: "python", prefix: [], shell: false },
    ];

const env = withToolBins(process.env);
for (const { cmd, prefix, shell } of candidates) {
  const probe = spawnSync(cmd, [...prefix, "--version"], {
    encoding: "utf8",
    env,
    shell,
    windowsHide: true,
  });
  if (probe.error || probe.status !== 0) continue;
  const run = spawnSync(cmd, [...prefix, script, ...scriptArgs], {
    stdio: "inherit",
    env,
    shell,
    windowsHide: true,
  });
  process.exit(run.status ?? 1);
}

console.error(
  `未找到可用的 Python 3。请安装后确保 PATH 中有 python / python3（Windows 也可用 py -3）。搜索路径：${env.PATH?.split(delimiter).slice(0, 8).join(delimiter)}…`,
);
process.exit(9009);
