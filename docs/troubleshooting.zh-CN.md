# 故障排查

## 没有收到事件

打开“诊断”，检查 helper、Hook、队列与数据库。预览 Hook 配置，确认三个 handler 都存在。修改 Hook 后重启 Codex。自定义 `CODEX_HOME` 时，CodexTurnChime 必须使用相同环境。

## 找不到 helper

开发/发布打包前必须构建 helper 并执行 `node scripts/stage-sidecar.mjs <target> release`。不要把任意路径或旧 helper 写进 Hook。

## 自定义声音失败

只支持不超过 25 MiB 的可读 WAV/MP3，扩展名必须和文件签名一致。文件移动后需要重新选择；应用会明确报错，不会悄悄替换成其他自定义文件。

## Transcript watcher 自动停用

这是预期的 fail-closed 行为。关闭 watcher，查看诊断信息，只提交应用/Codex 版本，不要附 transcript 正文。新格式需要审查并发布新 adapter，不会自动映射旧 key。

## Gatekeeper 或 SmartScreen 提醒

首个 beta 没有正式证书签名。请核对 SHA-256、SBOM 和 GitHub artifact attestation，不要全局关闭系统安全功能。
