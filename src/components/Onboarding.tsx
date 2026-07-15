import { BellRing, Check, ChevronRight, Link2, ShieldCheck, Volume2 } from "lucide-react";
import { useState } from "react";
import { useI18n } from "../i18n";
import type { AppSettings, MonitorKind } from "../types";
import { BrandMark } from "./BrandMark";

export function Onboarding({ settings, hookInstalled, onPreviewHook, onTestSound, onFinish }: { settings: AppSettings; hookInstalled: boolean; onPreviewHook: () => void; onTestSound: (kind: MonitorKind) => void; onFinish: (settings: AppSettings) => void }) {
  const { t } = useI18n();
  const [step, setStep] = useState(0);
  const [launchAtLogin, setLaunchAtLogin] = useState(false);
  const [watcher, setWatcher] = useState(false);
  const labels = ["setupStepIntro", "setupStepHook", "setupStepSounds", "setupStepOptions"] as const;
  return (
    <div className="onboarding-shell">
      <aside><BrandMark /><ol>{labels.map((label, index) => <li key={label} className={index === step ? "active" : index < step ? "done" : ""}><span>{index < step ? <Check size={14} /> : index + 1}</span>{t(label)}</li>)}</ol><p>{t("independentNotice")}</p></aside>
      <main>
        {step === 0 && <section className="onboarding-step hero-step"><div className="hero-mark"><BellRing /></div><span className="eyebrow">Local-first desktop utility</span><h1>{t("onboardingTitle")}</h1><p>{t("onboardingIntro")}</p><div className="trust-note"><ShieldCheck /><span>{t("privacyPromise")}</span></div></section>}
        {step === 1 && <section className="onboarding-step"><div className="hero-mark"><Link2 /></div><span className="eyebrow">Official Hooks</span><h1>{t("setupStepHook")}</h1><p>{t("confirmInstallHelp")}</p><button className="integration-preview" type="button" onClick={onPreviewHook}><div><strong>{t("hookIntegration")}</strong><small>UserPromptSubmit · PermissionRequest · Stop</small></div><span className={hookInstalled ? "health-badge ok" : "health-badge"}>{hookInstalled ? t("alreadyInstalled") : t("previewChanges")}</span><ChevronRight /></button></section>}
        {step === 2 && <section className="onboarding-step"><div className="hero-mark"><Volume2 /></div><span className="eyebrow">Two distinct signals</span><h1>{t("setupStepSounds")}</h1><p>Use separate, adjustable sounds for attention and completion.</p><div className="sound-test-grid"><button type="button" onClick={() => onTestSound("needs_input")}><BellRing /><strong>{t("needsInput")}</strong><span>{t("test")}</span></button><button type="button" onClick={() => onTestSound("ready")}><Check /><strong>{t("ready")}</strong><span>{t("test")}</span></button></div></section>}
        {step === 3 && <section className="onboarding-step"><div className="hero-mark"><ShieldCheck /></div><span className="eyebrow">Opt in explicitly</span><h1>{t("setupStepOptions")}</h1><p>Both background options are off by default.</p><label className="option-check"><input type="checkbox" checked={launchAtLogin} onChange={(event) => setLaunchAtLogin(event.target.checked)} /><div><strong>{t("launchAtLogin")}</strong><small>{t("launchAtLoginHelp")}</small></div></label><label className="option-check"><input type="checkbox" checked={watcher} onChange={(event) => setWatcher(event.target.checked)} /><div><strong>{t("transcriptWatcher")}</strong><small>{t("transcriptWatcherHelp")}</small></div></label></section>}
        <footer><button className="text-button" type="button" onClick={() => onFinish({ ...settings, onboarding_complete: true })}>{t("skipForNow")}</button><button className="primary-button" type="button" onClick={() => step < 3 ? setStep(step + 1) : onFinish({ ...settings, onboarding_complete: true, launch_at_login: launchAtLogin, transcript_watcher_enabled: watcher })}>{step < 3 ? t("continue") : t("finishSetup")}<ChevronRight size={16} /></button></footer>
      </main>
    </div>
  );
}
