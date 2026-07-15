# Voice prompt catalog

This document is the stable generation and packaging contract for built-in CodexTurnChime voice packs.

## Stable prompt catalog

Every complete voice pack contains all eight prompts. The numeric sequence, ID, meaning, and spoken text are fixed. Do not rename IDs or provide aliases.

| No. | ID | Event mapping | Simplified Chinese (`zh-CN`) | English (`en-US`) | Filename |
| --- | --- | --- | --- | --- | --- |
| `001` | `permission_requested` | `needs_input + permission_requested` | Codex 需要你的授权。 | Codex needs your approval. | `001-permission-requested.wav` |
| `002` | `request_user_input` | `needs_input + request_user_input` | Codex 正在等待你的回复。 | Codex is waiting for your reply. | `002-request-user-input.wav` |
| `003` | `needs_input` | Other exact `needs_input` events | Codex 需要你的操作。 | Codex needs your attention. | `003-needs-input.wav` |
| `004` | `turn_stopped` | `ready + turn_stopped` | Codex 已完成本轮回复。 | Codex has finished this turn. | `004-turn-stopped.wav` |
| `005` | `task_complete` | `ready + task_complete` | 任务已经完成。 | The task is complete. | `005-task-complete.wav` |
| `006` | `ready` | Other exact `ready` events | Codex 已准备好，请查看结果。 | Codex is ready. Please review the result. | `006-ready.wav` |
| `007` | `explicit_failure` | `blocked + explicit_failure` | 任务执行失败，请检查。 | The task failed. Please review it. | `007-explicit-failure.wav` |
| `008` | `interrupted` | `stopped + interrupted` | 任务已中断。 | The task was interrupted. | `008-interrupted.wav` |

All eight prompts are integrated. Attention settings control `001`–`003` and `007`; outcome settings control `004`–`006` and `008`.

## Batch generation content

Only synthesize the text after the final `|`. Do not speak the sequence number or ID.

### Simplified Chinese

```text
001|permission_requested|Codex 需要你的授权。
002|request_user_input|Codex 正在等待你的回复。
003|needs_input|Codex 需要你的操作。
004|turn_stopped|Codex 已完成本轮回复。
005|task_complete|任务已经完成。
006|ready|Codex 已准备好，请查看结果。
007|explicit_failure|任务执行失败，请检查。
008|interrupted|任务已中断。
```

### English

```text
001|permission_requested|Codex needs your approval.
002|request_user_input|Codex is waiting for your reply.
003|needs_input|Codex needs your attention.
004|turn_stopped|Codex has finished this turn.
005|task_complete|The task is complete.
006|ready|Codex is ready. Please review the result.
007|explicit_failure|The task failed. Please review it.
008|interrupted|The task was interrupted.
```

## Multiple-scheme directory contract

Each voice scheme contains matching Chinese and English packs. The UI language selects the locale automatically; users select the voice scheme only. A pack ID describes only the voice/style and must use lowercase ASCII letters, digits, and hyphens.

```text
src/assets/sounds/voice-packs/
├── zh-CN/
│   ├── calm-female-01/
│   │   ├── 001-permission-requested.wav
│   │   ├── ...
│   │   └── 008-interrupted.wav
│   └── clear-male-01/
│       └── ...
└── en-US/
    ├── calm-female-01/
    │   └── ...
    └── clear-male-01/
        └── ...
```

Suggested pack IDs include `calm-female-01`, `warm-female-01`, `clear-male-01`, and `concise-neutral-01`. The same pack ID may exist under both locales when it represents matching bilingual voices.

The first imported bilingual scheme is named **Lumi**. It uses the application scheme ID `builtin:voice:lumi` and the asset pack ID `ai-voice-01` under both locales.

## Audio delivery requirements

- Format: WAV, signed 16-bit PCM (`pcm_s16le`).
- Sample rate: 24 kHz.
- Channels: mono.
- Target integrated loudness: approximately `-18 LUFS` across one pack.
- True peak: no higher than `-1.5 dBTP`.
- One exact sentence per file; no spoken number, ID, language, or pack name.
- No music, ambient layer, notification chime, reverb tail, or other sound effect.
- Keep volume, speaking pace, and perceived loudness consistent across all eight files in one pack.
- Keep leading and trailing silence at or below 150 ms where the generator permits it.
- Recommended duration: approximately 0.8 to 2.8 seconds per file.

## Intentionally silent events

The following exact events do not need voice files because they are progress signals rather than attention or outcome notifications:

- `running + task_started`
- `running + user_prompt_submitted`
- `running + user_input_received`
- `unknown`
