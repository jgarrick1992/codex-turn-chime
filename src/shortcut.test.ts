import { describe, expect, it } from "vitest";
import {
  DEFAULT_DISMISS_REMINDER_SHORTCUT,
  shortcutDisplayTokens,
  shortcutFromKeyboardEvent,
} from "./shortcut";

const keyboardEvent = (overrides: Partial<KeyboardEvent>): KeyboardEvent => ({
  altKey: false,
  code: "KeyK",
  ctrlKey: false,
  metaKey: false,
  shiftKey: false,
  ...overrides,
} as KeyboardEvent);

describe("shortcut recorder helpers", () => {
  it("records the default accelerator on macOS and Windows", () => {
    expect(shortcutFromKeyboardEvent(keyboardEvent({ metaKey: true, shiftKey: true }), true)).toBe(DEFAULT_DISMISS_REMINDER_SHORTCUT);
    expect(shortcutFromKeyboardEvent(keyboardEvent({ ctrlKey: true, shiftKey: true }), false)).toBe(DEFAULT_DISMISS_REMINDER_SHORTCUT);
  });

  it("records a new accelerator in Tauri format", () => {
    expect(shortcutFromKeyboardEvent(keyboardEvent({ code: "KeyJ", ctrlKey: true, altKey: true }), false)).toBe("CommandOrControl+Alt+J");
  });

  it("rejects a non-modified key and modifier-only input", () => {
    expect(shortcutFromKeyboardEvent(keyboardEvent({ code: "KeyK" }), true)).toBeNull();
    expect(shortcutFromKeyboardEvent(keyboardEvent({ code: "ShiftLeft", shiftKey: true }), true)).toBeNull();
  });

  it("uses platform-native display labels", () => {
    expect(shortcutDisplayTokens(DEFAULT_DISMISS_REMINDER_SHORTCUT, true)).toEqual(["⌘", "⇧", "K"]);
    expect(shortcutDisplayTokens(DEFAULT_DISMISS_REMINDER_SHORTCUT, false)).toEqual(["Ctrl", "Shift", "K"]);
  });
});
