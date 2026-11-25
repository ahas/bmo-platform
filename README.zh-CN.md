[English](README.md) | [한국어](README.ko.md) | [日本語](README.ja.md) | **简体中文**

# agency

负责供给、投递和控制智能体的服务。其中有两条轴线在运转：一条是拥有房间、与人对话的监督者（supervisor），
另一条是执行工作的角色（role）。调用方在某个路径上请求某个角色的一次运行（run）时，agency 会在该角色的
上限内放行，从角色的提供方顺序中挑出仍有配额的提供方并启动它，随后传递各轮对话、读取智能体的输出，并结束
这次运行。herdr 是会话契约背后的后端之一。

## 文档

- `docs/api.md`：边界契约，涵盖 agency 暴露或调用的所有路由和事件。
- `docs/agents.md`：两条轴线（监督者与角色）、运行，以及触达所有智能体的唯一接口。
- `docs/supervisors.md`：监督者记录包含的内容，以及一个运行器如何承载所有监督者的对话轮次。
- `docs/agent-roles.md`：角色记录包含的内容、策略预置的角色，以及它们的捆绑包和会话。
- `docs/repositories.md`：仓库记录包含的内容、在其中运行时会拿到什么、作业（job）以及固定的模型。
- `docs/job-review.md`：作业结束前，下单方如何分阶段审查作业的报告。
- `docs/provider-order.md`：运行如何获得提供方和模型：顺序、配额、故障转移与等待。
- `docs/role-slots.md`：如何在角色上限内放行运行：队列、预留、跟随监督者的角色，以及常驻的分支检出。
- `docs/sessions.md`：监督者会话与角色会话如何保持、复用、折叠（fold）和替换。
- `docs/keepalive-schedule.md`：何时 ping 监督者的前缀以保持其缓存温热。
- `docs/PERSONAL-DATA.md`：仓库拒绝收录的个人数据，以及守住这一点的检查。
- `docs/` 中的其余文档：向运行中的程序注入话语（`agent-control.md`）、启动命令行、账户及其回退、用量窗口、
  已注册的模型和模型文件、仓库的检出与工作台、工作节点、工具以及运行记录。

## 目录

| Folder | Holds |
|---|---|
| `crates/` | Rust crate |
| `deploy/` | 服务单元 |
| `docs/` | 权威文档 |
| `packages/` | TypeScript 包，同属一个 Bun 工作区 |
| `screen-kit/` | 在个人电脑上构建个人应用界面的工具：与控制台同一版本的 bmo-ui 打包构建 |
| `policy/` | agency 依据的固定值 |
| `prompts/` | agency 组装智能体启动内容所用的提示词 |
| `skills/` | 仓库自带的 worker 技能 |
| `tests/` | 全部测试 |
| `tools/` | agency 提供给工具宿主的工具 |

## 结构

| Path | Holds |
|---|---|
| `crates/agent-session` | 会话契约：启动、发送一轮、读取状态、中断、折叠、结束、列出 |
| `crates/herdr` | herdr 的 socket 客户端 |
| `crates/agent-host` | 放置在窗格（pane）中的各个执行的监管者 |
| `crates/pane-head` | 驻留在窗格内的进程 |
| `crates/harness-machine` | harness 在机器上读取、写入和启动的内容 |
| `crates/harness-providers` | 各提供方就某台机器给出的回答 |
| `crates/node-link` | 机器之间的连接 |
| `crates/llm-gateway` | 已注册模型的前端 |
| `crates/policy` | `policy/` 的读取器 |
| `crates/cache-siren` | 各账户的用量窗口、其读取器，以及缓存寿命的判断 |
| `crates/client` | crate `bmo-agency-client`：agency API 的每个请求体和成员都对应一个 Rust 类型，每条路由对应一个方法 |
| `crates/store` | agency 的数据表、迁移以及作用于其上的语句 |
| `crates/tool-call` | 工具宿主调用协议中 agency 一侧的实现，由 agency 的各个工具共享 |
| `crates/server` | 服务本体：会话、位置、线路、角色槽位、控制路由、动词；其二进制为 `agency-server` 和 `bmo-agency` |
| `tools/` | agency 提供给工具宿主的工具：各项流程与规则手册读取器 |
| `packages/` | TypeScript 包，同属一个 Bun 工作区 |
| `screen-kit/` | 在个人电脑上构建个人应用界面的工具：与控制台同一版本的 bmo-ui 打包构建 |
| `policy/`, `prompts/` | agency 依据的固定值与提示词 |
| `deploy/` | 服务单元：每次部署都会重启的 agency 服务器、智能体宿主和模型网关，以及 agency 部署不负责维持的 herdr 单元 |
| `testbed.toml` | testbed 的声明：如何测试、检查、构建和部署本仓库。部署目标是 agency 自己的部署树 `~/.local/share/bmo-agency`，与它共用的数据根目录 `~/.local/share/bmo/data` 并列 |
| `install.toml` | 安装声明：在新机器上安装 agency 所需的内容，由 `bmo install` 读取 |
| `tests/` | 全部测试 |

## 命令

```sh
cargo check --workspace --all-targets
cargo test --workspace
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
(cd packages && bun install && bunx tsc --noEmit)
(cd tests/packages && bun test)
```

## 许可证

本仓库自身的代码采用 MIT 许可证，全文见根目录下的 `LICENSE`。`cache-siren` crate 派生自一个无法确认其许可证
的上游项目，因此不带任何许可证声明，也不在本仓库 MIT 许可证的覆盖范围内。

## 版本

版本属于整个套件，遵循 semver，在构建时从 git 标签读取；工作区清单中的版本只是永不变动的占位值。
