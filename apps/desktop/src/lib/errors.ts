export function errorDetail(reason: unknown): string {
  if (reason instanceof Error) return reason.message;
  if (typeof reason === 'string') return reason;
  if (reason && typeof reason === 'object') {
    const value = reason as Record<string, unknown>;
    if (typeof value.message === 'string') return value.message;
    if (typeof value.error === 'string') return value.error;
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
