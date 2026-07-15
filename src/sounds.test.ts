import { describe, expect, it } from "vitest";
import { isBuiltInVoiceScheme, LUMI_VOICE_SCHEME, resolveVoicePromptKey } from "./sounds";

describe("resolveVoicePromptKey", () => {
  it("uses exact needs-input reasons", () => {
    expect(resolveVoicePromptKey("needs_input", "permission_requested")).toBe("permission_requested");
    expect(resolveVoicePromptKey("needs_input", "request_user_input")).toBe("request_user_input");
    expect(resolveVoicePromptKey("needs_input", "other")).toBe("needs_input");
  });

  it("uses exact ready reasons", () => {
    expect(resolveVoicePromptKey("ready", "turn_stopped")).toBe("turn_stopped");
    expect(resolveVoicePromptKey("ready", "task_complete")).toBe("task_complete");
    expect(resolveVoicePromptKey("ready", null)).toBe("ready");
  });

  it("does not add voice prompts to unconfigured states", () => {
    expect(resolveVoicePromptKey("running", "user_prompt_submitted")).toBeNull();
    expect(resolveVoicePromptKey("unknown", null)).toBeNull();
  });

  it("uses exact stopped and blocked reasons", () => {
    expect(resolveVoicePromptKey("stopped", "interrupted")).toBe("interrupted");
    expect(resolveVoicePromptKey("blocked", "explicit_failure")).toBe("explicit_failure");
    expect(resolveVoicePromptKey("blocked", "other")).toBeNull();
  });

  it("accepts only the registered bilingual voice scheme ID", () => {
    expect(isBuiltInVoiceScheme(LUMI_VOICE_SCHEME)).toBe(true);
    expect(isBuiltInVoiceScheme("builtin:voice:zh-CN:ai-voice-01")).toBe(false);
    expect(isBuiltInVoiceScheme("builtin:voice:en-US:ai-voice-01")).toBe(false);
    expect(isBuiltInVoiceScheme(null)).toBe(false);
  });
});
