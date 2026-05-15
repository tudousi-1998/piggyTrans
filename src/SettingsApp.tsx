import { useEffect, useRef, useState, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { AllSettings, TranslationProvider } from "./types";

type SettingsTab = "general" | "translation";

const defaultSettings = (): AllSettings => ({
  general: {
    hotkey: "CommandOrControl+R",
    launch_at_login: false,
    translation_provider: "baidu_general",
    ui_body_font_size: 16,
  },
  baidu_general: { app_id: "", app_key: "" },
  baidu_llm: {
    api_url: "https://fanyi-api.baidu.com/ait/api/aiTextTranslate",
    app_id: "",
    api_key: "",
  },
  custom_llm: {
    api_base: "https://api.openai.com/v1",
    api_key: "",
    model: "gpt-4o-mini",
  },
});

const PROVIDERS: {
  id: TranslationProvider;
  title: string;
  desc: string;
}[] = [
  {
    id: "baidu_general",
    title: "百度通用翻译",
    desc: "标准机器翻译，需应用 ID 与密钥",
  },
  {
    id: "baidu_llm",
    title: "百度大模型翻译",
    desc: "AI 文本翻译，需应用 ID 与 API Key",
  },
  {
    id: "llm",
    title: "自定义大模型",
    desc: "OpenAI 兼容接口，如 GPT、DeepSeek 等",
  },
];

const TABS: { id: SettingsTab; label: string }[] = [
  { id: "general", label: "通用" },
  { id: "translation", label: "翻译引擎" },
];

function normalizeProvider(p: string): TranslationProvider {
  if (p === "baidu" || p === "baidu_general") return "baidu_general";
  if (p === "baidu_llm") return "baidu_llm";
  return "llm";
}

function fontVars(size: number): React.CSSProperties {
  return { "--pt-font-size": `${size}px` } as React.CSSProperties;
}

function formatHotkey(hotkey: string): string {
  const isMac = /Mac|iPhone|iPad|iPod/.test(navigator.userAgent);
  const parts = hotkey.split("+").filter(Boolean);
  return parts
    .map((p) => {
      if (p === "CommandOrControl") return isMac ? "⌘" : "Ctrl";
      if (p === "Control") return isMac ? "⌃" : "Ctrl";
      if (p === "Alt" || p === "Option") return isMac ? "⌥" : "Alt";
      if (p === "Shift") return isMac ? "⇧" : "Shift";
      return p;
    })
    .join(isMac ? "" : " + ");
}

function providerConfigured(draft: AllSettings, provider: TranslationProvider): boolean {
  if (provider === "baidu_general") {
    return Boolean(draft.baidu_general.app_id.trim() && draft.baidu_general.app_key.trim());
  }
  if (provider === "baidu_llm") {
    return Boolean(
      draft.baidu_llm.app_id.trim() &&
        draft.baidu_llm.api_key.trim() &&
        draft.baidu_llm.api_url.trim(),
    );
  }
  return Boolean(
    draft.custom_llm.api_base.trim() &&
      draft.custom_llm.api_key.trim() &&
      draft.custom_llm.model.trim(),
  );
}

function Field({
  label,
  hint,
  children,
}: {
  label: string;
  hint?: string;
  children: ReactNode;
}) {
  return (
    <div className="pt-field">
      <span className="pt-field-label">{label}</span>
      {children}
      {hint ? <p className="pt-field-hint">{hint}</p> : null}
    </div>
  );
}

function PasswordInput({
  value,
  onChange,
  placeholder,
}: {
  value: string;
  onChange: (v: string) => void;
  placeholder?: string;
}) {
  const [visible, setVisible] = useState(false);
  return (
    <div className="pt-input-wrap">
      <input
        className="pt-input pt-input--with-action"
        type={visible ? "text" : "password"}
        placeholder={placeholder}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        autoComplete="off"
      />
      <button
        type="button"
        className="pt-input-action"
        onClick={() => setVisible((v) => !v)}
        aria-label={visible ? "隐藏密码" : "显示密码"}
        tabIndex={-1}
      >
        {visible ? "隐藏" : "显示"}
      </button>
    </div>
  );
}

export default function SettingsApp() {
  const [draft, setDraft] = useState<AllSettings>(defaultSettings);
  const [loaded, setLoaded] = useState(false);
  const [tab, setTab] = useState<SettingsTab>("general");
  const [saveState, setSaveState] = useState<"idle" | "saving" | "saved" | "error">("idle");
  const [saveError, setSaveError] = useState<string | null>(null);
  const skipAutoSaveRef = useRef(true);

  useEffect(() => {
    void (async () => {
      try {
        const s = await invoke<AllSettings>("load_settings");
        const merged = { ...defaultSettings(), ...s };
        merged.general = {
          ...defaultSettings().general,
          ...s.general,
          translation_provider: normalizeProvider(
            s.general?.translation_provider ?? "baidu_general",
          ),
        };
        merged.baidu_general = { ...defaultSettings().baidu_general, ...s.baidu_general };
        merged.baidu_llm = { ...defaultSettings().baidu_llm, ...s.baidu_llm };
        merged.custom_llm = { ...defaultSettings().custom_llm, ...s.custom_llm };
        setDraft(merged);
      } catch {
        setDraft(defaultSettings());
      } finally {
        setLoaded(true);
      }
    })();
  }, []);

  useEffect(() => {
    if (!loaded) return;
    if (skipAutoSaveRef.current) {
      skipAutoSaveRef.current = false;
      return;
    }
    setSaveState("saving");
    const timer = window.setTimeout(() => {
      void invoke("save_settings", { settings: draft })
        .then(() => {
          setSaveState("saved");
          setSaveError(null);
          window.setTimeout(() => setSaveState("idle"), 2000);
        })
        .catch((e) => {
          setSaveState("error");
          setSaveError(String(e));
        });
    }, 400);
    return () => window.clearTimeout(timer);
  }, [draft, loaded]);

  const setProvider = (translation_provider: TranslationProvider) => {
    setDraft((d) => ({
      ...d,
      general: { ...d.general, translation_provider },
    }));
  };

  const body = draft.general.ui_body_font_size;
  const provider = draft.general.translation_provider;
  const activeReady = providerConfigured(draft, provider);

  if (!loaded) {
    return (
      <div className="pt-app pt-app--settings" style={fontVars(body)}>
        <p className="pt-empty-text">加载中…</p>
      </div>
    );
  }

  return (
    <div className="pt-app pt-app--settings" style={fontVars(body)}>
      <header className="pt-settings-header">
        <div>
          <h1 className="pt-page-title">小猪翻译</h1>
        </div>
        <SaveBadge state={saveState} error={saveError} />
      </header>

      <div className="pt-settings-body">
        <nav className="pt-settings-nav" aria-label="设置分类">
          {TABS.map((t) => (
            <button
              key={t.id}
              type="button"
              className={`pt-settings-nav-item${tab === t.id ? " pt-settings-nav-item--active" : ""}`}
              onClick={() => setTab(t.id)}
            >
              {t.label}
              {t.id === "translation" && !activeReady ? (
                <span className="pt-settings-nav-dot" title="当前引擎未配置完成" />
              ) : null}
            </button>
          ))}
        </nav>

        <main className="pt-settings-main">
          {tab === "general" && (
            <section className="pt-card pt-card--flat">
              <div className="pt-usage-guide">
                <h3 className="pt-usage-guide-title">使用说明</h3>
                <ol className="pt-usage-guide-list">
                  <li>在「翻译引擎」中配置 API，并选择要使用的翻译服务</li>
                  <li>
                    选中任意文本后，按下全局快捷键{" "}
                    <kbd className="pt-kbd">{formatHotkey(draft.general.hotkey)}</kbd>{" "}
                    即可弹出翻译结果
                  </li>
                  <li>未选中文字时按快捷键，会打开浮窗供手动输入并翻译</li>
                  <li>
                    macOS 首次使用需在「系统设置 → 隐私与安全性 → 辅助功能」中授权本应用
                  </li>
                  <li>应用常驻菜单栏，点击托盘图标可随时打开本设置窗口</li>
                </ol>
              </div>

              <div className="pt-field-divider" />

              <Field
                label="全局快捷键"
                hint="Tauri 格式：CommandOrControl、Alt、Shift + 字母/数字，如 CommandOrControl+Shift+T"
              >
                <input
                  className="pt-input"
                  value={draft.general.hotkey}
                  onChange={(e) =>
                    setDraft((d) => ({
                      ...d,
                      general: { ...d.general, hotkey: e.target.value },
                    }))
                  }
                />
              </Field>

              <div className="pt-field-divider" />

              <label className="pt-switch-row pt-switch-row--card">
                <div>
                  <span className="pt-switch-title">登录时自动启动</span>
                  <span className="pt-switch-desc">开机后可在菜单栏快速使用翻译</span>
                </div>
                <input
                  type="checkbox"
                  checked={draft.general.launch_at_login}
                  onChange={(e) =>
                    setDraft((d) => ({
                      ...d,
                      general: { ...d.general, launch_at_login: e.target.checked },
                    }))
                  }
                />
              </label>
            </section>
          )}

          {tab === "translation" && (
            <div className="pt-translation-panel">
              <aside className="pt-translation-panel-left">
                <div className="pt-provider-grid">
                  {PROVIDERS.map((p) => {
                    const selected = provider === p.id;
                    const ready = providerConfigured(draft, p.id);
                    return (
                      <button
                        key={p.id}
                        type="button"
                        className={`pt-provider-card${selected ? " pt-provider-card--selected" : ""}`}
                        onClick={() => setProvider(p.id)}
                      >
                        <span className="pt-provider-card-title">{p.title}</span>
                        <span className="pt-provider-card-desc">{p.desc}</span>
                        <span className="pt-provider-card-meta">
                          {selected ? "当前使用" : ready ? "已配置" : "未配置"}
                        </span>
                      </button>
                    );
                  })}
                </div>
              </aside>

              <section className="pt-card pt-card--flat pt-translation-panel-right">
                <h2 className="pt-card-title">
                  {PROVIDERS.find((p) => p.id === provider)?.title ?? "引擎配置"}
                </h2>
                {!activeReady ? (
                  <p className="pt-alert pt-alert-info pt-config-warn">
                    请填写下方必填项，保存后即可使用当前引擎翻译。
                  </p>
                ) : null}

                {provider === "baidu_general" && (
                  <BaiduGeneralForm draft={draft} setDraft={setDraft} />
                )}
                {provider === "baidu_llm" && (
                  <BaiduLlmForm draft={draft} setDraft={setDraft} />
                )}
                {provider === "llm" && <CustomLlmForm draft={draft} setDraft={setDraft} />}
              </section>
            </div>
          )}
        </main>
      </div>
    </div>
  );
}

function SaveBadge({
  state,
  error,
}: {
  state: "idle" | "saving" | "saved" | "error";
  error: string | null;
}) {
  if (state === "saving") {
    return <span className="pt-save-badge pt-save-badge--saving">保存中…</span>;
  }
  if (state === "saved") {
    return <span className="pt-save-badge pt-save-badge--saved">已保存</span>;
  }
  if (state === "error" && error) {
    return (
      <span className="pt-save-badge pt-save-badge--error" title={error}>
        保存失败
      </span>
    );
  }
  return null;
}

function BaiduGeneralForm({
  draft,
  setDraft,
}: {
  draft: AllSettings;
  setDraft: React.Dispatch<React.SetStateAction<AllSettings>>;
}) {
  return (
    <>
      <Field label="应用 ID（appid）" hint="在百度翻译开放平台控制台获取">
        <input
          className="pt-input"
          value={draft.baidu_general.app_id}
          onChange={(e) =>
            setDraft((d) => ({
              ...d,
              baidu_general: { ...d.baidu_general, app_id: e.target.value },
            }))
          }
        />
      </Field>
      <Field label="密钥（secret）">
        <PasswordInput
          value={draft.baidu_general.app_key}
          onChange={(v) =>
            setDraft((d) => ({
              ...d,
              baidu_general: { ...d.baidu_general, app_key: v },
            }))
          }
        />
      </Field>
    </>
  );
}

function BaiduLlmForm({
  draft,
  setDraft,
}: {
  draft: AllSettings;
  setDraft: React.Dispatch<React.SetStateAction<AllSettings>>;
}) {
  return (
    <>
      <Field label="API 地址">
        <input
          className="pt-input"
          value={draft.baidu_llm.api_url}
          onChange={(e) =>
            setDraft((d) => ({
              ...d,
              baidu_llm: { ...d.baidu_llm, api_url: e.target.value },
            }))
          }
        />
      </Field>
      <Field label="应用 ID（appid）">
        <input
          className="pt-input"
          value={draft.baidu_llm.app_id}
          onChange={(e) =>
            setDraft((d) => ({
              ...d,
              baidu_llm: { ...d.baidu_llm, app_id: e.target.value },
            }))
          }
        />
      </Field>
      <Field label="API Key">
        <PasswordInput
          value={draft.baidu_llm.api_key}
          onChange={(v) =>
            setDraft((d) => ({
              ...d,
              baidu_llm: { ...d.baidu_llm, api_key: v },
            }))
          }
        />
      </Field>
    </>
  );
}

function CustomLlmForm({
  draft,
  setDraft,
}: {
  draft: AllSettings;
  setDraft: React.Dispatch<React.SetStateAction<AllSettings>>;
}) {
  return (
    <>
      <Field
        label="API Base URL"
      >
        <input
          className="pt-input"
          value={draft.custom_llm.api_base}
          onChange={(e) =>
            setDraft((d) => ({
              ...d,
              custom_llm: { ...d.custom_llm, api_base: e.target.value },
            }))
          }
        />
      </Field>
      <Field label="API Key">
        <PasswordInput
          value={draft.custom_llm.api_key}
          onChange={(v) =>
            setDraft((d) => ({
              ...d,
              custom_llm: { ...d.custom_llm, api_key: v },
            }))
          }
        />
      </Field>
      <Field label="模型名称" hint="如 gpt-4o-mini、deepseek-chat">
        <input
          className="pt-input"
          value={draft.custom_llm.model}
          onChange={(e) =>
            setDraft((d) => ({
              ...d,
              custom_llm: { ...d.custom_llm, model: e.target.value },
            }))
          }
        />
      </Field>
    </>
  );
}
