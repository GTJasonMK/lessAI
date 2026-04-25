import { infoRuntime } from "../../lib/runtimeLog";

const SCROLL_RESTORE_PREFIX = "[lessai::scroll_restore]";

export function snapshotScrollNode(node: HTMLDivElement | null) {
  if (!node) {
    return { present: false } as const;
  }

  return {
    present: true,
    scrollTop: node.scrollTop,
    scrollHeight: node.scrollHeight,
    clientHeight: node.clientHeight,
    connected: node.isConnected
  } as const;
}

export function logScrollRestore(event: string, detail: Record<string, unknown>) {
  void infoRuntime(`${SCROLL_RESTORE_PREFIX} ${event} ${JSON.stringify(detail)}`);
}
