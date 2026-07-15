import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { I18nProvider } from "../i18n";
import { DEFAULT_DISMISS_REMINDER_SHORTCUT } from "../shortcut";
import { ShortcutRecorder } from "./ShortcutRecorder";

afterEach(cleanup);

function renderRecorder(value: string | null, onChange = vi.fn()) {
  const result = render(
    <I18nProvider>
      <ShortcutRecorder value={value} registered={Boolean(value)} registrationMessage={null} onChange={onChange} />
    </I18nProvider>,
  );
  return { ...result, onChange };
}

describe("ShortcutRecorder", () => {
  it("records a modified non-modifier key", () => {
    const { onChange } = renderRecorder(DEFAULT_DISMISS_REMINDER_SHORTCUT);
    fireEvent.click(screen.getByRole("button", { name: "Record global shortcut" }));
    fireEvent.keyDown(window, { code: "KeyJ", key: "j", ctrlKey: true, shiftKey: true });
    expect(onChange).toHaveBeenCalledWith("CommandOrControl+Shift+J");
  });

  it("does not accept modifier-only input", () => {
    const { onChange } = renderRecorder(DEFAULT_DISMISS_REMINDER_SHORTCUT);
    fireEvent.click(screen.getByRole("button", { name: "Record global shortcut" }));
    fireEvent.keyDown(window, { code: "ShiftLeft", key: "Shift", shiftKey: true });
    expect(onChange).not.toHaveBeenCalled();
  });

  it("clears and restores the default shortcut", () => {
    const first = renderRecorder("CommandOrControl+Alt+J");
    fireEvent.click(screen.getByRole("button", { name: "Clear" }));
    expect(first.onChange).toHaveBeenCalledWith(null);
    first.unmount();

    const second = renderRecorder(null);
    fireEvent.click(screen.getByRole("button", { name: "Restore default" }));
    expect(second.onChange).toHaveBeenCalledWith(DEFAULT_DISMISS_REMINDER_SHORTCUT);
  });
});
