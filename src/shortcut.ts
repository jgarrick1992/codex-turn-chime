export const DEFAULT_DISMISS_REMINDER_SHORTCUT = "CommandOrControl+Shift+K";

type ShortcutKeyboardEvent = Pick<
  KeyboardEvent,
  "altKey" | "code" | "ctrlKey" | "metaKey" | "shiftKey"
>;

const modifierCodes = new Set([
  "AltLeft",
  "AltRight",
  "ControlLeft",
  "ControlRight",
  "MetaLeft",
  "MetaRight",
  "ShiftLeft",
  "ShiftRight",
]);

function acceleratorKey(code: string): string | null {
  if (modifierCodes.has(code)) return null;
  const letter = /^Key([A-Z])$/.exec(code);
  if (letter) return letter[1];
  const digit = /^Digit([0-9])$/.exec(code);
  if (digit) return digit[1];
  if (/^F(?:[1-9]|1[0-9]|2[0-4])$/.test(code)) return code;
  if (/^Numpad[0-9]$/.test(code)) return code;
  return new Set([
    "ArrowDown",
    "ArrowLeft",
    "ArrowRight",
    "ArrowUp",
    "Backspace",
    "Comma",
    "Delete",
    "End",
    "Enter",
    "Equal",
    "Home",
    "Minus",
    "PageDown",
    "PageUp",
    "Period",
    "Semicolon",
    "Slash",
    "Space",
    "Tab",
  ]).has(code) ? code : null;
}

export function isMacPlatform(): boolean {
  return /Mac|iPhone|iPad|iPod/i.test(navigator.userAgent);
}

export function shortcutFromKeyboardEvent(
  event: ShortcutKeyboardEvent,
  isMac: boolean,
): string | null {
  const modifiers: string[] = [];
  if ((isMac && event.metaKey) || (!isMac && event.ctrlKey)) {
    modifiers.push("CommandOrControl");
  }
  if (isMac && event.ctrlKey) modifiers.push("Control");
  if (!isMac && event.metaKey) modifiers.push("Super");
  if (event.altKey) modifiers.push("Alt");
  if (event.shiftKey) modifiers.push("Shift");
  const key = acceleratorKey(event.code);
  if (modifiers.length === 0 || !key) return null;
  return [...modifiers, key].join("+");
}

export function shortcutDisplayTokens(shortcut: string, isMac: boolean): string[] {
  const macLabels: Record<string, string> = {
    CommandOrControl: "⌘",
    Command: "⌘",
    Control: "⌃",
    Alt: "⌥",
    Shift: "⇧",
    Super: "⌘",
  };
  const otherLabels: Record<string, string> = {
    CommandOrControl: "Ctrl",
    Command: "Cmd",
    Control: "Ctrl",
    Alt: "Alt",
    Shift: "Shift",
    Super: "Win",
  };
  return shortcut.split("+").map((token) => (isMac ? macLabels[token] : otherLabels[token]) || token);
}
