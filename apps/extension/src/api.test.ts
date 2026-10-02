import { afterEach, describe, expect, it, vi } from 'vitest';
import { getStatus, translate } from './api';

describe('local API client', () => {
  afterEach(() => vi.restoreAllMocks());

  it('sends the bearer token and browser field names', async () => {
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(
        JSON.stringify({
          text: 'Привет',
          model_id: 'translategemma-4b',
          adapter_id: 'translategemma',
          latency_ms: 42,
          provider_id: 'deepL',
          billed_characters: 450,
        }),
        { status: 200, headers: { 'Content-Type': 'application/json' } },
      ),
    );

    await expect(
      translate('secret', 'Hello', 'en', 'ru'),
    ).resolves.toMatchObject({
      text: 'Привет',
      latency_ms: 42,
      provider_id: 'deepL',
      billed_characters: 450,
    });
    expect(fetchMock).toHaveBeenCalledWith(
      'http://127.0.0.1:47831/api/v1/translate',
      expect.objectContaining({
        headers: expect.objectContaining({ Authorization: 'Bearer secret' }),
        body: JSON.stringify({
          text: 'Hello',
          sourceLanguage: 'en',
          targetLanguage: 'ru',
        }),
      }),
    );
    const body = JSON.parse(fetchMock.mock.calls[0][1]?.body as string);
    expect(body).toEqual({
      text: 'Hello',
      sourceLanguage: 'en',
      targetLanguage: 'ru',
    });
  });

  it('surfaces the desktop API error message', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(JSON.stringify({ error: 'authorization required' }), {
        status: 401,
      }),
    );

    await expect(translate('wrong', 'Hello', 'en', 'ru')).rejects.toThrow(
      'authorization required',
    );
  });

  it('validates a pairing token through the status endpoint', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(
        JSON.stringify({
          available: true,
          endpoint: 'local',
          detail: 'LM Studio is reachable',
        }),
        { status: 200 },
      ),
    );

    await expect(getStatus('secret')).resolves.toMatchObject({
      available: true,
    });
  });
});
