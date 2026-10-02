# Local API

LingvoLoc exposes a small authenticated API for the future Chromium extension.

## Security

- The server binds only to `127.0.0.1:47831`.
- Every request requires `Authorization: Bearer <token>`.
- The token is generated at application startup and is available only to the trusted Tauri frontend through `get_api_token`.
- Browser CORS requests are accepted only from `chrome-extension://...` origins.
- Website origins are rejected; requests without an `Origin` header remain available for local diagnostics.

## Endpoints

All endpoints use the `/api/v1` prefix:

- `GET /status`
- `GET /models`
- `GET /settings/language-pair`
- `POST /translate`

Translation body:

```json
{
  "text": "Hello",
  "sourceLanguage": "en",
  "targetLanguage": "ru"
}
```

The API uses the runtime, cloud provider, model, adapter, and translation style selected in the desktop application. Cloud API keys are resolved from Windows Credential Manager and are never accepted in the request body. Successful API translations are added to local history just like desktop translations and contribute to the in-memory session usage panel.

Successful translations include exact provider usage when available:

```json
{
  "prompt_tokens": 214,
  "completion_tokens": 118,
  "total_tokens": 332,
  "provider_id": "openAi",
  "billed_characters": null
}
```

When the provider omits usage, these fields are unavailable rather than estimated. DeepL may return `billed_characters` instead of token counts. Session aggregates are exposed only in the desktop Settings surface and are not persisted.
