# Model Manager

The initial Model Manager enumerates models from LM Studio without assuming a fixed model ID. The UI displays the explicit model ID, owner metadata, and quantization suffix when LM Studio exposes it in the ID, for example `q8_0` or `f16`.

Quantization metadata inferred from an ID is display-only. Adapter selection remains an explicit persisted setting; the application does not choose an adapter by searching for `gemma` in a model name.

LM Studio's current `/v1/models` response does not expose richer architecture, memory, or context metadata. Those fields remain unknown until a runtime provides them.

The `/v1/models/{id}` endpoint was also checked and currently returns only the same ID and owner fields. The UI therefore provides an explicit refresh action rather than inventing architecture or memory values.
