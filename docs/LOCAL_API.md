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

The API uses the model and adapter selected in the desktop application. Successful API translations are added to local history just like desktop translations.

Successful translations include exact provider usage when available:

```json
{
  "prompt_tokens": 214,
  "completion_tokens": 118,
  "total_tokens": 332
}
```

When the provider omits usage, these fields are unavailable rather than estimated. Usage is not persisted and aggregate statistics are not exposed.
