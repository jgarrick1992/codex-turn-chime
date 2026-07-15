import { Keyboard, RotateCcw, X } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { useI18n } from "../i18n";
import {
  DEFAULT_DISMISS_REMINDER_SHORTCUT,
  isMacPlatform,
  shortcutDisplayTokens,
  shortcutFromKeyboardEvent,
} from "../shortcut";

export function ShortcutRecorder({
  value,
  registered,
  registrationMessage,
  onChange,
}: {
  value: string | null;
  registered: boolean;
  registrationMessage: string | null;
  onChange: (value: string | null) => void;
}) {
  const { t } = useI18n();
  const [recording, setRecording] = useState(false);
  const [recordingError, setRecordingError] = useState(false);
  const isMac = useMemo(isMacPlatform, []);
  const displayTokens = useMemo(
    () => value ? shortcutDisplayTokens(value, isMac) : [],
    [isMac, value],
  );

  useEffect(() => {
    if (!recording) return;
    const record = (event: KeyboardEvent) => {
      event.preventDefault();
      event.stopPropagation();
      if (event.code === "Escape") {
        setRecording(false);
        setRecordingError(false);
        return;
      }
      const shortcut = shortcutFromKeyboardEvent(event, isMac);
      if (!shortcut) {
        if (!["Alt", "Control", "Meta", "Shift"].includes(event.key)) {
          setRecordingError(true);
        }
        return;
      }
      setRecording(false);
      setRecordingError(false);
      onChange(shortcut);
    };
    window.addEventListener("keydown", record, true);
    return () => window.removeEventListener("keydown", record, true);
  }, [isMac, onChange, recording]);

  const status = !value
    ? { className: "health-badge", label: t("shortcutDisabled") }
    : registered
      ? { className: "health-badge ok", label: t("shortcutActive") }
      : { className: "health-badge bad", label: t("shortcutInactive") };

  return (
    <div className="shortcut-setting">
      <div className="setting-icon"><Keyboard /></div>
      <div className="shortcut-copy">
        <div className="setting-heading">
          <div><strong>{t("dismissReminderShortcut")}</strong><p>{t("dismissReminderShortcutHelp")}</p></div>
          <span className={status.className}>{status.label}</span>
        </div>
        <button
          className={recording ? "shortcut-recorder recording" : "shortcut-recorder"}
          type="button"
          aria-label={t("recordShortcut")}
          aria-pressed={recording}
          onClick={() => { setRecording(true); setRecordingError(false); }}
        >
          {recording ? <span className="recording-prompt">{t("pressShortcut")}</span> : value ? (
            <span className={isMac ? "keycap-group mac" : "keycap-group"}>
              {displayTokens.map((token, index) => <span key={`${token}-${index}`}><kbd>{token}</kbd>{!isMac && index < displayTokens.length - 1 && <i>+</i>}</span>)}
            </span>
          ) : <span className="recording-prompt muted-copy">{t("shortcutDisabled")}</span>}
          <small>{recording ? t("escapeToCancel") : t("clickToRecord")}</small>
        </button>
        {(recordingError || registrationMessage) && <p className="shortcut-error" role="alert">{recordingError ? t("shortcutNeedsModifier") : registrationMessage}</p>}
        <div className="shortcut-actions">
          <button className="text-button danger" type="button" disabled={!value} onClick={() => { setRecording(false); onChange(null); }}><X size={14} />{t("clearShortcut")}</button>
          <button className="text-button" type="button" disabled={value === DEFAULT_DISMISS_REMINDER_SHORTCUT} onClick={() => { setRecording(false); onChange(DEFAULT_DISMISS_REMINDER_SHORTCUT); }}><RotateCcw size={14} />{t("restoreDefault")}</button>
        </div>
      </div>
    </div>
  );
}
