const WINDOW_DRAG_EXCLUDED_SELECTORS = [
  '[data-window-drag-exclude="true"]',
  "button",
  "input",
  "textarea",
  "select",
  "option",
  "label",
  "a",
  "summary",
  '[role="button"]',
  '[contenteditable="true"]',
  ".panel",
  ".modal-overlay",
  ".modal-card",
  ".dialog-card",
  ".settings-content",
  ".toast-layer",
  ".notice",
  ".scroll-region"
] as const;

export const WINDOW_DRAG_EXCLUDED_SELECTOR = WINDOW_DRAG_EXCLUDED_SELECTORS.join(", ");

export function isWindowDragExcludedTarget(target: EventTarget | null) {
  if (typeof Element === "undefined" || !(target instanceof Element)) {
    return false;
  }
  return target.closest(WINDOW_DRAG_EXCLUDED_SELECTOR) != null;
}
