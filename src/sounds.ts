import enPermissionRequestedVoice from "./assets/sounds/voice-packs/en-US/ai-voice-01/001-permission-requested.wav";
import enRequestUserInputVoice from "./assets/sounds/voice-packs/en-US/ai-voice-01/002-request-user-input.wav";
import enNeedsInputVoice from "./assets/sounds/voice-packs/en-US/ai-voice-01/003-needs-input.wav";
import enTurnStoppedVoice from "./assets/sounds/voice-packs/en-US/ai-voice-01/004-turn-stopped.wav";
import enTaskCompleteVoice from "./assets/sounds/voice-packs/en-US/ai-voice-01/005-task-complete.wav";
import enReadyVoice from "./assets/sounds/voice-packs/en-US/ai-voice-01/006-ready.wav";
import enExplicitFailureVoice from "./assets/sounds/voice-packs/en-US/ai-voice-01/007-explicit-failure.wav";
import enInterruptedVoice from "./assets/sounds/voice-packs/en-US/ai-voice-01/008-interrupted.wav";
import zhPermissionRequestedVoice from "./assets/sounds/voice-packs/zh-CN/ai-voice-01/001-permission-requested.wav";
import zhRequestUserInputVoice from "./assets/sounds/voice-packs/zh-CN/ai-voice-01/002-request-user-input.wav";
import zhNeedsInputVoice from "./assets/sounds/voice-packs/zh-CN/ai-voice-01/003-needs-input.wav";
import zhTurnStoppedVoice from "./assets/sounds/voice-packs/zh-CN/ai-voice-01/004-turn-stopped.wav";
import zhTaskCompleteVoice from "./assets/sounds/voice-packs/zh-CN/ai-voice-01/005-task-complete.wav";
import zhReadyVoice from "./assets/sounds/voice-packs/zh-CN/ai-voice-01/006-ready.wav";
import zhExplicitFailureVoice from "./assets/sounds/voice-packs/zh-CN/ai-voice-01/007-explicit-failure.wav";
import zhInterruptedVoice from "./assets/sounds/voice-packs/zh-CN/ai-voice-01/008-interrupted.wav";
import type { AppSettings, MonitorKind } from "./types";

export const LUMI_VOICE_SCHEME = "builtin:voice:lumi";
export const MAX_SOUND_VOLUME = 2;

export type BuiltInVoiceSchemeId = typeof LUMI_VOICE_SCHEME;

export type VoicePromptKey =
  | "permission_requested"
  | "request_user_input"
  | "needs_input"
  | "turn_stopped"
  | "task_complete"
  | "ready"
  | "explicit_failure"
  | "interrupted";

export interface SoundPlayback {
  finished: Promise<void>;
  stop: () => void;
}

const VOICE_SCHEMES: Record<BuiltInVoiceSchemeId, Record<AppSettings["language"], Record<VoicePromptKey, string>>> = {
  [LUMI_VOICE_SCHEME]: {
    en: {
      permission_requested: enPermissionRequestedVoice,
      request_user_input: enRequestUserInputVoice,
      needs_input: enNeedsInputVoice,
      turn_stopped: enTurnStoppedVoice,
      task_complete: enTaskCompleteVoice,
      ready: enReadyVoice,
      explicit_failure: enExplicitFailureVoice,
      interrupted: enInterruptedVoice,
    },
    "zh-CN": {
      permission_requested: zhPermissionRequestedVoice,
      request_user_input: zhRequestUserInputVoice,
      needs_input: zhNeedsInputVoice,
      turn_stopped: zhTurnStoppedVoice,
      task_complete: zhTaskCompleteVoice,
      ready: zhReadyVoice,
      explicit_failure: zhExplicitFailureVoice,
      interrupted: zhInterruptedVoice,
    },
  },
};

export function isBuiltInVoiceScheme(value: string | null): value is BuiltInVoiceSchemeId {
  return value === LUMI_VOICE_SCHEME;
}

export function resolveVoicePromptKey(
  kind: MonitorKind,
  reason: string | null,
): VoicePromptKey | null {
  if (kind === "needs_input") {
    if (reason === "permission_requested") return "permission_requested";
    if (reason === "request_user_input") return "request_user_input";
    return "needs_input";
  }
  if (kind === "ready") {
    if (reason === "turn_stopped") return "turn_stopped";
    if (reason === "task_complete") return "task_complete";
    return "ready";
  }
  if (kind === "blocked" && reason === "explicit_failure") return "explicit_failure";
  if (kind === "stopped" && reason === "interrupted") return "interrupted";
  return null;
}

export async function startAudioPlayback(
  source: string,
  volume: number,
): Promise<SoundPlayback> {
  const audio = new Audio(source);
  const context = new AudioContext();
  const mediaSource = context.createMediaElementSource(audio);
  const gain = context.createGain();
  gain.gain.value = volume;
  mediaSource.connect(gain);
  if (volume > 1) {
    const compressor = context.createDynamicsCompressor();
    compressor.threshold.value = -3;
    compressor.knee.value = 6;
    compressor.ratio.value = 12;
    compressor.attack.value = 0.003;
    compressor.release.value = 0.25;
    gain.connect(compressor);
    compressor.connect(context.destination);
  } else {
    gain.connect(context.destination);
  }
  let finish: () => void = () => undefined;
  const finished = new Promise<void>((resolve) => {
    let settled = false;
    finish = () => {
      if (settled) return;
      settled = true;
      void context.close();
      resolve();
    };
    audio.addEventListener("ended", finish, { once: true });
    audio.addEventListener("error", finish, { once: true });
  });
  const stop = () => {
    audio.pause();
    audio.currentTime = 0;
    finish();
  };
  try {
    await context.resume();
    await audio.play();
  } catch (cause) {
    finish();
    throw cause;
  }
  return { finished, stop };
}

export async function startVoicePrompt(
  schemeId: BuiltInVoiceSchemeId,
  language: AppSettings["language"],
  kind: MonitorKind,
  reason: string | null,
  volume: number,
): Promise<SoundPlayback | null> {
  const key = resolveVoicePromptKey(kind, reason);
  if (!key) return null;
  const source = VOICE_SCHEMES[schemeId][language][key];
  return startAudioPlayback(source, volume);
}
