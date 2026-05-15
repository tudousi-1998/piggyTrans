import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  overlayUsesSplitLayout,
  translationIsMultiline,
  useOverlayWindowSize,
} from "./overlayWindow";
import type { AppSettings, OverlayOpenPayload, TranslationResult } from "./types";

type ViewState =
  | { kind: "idle" }
  | { kind: "loading"; text: string }
  | { kind: "result"; result: TranslationResult }
  | { kind: "error"; message: string }
  | { kind: "permission" }
  | {
      kind: "manual";
      manualKey: number;
      last: TranslationResult | null;
      err: string | null;
      busy: boolean;
    };

function fontVars(size: number): React.CSSProperties {
  return { "--pt-font-size": `${size}px` } as React.CSSProperties;
}

export default function OverlayApp() {
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [state, setState] = useState<ViewState>({ kind: "idle" });
  const cardRef = useRef<HTMLDivElement>(null);
  const splitLayout = overlayUsesSplitLayout(state);

  useEffect(() => {
    document.body.classList.add("pt-body--overlay");
    return () => document.body.classList.remove("pt-body--overlay");
  }, []);

  useOverlayWindowSize(splitLayout, cardRef, [state, settings?.ui_body_font_size]);

  useEffect(() => {
    void invoke<{ general: AppSettings }>("load_settings")
      .then((all) => setSettings(all.general))
      .catch(() =>
        setSettings({
          hotkey: "CommandOrControl+R",
          launch_at_login: false,
          translation_provider: "baidu_general",
          ui_body_font_size: 16,
        }),
      );
  }, []);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        void invoke("overlay_hide");
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const runTranslate = useCallback(async (text: string, manualStayOpen: boolean) => {
    const normalized = text.replace(/^\s+/, "").replace(/\s+$/, "");
    if (!normalized) return;
    if (manualStayOpen) {
      setState((s) =>
        s.kind === "manual"
          ? { ...s, busy: true, err: null }
          : {
              kind: "manual",
              manualKey: 0,
              last: null,
              err: null,
              busy: true,
            },
      );
    } else {
      setState({ kind: "loading", text: normalized });
    }
    try {
      const result = await invoke<TranslationResult>("translate", { text: normalized });
      if (manualStayOpen) {
        setState((s) =>
          s.kind === "manual"
            ? { ...s, last: result, err: null, busy: false }
            : {
                kind: "manual",
                manualKey: 0,
                last: result,
                err: null,
                busy: false,
              },
        );
      } else {
        setState({ kind: "result", result });
      }
    } catch (e) {
      const msg = String(e);
      if (manualStayOpen) {
        setState((s) =>
          s.kind === "manual"
            ? { ...s, busy: false, err: msg }
            : {
                kind: "manual",
                manualKey: 0,
                last: null,
                err: msg,
                busy: false,
              },
        );
      } else {
        setState({ kind: "error", message: msg });
      }
    }
  }, []);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void listen<OverlayOpenPayload>("piggy-open", (ev) => {
      const p = ev.payload;
      if (p.mode === "permission") {
        setState({ kind: "permission" });
        return;
      }
      if (p.mode === "manual") {
        setState({
          kind: "manual",
          manualKey: Date.now(),
          last: null,
          err: null,
          busy: false,
        });
        return;
      }
      if (p.mode === "translate" && p.text) {
        void runTranslate(p.text, false);
      }
    }).then((fn) => {
      unlisten = fn;
    });
    return () => {
      unlisten?.();
    };
  }, [runTranslate]);

  const body = settings?.ui_body_font_size ?? 16;

  return (
    <div className="pt-app pt-app--overlay" style={fontVars(body)}>
      <div ref={cardRef} className="pt-overlay-card">
        <header className="pt-overlay-header" data-tauri-drag-region="deep">
          <div className="pt-overlay-drag">
            <h1 className="pt-page-title" style={{ margin: 0 }}>
              小猪翻译
            </h1>
          </div>
          <button
            type="button"
            className="pt-btn-icon pt-overlay-no-drag"
            onClick={() => void invoke("overlay_hide")}
            aria-label="关闭"
          >
            ×
          </button>
        </header>

        {state.kind !== "manual" && (
        <div className="pt-overlay-scroll">
          {state.kind === "idle" && (
            <p className="pt-empty-text">按全局快捷键翻译划选内容；无选区时可手动输入。</p>
          )}

          {state.kind === "loading" && (
            <TranslationBlock original={state.text} translated={null} loading />
          )}

          {state.kind === "result" && (
            <TranslationBlock
              original={state.result.original_text}
              translated={state.result.translated_text}
            />
          )}

          {state.kind === "error" && (
            <p className="pt-alert pt-alert-error" style={{ margin: 0 }}>
              {state.message}
            </p>
          )}

          {state.kind === "permission" && (
            <div>
              <p className="pt-alert pt-alert-info" style={{ margin: "0 0 12px" }}>
                需要「辅助功能」权限才能读取划选文字（macOS）。请在系统设置中勾选本应用；若从终端或
                IDE 启动，请为对应宿主应用开启权限。
              </p>
              <div className="pt-btn-group">
                <button
                  type="button"
                  className="pt-btn pt-btn-default pt-overlay-no-drag"
                  onClick={() => void invoke("open_accessibility_settings")}
                >
                  打开辅助功能设置
                </button>
                <button
                  type="button"
                  className="pt-btn pt-btn-primary pt-overlay-no-drag"
                  onClick={() => void invoke("request_ax_trust_prompt")}
                >
                  请求系统授权
                </button>
              </div>
            </div>
          )}

        </div>
        )}

        {state.kind === "manual" && (
          <ManualBlock
            key={state.manualKey}
            busy={state.busy}
            last={state.last}
            err={state.err}
            onSubmit={(t) => void runTranslate(t, true)}
          />
        )}
      </div>
    </div>
  );
}

