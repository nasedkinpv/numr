# User guide

See the [README](../README.md#usage) for installation, CLI usage, operations, and units, or the [JSON-RPC reference](json-rpc.md) for the server protocol.

## Keybindings (TUI)

The TUI supports **Vim** (default) and **Standard** modes. Press `Shift+Tab` to switch; mouse wheel and trackpad scrolling work in both.

### Vim Mode

#### Normal Mode

| Key | Action |
|-----|--------|
| `i` / `a` | Enter Insert mode at/after cursor |
| `I` / `A` | Enter Insert mode at line start/end |
| `o` / `O` | New line below/above and enter Insert mode |
| `s` | Substitute character (delete and insert) |
| `C` | Change to end of line |
| `h` / `j` / `k` / `l` | Move left/down/up/right |
| `w` / `b` / `e` | Word forward/backward/end |
| `0` / `$` | Line start/end |
| `gg` / `G` | First/last line |
| `Space` | Move right |
| `PageUp/Down` | Scroll page |
| `x` / `X` | Delete char forward/backward |
| `dd` | Delete line |
| `D` | Delete to end of line |
| `J` | Join lines |
| `W` / `N` / `H` | Toggle wrap/line numbers/header |
| `?` / `F1` | Toggle help |
| `Ctrl+s` | Save |
| `Ctrl+r` | Refresh rates |
| `F12` | Toggle debug |
| `Shift+Tab` | Switch to Standard mode |
| `q` | Quit |

#### Insert Mode

| Key | Action |
|-----|--------|
| `Esc` | Return to Normal mode |
| Type | Insert text |
| `Backspace` / `Delete` | Delete char backward/forward |
| `Option+Backspace` / `Ctrl+w` | Delete previous word |
| `Cmd+Backspace` / `Ctrl+u` | Delete to line start |
| `Enter` | New line |
| `Arrows` / `PageUp/Down` | Navigate |
| `Home` / `End` | Line start/end |
| `Ctrl+s` | Save |

### Standard Mode

Direct input without modal switching.

| Key | Action |
|-----|--------|
| Type | Insert text directly |
| `Arrow keys` | Move cursor |
| `Home` / `End` | Line start/end |
| `PageUp/Down` | Scroll page |
| `Ctrl+a` / `Ctrl+e` | Line start/end |
| `Ctrl+g` | Go to first line |
| `Backspace` / `Delete` | Delete char |
| `Option+Backspace` / `Ctrl+w` | Delete previous word |
| `Cmd+Backspace` / `Ctrl+u` | Delete to line start |
| `Ctrl+k` | Delete to line end |
| `Enter` | New line |
| `Option+z` | Toggle wrap |
| `Ctrl+l` / `Ctrl+h` | Toggle line numbers/header |
| `?` / `F1` | Toggle help |
| `Ctrl+s` | Save |
| `Ctrl+r` | Refresh rates |
| `Shift+Tab` | Switch to Vim mode |
| `Ctrl+q` | Quit |

Wrap mode places results on the final expression row, ignoring trailing comments. The footer groups currency and unit totals separately, using the current exchange rates.

## Configuration

Native config, rate cache, and the default document live in the OS config directory:

| Platform | Directory |
|----------|-----------|
| Linux | `$XDG_CONFIG_HOME/numr/`, or `~/.config/numr/` if unset |
| macOS | `~/Library/Application Support/numr/` |

`numr` opens `default.numr` there unless `[files] default_path` overrides it. A command-line file argument takes precedence over both. TUI settings persist automatically when toggled.

Example `config.toml` (omit `default_path` to keep the default document):

```toml
[preferences]
keybinding_mode = "vim"   # "vim" or "standard"
wrap_mode = false
show_line_numbers = false
show_header = false
# decimals = 4          # Optional display precision, 0–28 decimal places

[files]
default_path = "~/Documents/calculations.numr"

[api]
fiat_rates_url = "https://open.er-api.com/v6/latest/USD"
crypto_rates_url = "https://api.coingecko.com/api/v3/simple/price"

[api.keys]
coingecko_api_key = "your-key-here"
```

The TUI uses the file and API settings; `numr-cli` reads the display precision, not those TUI overrides. CoinGecko API key header (demo vs pro) is selected automatically based on the URL host.

### Display precision

Set `[preferences] decimals = 4` to display `152/150` as `1.0133` in the TUI, CLI/REPL, and JSON-RPC `display` strings. This only rounds presentation: stored values, variables, and subsequent calculations keep their full precision. JSON-RPC `value` retains its legacy format (`"1.01"` in this example), not the configured display precision or a guarantee of full stored precision.

Explicit precision applies to numbers, percentages, units, and currencies; binary and hexadecimal output are unchanged. Small values may display as zero. Omit `decimals` for the default formatting (two places for ordinary numbers, currency-specific precision, and the small-number fallback). Values outside 0–28 are invalid config.

By contrast, `round(x, n)` changes the value, rounding to 0–28 decimal places with midpoint-to-even rounding; `round(x)` rounds to an integer.

Web integrations can set presentation precision with `WasmEngine.set_decimal_places(4)` and reset it with `WasmEngine.set_decimal_places(undefined)`. They do not read native config; the separate web frontend owns its settings and persistence.

### Exchange rates

Native adapters explicitly load and share `rates.json` in the config directory, with a 1-hour expiry and atomic cache replacement:

- **TUI**: Loads the cache and starts a background refresh without blocking the event loop.
- **CLI/REPL**: Loads the cache, then fetches only if no usable cached rates exist.
- **JSON-RPC server**: Loads the cache at startup; network access occurs only through `reload_rates`.

Without usable cached or fetched rates, the engine uses built-in defaults, not live rates. `Engine::new()` itself performs no filesystem or network I/O; web integrations supply rates separately.

Default sources are [open.er-api.com](https://open.er-api.com) for fiat and [CoinGecko](https://www.coingecko.com/en/api) for crypto. See [architecture](architecture.md) for rate data flow and embedding contracts.
