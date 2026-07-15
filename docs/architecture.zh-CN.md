# 架构

## 信任边界

CodexTurnChime 是本地桌面进程。v0.1 没有账号、远程 API、遥测、崩溃上传或网络同步。输入只来自官方 Codex Hook JSON、用户主动开启的本地 transcript JSONL、应用设置和用户明确选择的音频文件。

## 组件

1. `codex-turn-chime-hook` 从 stdin 读取一个 Hook JSON，只映射精确官方事件名，在文件锁保护下追加一个 `MonitorEvent v1`，然后退出。即使监控失败，进程也返回 0，不能阻断 Codex。
2. 队列工作线程只消费完整 JSONL 行，拒绝无效记录，保留末尾半行；SQLite 写入失败时把事件放回队列。
3. SQLite 按 `event_id` 去重，只接受 schema 1，以时间顺序规约并物化当前状态。
4. 可选 watcher 增量读取 `CODEX_HOME/sessions` 的 `.jsonl`，保存文件身份和字节偏移；已识别记录不符合 `codex-jsonl-v1` 时自动停用。
5. Tauri 将接受的事件发送给 React 界面，界面刷新并播放本地声音。WebView 没有任意 shell 执行能力。

## 唯一事件契约

`MonitorEvent v1` 只有 `schema_version`、`event_id`、`source`、`session_id`、`turn_id`、`kind`、`occurred_at`、`cwd` 和 `reason`。内部队列和设置遇到未知字段会拒绝，不存在旧 key、别名或猜测性兼容。

## 状态转换

| 输入 | 状态 |
| --- | --- |
| `UserPromptSubmit`、`task_started` | `running` |
| `PermissionRequest`、未完成 `request_user_input` | `needs_input` |
| 匹配的 `function_call_output` | `running` |
| `Stop`、`task_complete` | `ready` |
| `turn_aborted(reason: interrupted)` | `stopped` |
| 明确失败 | `blocked` |

迟到事件不能覆盖更新状态；同一时间用 `event_id` 决定顺序。全局优先级为 Needs input、Blocked、Ready、Running、Stopped、Unknown。用户中断绝不能成为 Blocked。

## 本地存储

- macOS：`~/Library/Application Support/io.github.jgarrick1992.codexturnchime/`
- Windows：`%APPDATA%\io.github.jgarrick1992.codexturnchime\`

目录包含数据库、设置、事件队列、日志与配置备份。数据库只有 `monitor_events`、`task_states` 和 `watcher_checkpoints`，历史固定保留 30 天。
