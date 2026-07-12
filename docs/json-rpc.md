# JSON-RPC 2.0 Interface

`numr-cli --server` exposes a stateful JSON-RPC 2.0 calculator over standard input and output. It is intended for editors, launchers, and local tools that can keep one process alive.

## Transport

- Each request or batch is one complete JSON value followed by `\n` (CRLF is also accepted).
- Each response is compact JSON followed by `\n` and is flushed immediately.
- A frame may contain at most 1 MiB (1,048,576 bytes) before the line ending.
- Oversized frames receive `-32600 Invalid Request` with `id: null`; the server drains that line and continues with later frames.
- Core expression limits still apply inside a valid transport frame: 16 KiB input, 256 operations, and 128 nesting levels per expression.

The server is stateful. Variables, history, continuations, and totals persist until `clear` or process exit. Applied rates survive `clear` and remain until they are replaced or the process exits.

## Requests, Notifications, and Batches

A call uses JSON-RPC version `2.0`, a string method, and a string, number, or `null` ID:

```json
{"jsonrpc":"2.0","method":"eval","params":{"expr":"20% of 150"},"id":1}
```

An omitted `id` is a notification. Notifications execute in order and can update later state, but the server writes no response for them. An explicit `"id": null` is still a call and receives a response with `"id": null`.

A batch is a non-empty JSON array. Entries execute sequentially, so a notification can define a variable used by a later call in the same batch. Responses retain call order, omit notification responses, and include an error entry for each invalid item. A batch containing only notifications produces no output line. An empty array is an invalid request.

```json
[
  {"jsonrpc":"2.0","method":"eval","params":{"expr":"subtotal = 40"}},
  {"jsonrpc":"2.0","method":"eval","params":{"expr":"subtotal + 2"},"id":"answer"}
]
```

## Methods

### `eval`

Evaluates and appends one line to existing state.

Params:

```json
{"expr":"2 km in m"}
```

Result: one [calculator value](#calculator-value-schema).

### `eval_lines`

Evaluates and appends lines in order without clearing existing state.

Params:

```json
{"lines":["price = $100","+ 20%","price"]}
```

Result: an array containing one calculator value per input line.

### `clear`

Clears document history and user variables. It accepts no params; omitted params, `[]`, and `{}` are accepted.

Result:

```json
{"message":"Cleared"}
```

### `get_totals`

Returns grouped currency and physical-unit totals. Plain numbers and percentages are omitted; continuation source lines and display-only aggregate lines are not counted. It accepts no params.

Result: an array of calculator values.

### `get_variables`

Returns user variables sorted by name. Internal `total`, `_`, `ANS`, and `ans` bindings are excluded. It accepts no params.

```json
[
  {"name":"price","value":{"type":"currency","value":"100.00","unit":"USD","display":"$100.00"}}
]
```

### `reload_rates`

Fetches fiat and crypto rates, validates and applies usable values, and attempts to persist the native cache. It accepts no params. Provider or cache warnings may be included in the human-readable message.

```json
{"message":"Rates reloaded"}
```

Server startup only loads a usable local cache over deterministic defaults; it does not perform a network request until this method is called.

## Calculator Value Schema

All calculator-returning methods use the same stable shape:

| Field | Type | Meaning |
|---|---|---|
| `type` | string | `number`, `percentage`, `currency`, `unit`, `empty`, or `error` |
| `value` | string, optional | Machine-friendly Decimal text; percentages are expressed in percentage points |
| `unit` | string, optional | Currency code or unit symbol |
| `message` | string, optional | Evaluation error detail |
| `display` | string | Complete text intended for presentation |

Examples:

```json
{"type":"number","value":"30","display":"30"}
{"type":"percentage","value":"20","display":"20%"}
{"type":"currency","value":"100.00","unit":"USD","display":"$100.00"}
{"type":"unit","value":"2","unit":"km","display":"2 km"}
{"type":"empty","display":""}
{"type":"error","message":"division by zero","display":"Error: division by zero"}
```

Parse and evaluation failures inside `eval`/`eval_lines` are successful JSON-RPC results with `type: "error"`. JSON-RPC error objects are reserved for malformed protocol messages, invalid methods/params, transport limits, serialization failures, and rate-refresh server failures.

## Protocol Errors

Errors follow the JSON-RPC 2.0 object shape and may include a `data` field with diagnostic context:

```json
{"jsonrpc":"2.0","error":{"code":-32602,"message":"Invalid params","data":"..."},"id":9}
```

| Code | Message | Used for |
|---:|---|---|
| `-32700` | `Parse error` | Malformed JSON |
| `-32600` | `Invalid Request` | Invalid request shape/version/ID, empty batch, invalid batch item, or frame above 1 MiB |
| `-32601` | `Method not found` | Unknown method |
| `-32602` | `Invalid params` | Missing, unexpected, or incorrectly typed params |
| `-32603` | `Internal error` | Response serialization failure |
| `-32000` | `Server error` | Rate refresh/application failure |

Malformed JSON and requests whose ID cannot be validated use `id: null`. For a valid call, method and parameter errors preserve its original ID.

## Complete Shell Examples

Single call:

```bash
printf '%s\n' '{"jsonrpc":"2.0","method":"eval","params":{"expr":"20% of 150"},"id":1}' \
  | numr-cli --server
```

Stateful session:

```bash
printf '%s\n' \
  '{"jsonrpc":"2.0","method":"eval","params":{"expr":"price = $100"}}' \
  '{"jsonrpc":"2.0","method":"eval","params":{"expr":"price + 20%"},"id":2}' \
  | numr-cli --server
```

The first line is a notification, so the only response corresponds to ID `2`.
