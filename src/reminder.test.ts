import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createReminderController, dismissCurrentReminder } from "./reminder";
import type { SoundPlayback } from "./sounds";

function controllablePlayback() {
  let finish: () => void = () => undefined;
  const playback: SoundPlayback = {
    finished: new Promise<void>((resolve) => { finish = resolve; }),
    stop: vi.fn(),
  };
  return { finish, playback };
}

describe("createReminderController", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("waits for playback to finish and then applies the configured interval", async () => {
    const first = controllablePlayback();
    const second = controllablePlayback();
    const play = vi.fn()
      .mockResolvedValueOnce(first.playback)
      .mockResolvedValueOnce(second.playback);
    const controller = createReminderController(play, () => 3000);

    controller.start({ kind: "needs_input", reason: "request_user_input" });
    expect(play).toHaveBeenCalledTimes(1);
    first.finish();
    await Promise.resolve();
    await Promise.resolve();

    await vi.advanceTimersByTimeAsync(2999);
    expect(play).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(1);
    expect(play).toHaveBeenCalledTimes(2);
  });

  it("plays permission requests once because hooks do not report automatic approval completion", async () => {
    const current = controllablePlayback();
    const play = vi.fn().mockResolvedValue(current.playback);
    const controller = createReminderController(play, () => 3000);

    controller.start({ kind: "needs_input", reason: "permission_requested" });
    current.finish();
    await Promise.resolve();
    await Promise.resolve();
    await vi.runAllTimersAsync();

    expect(play).toHaveBeenCalledTimes(1);
  });

  it("stops the current playback and cancels future reminders", async () => {
    const current = controllablePlayback();
    const play = vi.fn().mockResolvedValue(current.playback);
    const controller = createReminderController(play, () => 3000);

    controller.start({ kind: "ready", reason: "turn_stopped" });
    await Promise.resolve();
    controller.stop();
    expect(current.playback.stop).toHaveBeenCalledTimes(1);

    current.finish();
    await vi.runAllTimersAsync();
    expect(play).toHaveBeenCalledTimes(1);
  });

  it("replaces an older message instead of running both", async () => {
    const first = controllablePlayback();
    const second = controllablePlayback();
    const play = vi.fn()
      .mockResolvedValueOnce(first.playback)
      .mockResolvedValueOnce(second.playback);
    const controller = createReminderController(play, () => 3000);

    controller.start({ kind: "needs_input", reason: "request_user_input" });
    await Promise.resolve();
    controller.start({ kind: "ready", reason: "task_complete" });
    await Promise.resolve();

    expect(first.playback.stop).toHaveBeenCalledTimes(1);
    expect(play).toHaveBeenLastCalledWith("ready", "task_complete");
  });

  it("dismisses only the current loop and allows the next event to alert", async () => {
    const first = controllablePlayback();
    const second = controllablePlayback();
    const play = vi.fn()
      .mockResolvedValueOnce(first.playback)
      .mockResolvedValueOnce(second.playback);
    const controller = createReminderController(play, () => 3000);

    controller.start({ kind: "needs_input", reason: "request_user_input" });
    await Promise.resolve();
    dismissCurrentReminder(controller);
    first.finish();
    await vi.runAllTimersAsync();

    expect(first.playback.stop).toHaveBeenCalledTimes(1);
    expect(play).toHaveBeenCalledTimes(1);

    controller.start({ kind: "ready", reason: "task_complete" });
    await Promise.resolve();
    expect(play).toHaveBeenCalledTimes(2);
    expect(play).toHaveBeenLastCalledWith("ready", "task_complete");
  });
});
