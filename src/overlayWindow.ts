import { useLayoutEffect, type RefObject } from "react";
import { LogicalSize } from "@tauri-apps/api/dpi";
import { getCurrentWindow } from "@tauri-apps/api/window";

export const OVERLAY_WIDTH_STACK = 400;
export const OVERLAY_WIDTH_SPLIT = 600;
export const OVERLAY_MIN_HEIGHT = 200;
export const OVERLAY_MAX_HEIGHT = 500;

export function translationIsMultiline(
  original: string,
  translated: string | null,
): boolean {
  if (original.includes("\n")) return true;
  if (translated?.includes("\n")) return true;
  return false;
}

type OverlayViewState =
  | { kind: "idle" }
  | { kind: "loading"; text: string }
  | { kind: "result"; result: { original_text: string; translated_text: string } }
  | { kind: "error" }
  | { kind: "permission" }
  | { kind: "manual" };

export function overlayUsesSplitLayout(state: OverlayViewState): boolean {
  if (state.kind === "manual") return true;
  if (state.kind === "loading") {
    return translationIsMultiline(state.text, null);
  }
  if (state.kind === "result") {
    return translationIsMultiline(
      state.result.original_text,
      state.result.translated_text,
    );
  }
  return false;
}

/** 仅改尺寸，保持窗口左上角位置不变。 */
export async function applyOverlayWindowSize(
  width: number,
  height: number,
): Promise<void> {
  const win = getCurrentWindow();
  const pos = await win.outerPosition();
  await win.setSize(new LogicalSize(width, height));
  await win.setPosition(pos);
}

function measureScrollContent(scroll: HTMLElement): number {
  if (scroll.children.length === 0) {
    return scroll.scrollHeight;
  }
  let total = 0;
  for (const node of scroll.children) {
    if (node instanceof HTMLElement) {
      total += node.offsetHeight;
    }
  }
  return total;
}

/** 按子块累加内容高度，不受 overflow 约束影响。 */
function measureOverlayContentHeight(card: HTMLElement): number {
  const style = getComputedStyle(card);
  const paddingY =
    parseFloat(style.paddingTop) + parseFloat(style.paddingBottom);
  let h = paddingY;

  for (const child of Array.from(card.children)) {
    if (!(child instanceof HTMLElement)) continue;
    if (child.classList.contains("pt-overlay-scroll")) {
      h += measureScrollContent(child);
    } else {
      h += child.offsetHeight;
    }
  }

  return Math.ceil(h);
}

export function useOverlayWindowSize(
  split: boolean,
  measureRef: RefObject<HTMLElement | null>,
  deps: unknown[],
): void {
  useLayoutEffect(() => {
    const card = measureRef.current;
    if (!card) return;

    let cancelled = false;
    let raf = 0;
    let syncing = false;
    let lastWidth = 0;
    let lastHeight = 0;

    const sync = async () => {
      if (syncing || cancelled) return;
      syncing = true;
      try {
        const width = split ? OVERLAY_WIDTH_SPLIT : OVERLAY_WIDTH_STACK;
        const natural = measureOverlayContentHeight(card);
        const height = Math.min(
          OVERLAY_MAX_HEIGHT,
          Math.max(OVERLAY_MIN_HEIGHT, natural),
        );
        const overflow = natural > OVERLAY_MAX_HEIGHT;

        card.classList.toggle("pt-overlay-card--overflow", overflow);
        card.style.height = overflow ? `${OVERLAY_MAX_HEIGHT}px` : "";

        if (width === lastWidth && height === lastHeight) return;
        lastWidth = width;
        lastHeight = height;

        await applyOverlayWindowSize(width, height);
      } finally {
        syncing = false;
      }
    };

    const schedule = () => {
      cancelAnimationFrame(raf);
      raf = requestAnimationFrame(() => {
        void sync();
      });
    };

    schedule();

    const ro = new ResizeObserver(() => {
      if (!syncing) schedule();
    });
    card.querySelectorAll(".pt-overlay-scroll > *").forEach((el) => {
      ro.observe(el);
    });
    card.querySelectorAll(".pt-overlay-manual-input").forEach((el) => {
      ro.observe(el);
    });

    return () => {
      cancelled = true;
      syncing = false;
      cancelAnimationFrame(raf);
      ro.disconnect();
      card.classList.remove("pt-overlay-card--overflow");
      card.style.height = "";
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps -- remeasure when content/layout changes
  }, [split, measureRef, ...deps]);
}
