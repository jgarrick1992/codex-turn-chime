# Hook 接入

官方 Codex Hook 是默认状态来源。CodexTurnChime 按官方 [Codex Hooks 文档](https://learn.chatgpt.com/docs/hooks)接入 `UserPromptSubmit`、`PermissionRequest` 和 `Stop`。

## 安全安装顺序

1. 定位 `${CODEX_HOME:-~/.codex}/hooks.json`。
2. 现有文件必须是有效 JSON，`hooks` 结构不兼容时拒绝修改。
3. 在内存生成完整新 JSON，并向用户展示前后差异。
4. 等待用户明确确认。
5. 把当前文件复制到应用备份目录。
6. 在同目录写临时文件，flush、sync 后原子替换。
7. 重新读取并核对 JSON 完全一致。
8. 在 Codex 中通过 `/hooks` 审查并信任这份 command Hook 的精确定义。

所有现有 Hook 都会保留。重复安装幂等。卸载只删除带项目精确状态标识的 command handler，不会用旧备份覆盖用户的新配置。

生成的 handler 同时包含 `command` 与 `commandWindows`，超时 1 秒。只有打包的 helper 确实存在时才允许安装。

新的或发生变化的非托管 command Hook 在用户审查并信任其精确定义之前不会执行。安装或修改 Hook 配置后需要重启 Codex。

Helper 只接受不超过 1 MiB 的单个 JSON 对象，要求精确的 `session_id`、`turn_id`、`cwd` 和 `hook_event_name`。额外官方字段可以存在，但不接受字段别名。未知事件不生成状态；任何失败都不影响 Codex 正常运行。