function TranslationBlock({
  original,
  translated,
  loading = false,
  forceSplit = false,
}: {
  original: string;
  translated: string | null;
  loading?: boolean;
  /** 手动输入模式：始终左右布局 */
  forceSplit?: boolean;
}) {
  const split = forceSplit || translationIsMultiline(original, translated);

  if (split) {
    return (
      <div className="pt-translation pt-translation--split">
        <div className="pt-translation-col">
          <span className="pt-block-label">原文</span>
          <p className="pt-block-content">{original}</p>
        </div>
        <div className="pt-translation-vdivider" aria-hidden="true" />
        <div className="pt-translation-col">
          <span className="pt-block-label">译文</span>
          {loading ? (
            <p className="pt-loading">正在翻译…</p>
          ) : (
            <p className="pt-block-content">{translated}</p>
          )}
        </div>
      </div>
    );
  }

  return (
    <div className="pt-translation pt-translation--stack">
      <span className="pt-block-label">原文</span>
      <p className="pt-block-content">{original}</p>
      <hr className="pt-divider" />
      <span className="pt-block-label">译文</span>
      {loading ? (
        <p className="pt-loading">正在翻译…</p>
      ) : (
        <p className="pt-block-content">{translated}</p>
      )}
    </div>
  );
}

function ManualBlock({
  busy,
  last,
  err,
  onSubmit,
}: {
  busy: boolean;
  last: TranslationResult | null;
  err: string | null;
  onSubmit: (t: string) => void;
}) {
  const [draft, setDraft] = useState("");
  const inputRef = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    const id = requestAnimationFrame(() => inputRef.current?.focus());
    return () => cancelAnimationFrame(id);
  }, []);

  const submit = () => {
    if (busy || !draft.trim()) return;
    onSubmit(draft);
  };

  return (
    <>
      <section className="pt-overlay-manual-input">
        {err && (
          <p className="pt-alert pt-alert-error" style={{ margin: "0 0 12px" }}>
            {err}
          </p>
        )}
        <textarea
          ref={inputRef}
          className="pt-textarea pt-textarea--overlay-manual pt-overlay-no-drag"
          value={draft}
          disabled={busy}
          autoFocus
          onChange={(e) => setDraft(e.target.value)}
          placeholder="输入中文或英文…"
          rows={3}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.shiftKey) {
              e.preventDefault();
              submit();
            }
          }}
        />
      </section>
      <div className="pt-overlay-scroll">
        {busy || last ? (
          <TranslationBlock
            original={busy ? draft : last!.original_text}
            translated={busy ? null : last!.translated_text}
            loading={busy}
            forceSplit
          />
        ) : (
          <p className="pt-empty-text">Enter 翻译，Shift + Enter 换行</p>
        )}
      </div>
    </>
  );
}
