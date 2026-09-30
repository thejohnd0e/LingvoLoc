export function errorDetail(reason: unknown): string {
  if (reason instanceof Error) return reason.message;
  if (typeof reason === 'string') return reason;
  if (reason && typeof reason === 'object') {
    const value = reason as Record<string, unknown>;
    if (typeof value.message === 'string') return value.message;
    if (typeof value.error === 'string') return value.error;
    // Rust `RuntimeError` arrives as `{ "Variant": "text" }` or `{ "Http": { status, detail } }`.
    const keys = Object.keys(value);
    if (keys.length === 1) {
      const inner = value[keys[0]];
      if (typeof inner === 'string') return inner;
      if (inner && typeof inner === 'object') {
        const http = inner as Record<string, unknown>;
        if (
          typeof http.status === 'number' &&
          typeof http.detail === 'string'
        ) {
          return `HTTP ${http.status}: ${http.detail}`;
        }
      }
    }
    try {
      const serialized = JSON.stringify(reason);
      if (serialized && serialized !== '{}') return serialized;
    } catch {
      // Fall through to the generic message for non-serializable errors.
    }
  }
  return String(reason) === '[object Object]'
    ? 'unknown error'
    : String(reason);
}
