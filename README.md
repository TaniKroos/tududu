# Tududu

Push-to-talk dictation that **translates as you speak**. Hold a shortcut, speak English, release, and the text is pasted into whatever app you're using in the language you picked (Spanish, French, German, and more).

Everything runs locally on your computer. No account, no cloud, and your voice never leaves the machine.

- **Platforms:** macOS (Apple Silicon and Intel) and Windows (x64 and ARM64)
- **Status:** early development. Translation is being built on top of the Handy dictation engine.

## How it works

1. Press the shortcut and speak English.
2. Silence is trimmed with Silero VAD.
3. A local speech model (Canary 1B v2) turns English speech directly into text in your target language.
4. The result is pasted into the active app.

## Using translation

1. On the **Models** page, download and select **Canary 1B v2** (about 800 MB).
2. On the **General** page, under **Translation**, pick a language in **Translate To**.
3. Click into any text field, hold the translate shortcut (**⌥T** on macOS, **Ctrl+Alt+T** on Windows), speak English, and release.

The normal shortcut (**⌥Space** / **Ctrl+Space**) still pastes plain English. If the selected model can't translate into the chosen language, Tududu pastes the English text instead.

Canary 1B v2 translates English into Bulgarian, Croatian, Czech, Danish, Dutch, Estonian, Finnish, French, German, Greek, Hungarian, Italian, Lithuanian, Maltese, Polish, Portuguese, Romanian, Russian, Slovak, Slovenian, Spanish, Swedish and Ukrainian (and those languages back into English).

To translate an audio file from the command line (16 kHz mono WAV), using a development build:

```bash
src-tauri/target/debug/handy -f speech.wav --model handy-computer/canary-1b-v2-gguf/canary-1b-v2-Q5_K_M.gguf --translate-to de
```

## Development

Prerequisites: [Rust](https://rustup.rs/), [Bun](https://bun.sh/), and CMake (`brew install cmake` on macOS).

```bash
bun install
mkdir -p src-tauri/resources/models
curl -L -o src-tauri/resources/models/silero-vad-v6.2-F32.gguf \
  https://huggingface.co/handy-computer/silero-vad-v6.2-gguf/resolve/main/silero-vad-v6.2-F32.gguf
CMAKE_POLICY_VERSION_MINIMUM=3.5 bun run tauri dev
```

See [BUILD.md](BUILD.md) for platform-specific setup and [AGENTS.md](AGENTS.md) for the architecture.

## Credits

Tududu is a fork of [Handy](https://github.com/cjpais/Handy) by CJ Pais, released under the MIT License. The original Handy README is kept in [HANDY_README.md](HANDY_README.md). Speech recognition and translation run on [transcribe.cpp](https://github.com/handy-computer/transcribe.cpp) and [ggml](https://github.com/ggml-org/ggml). The Canary model is by NVIDIA, licensed CC-BY-4.0.
