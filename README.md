<div align="center">

<img src="src-tauri/icons/128x128.png" width="96" alt="AgentSwitch">

# AgentSwitch

一个面向多种 AI 编程客户端的本地配置、路由、用量与工具管理桌面应用。

[![Release](https://img.shields.io/github/v/release/qwq202/cc-switch?label=release&color=2563eb)](https://github.com/qwq202/cc-switch/releases/latest)
[![Downloads](https://img.shields.io/github/downloads/qwq202/cc-switch/total?label=downloads)](https://github.com/qwq202/cc-switch/releases)
[![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux-64748b)](https://github.com/qwq202/cc-switch/releases/latest)
[![Tauri](https://img.shields.io/badge/Tauri-2-f97316)](https://tauri.app/)
[![License](https://img.shields.io/github/license/qwq202/cc-switch)](LICENSE)

[下载最新版](https://github.com/qwq202/cc-switch/releases/latest) · [查看更新日志](CHANGELOG.md) · [问题反馈](https://github.com/qwq202/cc-switch/issues) · [上游项目](https://github.com/farion1231/cc-switch)

</div>

> [!IMPORTANT]
> 本仓库是基于 [farion1231/cc-switch](https://github.com/farion1231/cc-switch) 的社区二次开发版本，不是上游作者发布的官方版本。应用继续沿用 **AgentSwitch** 名称和主要配置格式，以保持使用习惯与数据兼容；本分支的安装包、自动更新和问题反馈均由 [qwq202/cc-switch](https://github.com/qwq202/cc-switch) 独立维护。

## 这个二开版本解决什么问题

上游 AgentSwitch 已经提供了成熟的供应商切换、本地代理、MCP、Skills、会话与同步能力。本分支没有重新发明这些基础设施，而是围绕日常高频使用继续打磨：让界面更紧凑，让成本统计更可信，让 OpenCode 配置更自动，也让 Claude Desktop、Codex 和 Grok Build 的安装与升级少一些意外。

当前分支相对上游的主要改善来自 `v3.16.5` 至 `v3.16.8` 的提交记录。

### 更适合长期使用的界面

- 重构设置页为更清晰的全窗口布局，统一设置行、分组、折叠区和分段控件。
- 收紧供应商、代理、用量、Prompt、MCP、Skills 等高密度页面的间距和视觉层级。
- 重做会话管理器，加入固定搜索/筛选侧栏、批量选择和更干净的消息阅读区。
- 让会话列表、消息正文和目录分别滚动，避免滚动内容时整页跟着移动。
- 优化深色模式对比度、悬停状态、弹出层层级和长文本下的控件稳定性。

### 更可靠的用量与成本统计

- 接入本地 [Models.dev](https://models.dev/) 元数据缓存，统一为模型配置和价格计算提供数据。
- 支持按 6 小时、每天或每周刷新模型数据，并保留用户手动设置的价格。
- 修复重定向/路由场景的计费归属，统计实际使用的上游模型，而不是客户端请求时的别名。
- 隐藏没有实际 Token 的重定向别名，减少模型统计中的重复和空记录。
- 支持历史零成本记录回填，并区分新导入记录和已修复记录。
- 完善 Grok Build 会话同步，按每一轮真实模型计费，并处理缓存 Token、日志轮转和重复扫描。

### OpenCode 配置自动化

- 添加模型时自动生成易读名称，例如 `gpt-5.6` → `GPT 5.6`、`deepseek-v4-flash` → `DeepSeek V4 Flash`。
- 根据 Models.dev 数据自动写入上下文、最大输出、模型能力、价格和原生推理配置。
- 切换 OpenAI、Anthropic、Google 等 AI SDK 接口格式时，自动重新生成匹配该接口的模型配置。
- 修复中文拼音输入法组合输入被重复处理的问题。
- 修复 NewAPI 等包含渐变或裁剪路径的 SVG 图标在同一页面重复渲染时变空白的问题。

### Claude Desktop 与本地路由

- 提供 Claude Desktop 自动更新开关，并确保设置在配置重写后仍然保留。
- 停止 Claude Desktop 本地路由前检查其他应用是否仍在使用代理接管，避免误停共享代理。
- 对停止操作增加明确确认，让多应用共用本地路由时更可控。

### 工具安装、升级与发布

- 加入 Grok Build 的安装、版本检测、升级和 Usage 统计。
- 改进 Claude Code 与 Codex 的升级流程，覆盖原生安装器、Homebrew Cask、Node 版本管理器和损坏安装恢复。
- 升级完成后校验实际版本变化，避免上游命令报告成功但程序没有真正更新。
- 使用本仓库自己的 Tauri 更新端点和 GitHub Releases，不会从上游仓库误拉安装包。
- 通过 GitHub Actions 构建 Windows、macOS、Linux x86_64 与 Linux ARM64 安装包。

## 核心能力

### 多客户端统一管理

| 客户端/工具    | 主要能力                                                |
| -------------- | ------------------------------------------------------- |
| Claude Code    | 供应商切换、本地路由、模型映射、用量统计、会话管理      |
| Claude Desktop | 供应商与模型路由、自动更新控制、共享代理保护            |
| Codex          | Responses/Chat 路由、模型目录、推理配置、会话与用量统计 |
| Gemini CLI     | 供应商切换、本地路由、MCP/Skills 同步、用量统计         |
| OpenCode       | 供应商配置、Models.dev 自动配置、会话与用量统计         |
| OpenClaw       | 供应商、工作区文件、MCP 与 Skills 管理                  |
| Hermes         | 供应商、MCP、Skills 和配置管理                          |
| Grok Build     | 安装、升级、版本检测和本地会话用量统计                  |

### 供应商与本地代理

- 可视化管理官方服务、第三方中转和自定义 API。
- 一键切换供应商，并支持从系统托盘快速操作。
- 本地代理支持格式转换、热切换、自动故障转移、熔断和健康检查。
- Claude、Codex、Gemini 可分别启用代理接管，不必把所有客户端绑在同一路由上。
- 支持导入、导出、排序、共享配置片段和 `agentswitch://` Deep Link。

### MCP、Prompt 与 Skills

- 在一个界面管理 Claude、Codex、Gemini、OpenCode、Hermes 等客户端的 MCP Server。
- 管理 `CLAUDE.md`、`AGENTS.md`、`GEMINI.md` 等 Prompt 文件，并提供回填保护。
- 从 GitHub 仓库或 ZIP 安装 Skills，支持自定义仓库、软链接和文件复制。
- 在卸载或覆盖 Skill 前创建本地备份。

### 会话、同步与数据

- 浏览、搜索和恢复多个客户端的本地会话记录。
- 在 Usage 页面查看请求数、输入/输出/缓存 Token、成本趋势和请求明细。
- 支持自定义模型价格，自动数据更新不会覆盖用户手动价格。
- 支持本地目录、WebDAV 和 S3 兼容对象存储同步。
- SQLite、原子写入和自动备份共同降低配置损坏风险。

## 下载与安装

请从本仓库的 [Releases](https://github.com/qwq202/cc-switch/releases/latest) 下载。不要使用上游仓库的安装包覆盖本分支，否则自动更新来源和功能版本可能发生变化。

### Windows

- `AgentSwitch-v{version}-Windows.msi`：安装版。
- `AgentSwitch-v{version}-Windows-Portable.zip`：便携版，解压后直接运行。

### macOS

- `AgentSwitch-v{version}-macOS.dmg`：推荐安装方式。
- `AgentSwitch-v{version}-macOS.zip`：应用压缩包。

支持 macOS 12 及以上版本。当前分支发布的是未进行 Apple Developer 公证的社区构建；首次打开时如被系统拦截，请在“系统设置 → 隐私与安全性”中确认运行。

### Linux

- `AgentSwitch-v{version}-Linux-{arch}.AppImage`
- `AgentSwitch-v{version}-Linux-{arch}.deb`
- `AgentSwitch-v{version}-Linux-{arch}.rpm`

发布流程覆盖 x86_64 和 ARM64。不同版本的具体资产以对应 Release 页面为准。

## 快速开始

1. 安装并启动 AgentSwitch。
2. 在左侧选择需要管理的客户端。
3. 添加官方预设、第三方供应商，或创建自定义配置。
4. 填写 API Key 和 Base URL，检查模型映射后保存。
5. 点击“启用”切换供应商；使用本地路由时，再开启对应客户端的代理接管。
6. 重新启动目标 CLI 或桌面客户端，让新的配置完整生效。

> [!TIP]
> 首次使用可以导入现有客户端配置。切换前仍建议备份重要配置，尤其是已经手工维护了大量自定义字段的 OpenCode、Codex 或 Claude 配置。

## 数据位置与隐私

AgentSwitch 采用本地优先设计。供应商配置、统计数据和备份默认保存在当前用户目录，不会因为使用本应用而自动上传到项目维护者的服务器。

| 数据        | 默认位置                      |
| ----------- | ----------------------------- |
| 主数据库    | `~/.agentswitch/agentswitch.db`   |
| 设备设置    | `~/.agentswitch/settings.json`  |
| 自动备份    | `~/.agentswitch/backups/`       |
| Skills 存储 | `~/.agentswitch/skills/`        |
| Skill 备份  | `~/.agentswitch/skill-backups/` |

使用 WebDAV 或 S3 同步时，数据会上传到你自己配置的远端存储。API Key 属于敏感信息，请妥善保护本地数据库、备份和同步凭据。

## 与上游的关系

- 上游项目：[farion1231/cc-switch](https://github.com/farion1231/cc-switch)
- 本二开项目：[qwq202/cc-switch](https://github.com/qwq202/cc-switch)
- 本分支保留上游 MIT License，并感谢 Jason Young 与所有上游贡献者打下的基础。
- 上游后续的重要修复会根据兼容性评估后同步；本分支不会承诺与上游每个提交实时一致。
- 本分支新增功能或安装包问题，请提交到本仓库的 [Issues](https://github.com/qwq202/cc-switch/issues)，不要让上游维护者承担二开版本的问题。

## 开发

### 环境要求

- Node.js 20+
- pnpm 10+
- Rust 1.85+
- Tauri 2 所需的平台依赖

### 常用命令

```bash
# 安装依赖
pnpm install

# 启动 Tauri 开发模式
pnpm dev

# TypeScript 类型检查
pnpm typecheck

# 格式检查
pnpm format:check

# 前端单元测试
pnpm test:unit

# 构建安装包
pnpm build

# Rust 测试
cd src-tauri && cargo test
```

### 技术栈

- 前端：React、TypeScript、Vite、Tailwind CSS、TanStack Query、shadcn/ui
- 桌面与后端：Tauri 2、Rust、Tokio、Serde
- 数据：SQLite、本地 JSON 配置、原子文件写入
- 测试：Vitest、Testing Library、MSW、Cargo Test

### 代码结构

```text
src/                         React + TypeScript 前端
├── components/              业务组件与 UI
├── hooks/                   前端业务 Hooks
├── lib/api/                 Tauri IPC 封装
└── i18n/                    界面翻译

src-tauri/src/               Rust 后端
├── commands/                Tauri Command 接口层
├── services/                业务服务
├── database/                SQLite 与 DAO
├── proxy/                   本地代理和协议转换
└── session_manager/         会话读取与管理

tests/                       前端测试
assets/                      截图和品牌资源
```

## 贡献

欢迎提交 Issue 和 Pull Request。提交前请至少运行：

```bash
pnpm typecheck
pnpm format:check
pnpm test:unit
cd src-tauri && cargo test
```

涉及供应商路由、用量计费、配置迁移或自动更新的改动，请在 PR 中说明兼容性影响、测试方法和回滚方式。

## License

本项目遵循 [MIT License](LICENSE)。原始版权归上游作者所有，二次开发部分由对应贡献者保留其贡献版权。
