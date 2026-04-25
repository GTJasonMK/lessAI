export function normalizeNetworkProxy(rawProxy: string) {
  const proxy = rawProxy.trim();
  if (!proxy) return undefined;
  return proxy.includes("://") ? proxy : `http://${proxy}`;
}

