# local-ai-advisor

```text
       .--------.                                      .----------.
      /  CPU   /|                                    /  7B Q4   /|
     /------- / |       LOCAL AI ADVISOR            /----------/ |
     |  RAM  |  +------> scan  score  rank -------->|   FAST   |  |
     |       | /                                      |   FITS   | /
     '-------'                                        '----------'
          Your silicon. Your shortlist. No guesswork.
```

Stop guessing which model your laptop can run.

`local-ai-advisor` scans your hardware, checks Ollama, and ranks local LLMs that fit your machine. Pick a use case and get a shortlist with memory estimates, expected speed, and the exact `ollama pull` command.

```console
$ local-ai-advisor recommend --use-case coding

Recommended models for coding:

1. qwen2.5-coder:7b
   Expected performance: fast
   Memory estimate: 5.5–7.0 GB
   Why: Strong coding performance with enough memory left for your editor and tools.
   Install: ollama pull qwen2.5-coder:7b
```

Your hardware profile and recommendation scores stay on your machine. The advisor prints install commands and leaves the pulling, benchmarking, and fan noise to you.

## Install

Install with Homebrew:

```bash
brew tap chrislaughlin/tap
brew trust --formula chrislaughlin/tap/local-ai-advisor # Homebrew 6+
brew install local-ai-advisor
```

Or try the one-line install:

```bash
brew install chrislaughlin/tap/local-ai-advisor
```

Homebrew 6 asks you to trust third-party formulae. If the one-line command reports an untrusted tap, run `brew trust --formula chrislaughlin/tap/local-ai-advisor` and repeat the install.

To build from source, install a current stable Rust toolchain and run:

```bash
cargo build --release
cargo install --path .
```

You can find the release binary at `target/release/local-ai-advisor` after the build.

## Find your model

```bash
# See what the advisor detects
local-ai-advisor scan

# Get the default shortlist
local-ai-advisor recommend

# Optimize the ranking for your workload
local-ai-advisor recommend --use-case coding
local-ai-advisor recommend --use-case agent
local-ai-advisor recommend --use-case reasoning

# Control catalogue access
local-ai-advisor recommend --online
local-ai-advisor recommend --offline

# Feed another tool or agent
local-ai-advisor scan --format json
local-ai-advisor recommend --format json
```

More commands:

```bash
local-ai-advisor catalog refresh
local-ai-advisor catalog search qwen
local-ai-advisor ollama
local-ai-advisor explain
```

`recommend` uses fresh cached metadata when it can and refreshes stale or missing public metadata. `--online` forces a refresh. `--offline` skips public network requests and combines the existing cache with the built-in catalogue. It can still query Ollama on your machine.

## Sample hardware scan

```text
Hardware summary:
- OS: macOS (aarch64)
- CPU: Apple M4 Pro (12 logical cores)
- Memory: 24.0 GB unified memory
- Available memory: 15.0 GB
- Ollama: installed and running
```

Rankings depend on your hardware, installed models, selected use case, and catalogue data.

## The math behind the shortlist

The advisor reserves memory for your operating system and open apps before it ranks a model:

- 8 GB or less: 2 GB reserved
- Around 16 GB: 4 GB reserved
- Around 24 GB: 6 GB reserved
- 32 GB or more: 25% reserved

It treats Apple Silicon RAM as unified memory and keeps the same headroom.

For a GGUF with a known file size, the advisor estimates minimum RAM at file size × 1.25 and recommended RAM at file size × 1.5. Without a file size, it estimates the weights from parameter count and quantization: about 0.5 GB per billion parameters for Q4, 0.65 GB for Q5, or 1.1 GB for Q8. It then adds 1–3 GB for runtime and context overhead.

Treat those numbers as estimates. Context length, KV cache type, runner, GPU offload, prompt caching, thermals, and open applications change memory use and speed. The labels `fast`, `usable`, and `slow` offer relative guidance rather than benchmark results.

## Ollama-aware ranking

The advisor boosts models you have installed when it can reach the local Ollama API. It suggests pull commands such as:

```bash
ollama pull qwen2.5-coder:7b
```

You choose whether to run them. The advisor validates public model names before placing them in a shell command.

## Catalogue and cache

The Hugging Face source searches popular public GGUF repositories and reads GGUF filenames for parameter size and quantization. The advisor treats public metadata as untrusted data.

It stores the catalogue cache here:

- macOS: `~/Library/Caches/local-ai-advisor/catalog.json`
- Linux: `~/.cache/local-ai-advisor/catalog.json` in a standard setup
- Windows: your local cache directory

The cache lasts 24 hours. If a refresh fails, the advisor tries the stale cache, then the built-in catalogue. You still get recommendations without a connection.

## Privacy and safety

The scanner reads your hardware through local, read-only system APIs and fixed-argument commands such as `sysctl`, `system_profiler`, `lspci`, and `nvidia-smi`. It sends no hardware profile with public catalogue requests.

The advisor does not interpolate user input into shell commands, execute downloaded model files, or make non-GET public HTTP requests. Network calls include timeouts.

## Development

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

See [`docs/homebrew-maintainer.md`](docs/homebrew-maintainer.md) for release and Homebrew maintenance instructions.
