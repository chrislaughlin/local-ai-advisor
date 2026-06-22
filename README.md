# local-ai-advisor

`local-ai-advisor` inspects the current computer and recommends local LLMs that should realistically fit. It combines a small offline catalogue, cached public GGUF metadata from Hugging Face, and the models installed in Ollama. Hardware details and recommendation scoring stay on the machine.

The project is deliberately advisory: it prints install commands but never downloads, installs, or executes a model.

## Install

Install with Homebrew:

```bash
brew tap chrislaughlin/tap
brew trust --formula chrislaughlin/tap/local-ai-advisor # Homebrew 6+
brew install local-ai-advisor
```

Or install it in one command:

```bash
brew install chrislaughlin/tap/local-ai-advisor
```

Homebrew 6 requires users to trust third-party formulae before installing. If
the one-line command reports an untrusted tap, run
`brew trust --formula chrislaughlin/tap/local-ai-advisor`, then repeat it.

To build directly from source, install a current stable Rust toolchain, then
build or install from this checkout:

```bash
cargo build --release
cargo install --path .
```

The release binary is also available at `target/release/local-ai-advisor` after building.

## Commands

```bash
local-ai-advisor scan
local-ai-advisor scan --format json

local-ai-advisor recommend
local-ai-advisor recommend --use-case coding
local-ai-advisor recommend --use-case agent
local-ai-advisor recommend --use-case reasoning
local-ai-advisor recommend --online
local-ai-advisor recommend --offline
local-ai-advisor recommend --format json

local-ai-advisor catalog refresh
local-ai-advisor catalog search qwen
local-ai-advisor ollama
local-ai-advisor explain
```

`recommend` uses a fresh cache when possible and refreshes stale or missing public metadata. `--online` forces that refresh. `--offline` makes no public network requests and combines any existing cache with the built-in catalogue; querying the loopback Ollama API remains allowed.

## Example

```text
Hardware summary:
- OS: macOS (aarch64)
- CPU: Apple M4 Pro (12 logical cores)
- Memory: 24.0 GB unified memory
- Available memory: 15.0 GB
- Ollama: installed and running

Recommended models for coding:

1. qwen2.5-coder:7b
   Best for: coding, agent, reasoning
   Expected performance: fast
   Memory estimate: 5.5–7.0 GB
   Why: A strong coding candidate that fits comfortably and leaves useful memory headroom.
   Install: ollama pull qwen2.5-coder:7b
```

Actual ordering changes with the machine, installed models, selected use case, and current catalogue.

## How estimates work

The advisor first reserves memory for the operating system and applications: 2 GB on machines with 8 GB or less, 4 GB around 16 GB, 6 GB around 24 GB, and 25% on machines with 32 GB or more. Apple Silicon RAM is treated as unified memory, with the same headroom reservation.

For a GGUF with a known file size, minimum RAM is estimated as file size × 1.25 and recommended RAM as file size × 1.5. Otherwise it estimates model weights from parameter count and quantization—roughly 0.5 GB per billion parameters for Q4, 0.65 GB for Q5, or 1.1 GB for Q8—then adds 1–3 GB of runtime/context overhead.

These figures are approximate. Context length, KV cache type, runner, GPU offload, prompt caching, thermals, and other open applications all affect real memory use and speed. “Fast”, “usable”, and “slow” are relative guidance, not benchmarks.

## Ollama

If Ollama is installed and its local API is reachable, local models receive a ranking boost. The tool only suggests commands:

```bash
ollama pull qwen2.5-coder:7b
```

It never runs a pull automatically. Public model names are validated before they can appear in a suggested shell command.

## Catalogue and cache

The Hugging Face source searches popular public GGUF repositories and inspects GGUF filenames for parameter size and quantization. Public metadata is untrusted and is used only as data. The cache lives in the platform cache directory:

- macOS: `~/Library/Caches/local-ai-advisor/catalog.json`
- Linux: `~/.cache/local-ai-advisor/catalog.json` (normally)
- Windows: the user’s local cache directory

Cache lifetime is 24 hours. A failed refresh falls back to stale cache data and then the built-in catalogue, so recommendation remains useful without connectivity.

## Privacy and safety

Hardware scanning is local. Hardware details are not uploaded; public catalogue requests contain no hardware profile. Inspection uses read-only system APIs and optional fixed-argument commands such as `sysctl`, `system_profiler`, `lspci`, and `nvidia-smi`. User input is never interpolated into shell commands, downloaded model files are never executed, and all public HTTP operations are GET requests with timeouts.

## Development

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

Release and Homebrew maintenance instructions are in
[`docs/homebrew-maintainer.md`](docs/homebrew-maintainer.md).
