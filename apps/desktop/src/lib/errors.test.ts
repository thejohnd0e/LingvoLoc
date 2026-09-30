import { describe, expect, it } from 'vitest';
import { errorDetail } from './errors';

describe('errorDetail', () => {
  it('unwraps serialized Rust runtime errors', () => {
    expect(errorDetail({ InvalidInput: 'choose another language' })).toBe(
      'choose another language',
    );
    expect(errorDetail({ Http: { status: 503, detail: 'busy' } })).toBe(
      'HTTP 503: busy',
    );
  });

  it('keeps plain messages', () => {
    expect(errorDetail(new Error('boom'))).toBe('boom');
    expect(errorDetail('text')).toBe('text');
    expect(errorDetail({})).toBe('unknown error');
  });
});
