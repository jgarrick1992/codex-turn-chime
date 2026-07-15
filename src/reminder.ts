import type { MonitorKind } from "./types";
import type { SoundPlayback } from "./sounds";

export interface ReminderMessage {
  kind: MonitorKind;
  reason: string | null;
}

export interface ReminderController {
  start: (message: ReminderMessage) => void;
  stop: () => void;
}

export function dismissCurrentReminder(controller: ReminderController | null): void {
  controller?.stop();
}

export function createReminderController(
  play: (kind: MonitorKind, reason: string | null) => Promise<SoundPlayback | null>,
  intervalMilliseconds: () => number,
): ReminderController {
  let generation = 0;
  let timeoutId: number | null = null;
  let playback: SoundPlayback | null = null;

  const shouldRepeat = (message: ReminderMessage) => message.reason !== "permission_requested";

  const stop = () => {
    generation += 1;
    if (timeoutId !== null) window.clearTimeout(timeoutId);
    timeoutId = null;
    playback?.stop();
    playback = null;
  };

  const run = async (message: ReminderMessage, activeGeneration: number) => {
    const nextPlayback = await play(message.kind, message.reason);
    if (activeGeneration !== generation) {
      nextPlayback?.stop();
      return;
    }
    if (!nextPlayback) return;
    playback = nextPlayback;
    await nextPlayback.finished;
    if (activeGeneration !== generation) return;
    playback = null;
    if (!shouldRepeat(message)) return;
    timeoutId = window.setTimeout(() => {
      timeoutId = null;
      void run(message, activeGeneration);
    }, intervalMilliseconds());
  };

  return {
    start: (message) => {
      stop();
      const activeGeneration = generation;
      void run(message, activeGeneration);
    },
    stop,
  };
}
