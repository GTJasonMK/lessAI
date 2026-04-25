import { isDemoRuntime } from "./runtimeMode";

type EventHandler<T> = (event: { payload: T }) => void;

const webHandlers = new Map<string, Set<EventHandler<unknown>>>();

function addWebListener<T>(eventName: string, handler: EventHandler<T>) {
  const existing = webHandlers.get(eventName);
  if (existing) {
    existing.add(handler as EventHandler<unknown>);
  } else {
    webHandlers.set(eventName, new Set([handler as EventHandler<unknown>]));
  }

  return () => {
    const handlers = webHandlers.get(eventName);
    if (!handlers) return;
    handlers.delete(handler as EventHandler<unknown>);
    if (handlers.size === 0) {
      webHandlers.delete(eventName);
    }
  };
}

function emitWebEvent<T>(eventName: string, payload: T) {
  const handlers = webHandlers.get(eventName);
  if (!handlers || handlers.size === 0) {
    return;
  }
  for (const handler of handlers) {
    try {
      (handler as EventHandler<T>)({ payload });
    } catch (error) {
      console.error("[lessai::web-events] handler failed", eventName, error);
    }
  }
}

export async function listenRuntimeEvent<T>(
  eventName: string,
  handler: EventHandler<T>
) {
  if (isDemoRuntime()) {
    return addWebListener(eventName, handler);
  }

  const { listen } = await import("@tauri-apps/api/event");
  return listen<T>(eventName, handler);
}

export async function emitRuntimeEvent<T>(eventName: string, payload: T) {
  if (isDemoRuntime()) {
    emitWebEvent(eventName, payload);
    return;
  }

  const { emit } = await import("@tauri-apps/api/event");
  await emit(eventName, payload);
}
