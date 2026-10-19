# Setting up Tududu

This guide takes you from a fresh clone to speaking English and getting the text pasted in another language, on **macOS** or **Windows**.

Tududu runs entirely on your computer. The only downloads are the tools and the speech model; your voice and text never leave the machine.

## Quick start: one command

The setup script installs whatever's missing, gets the code, and starts Tududu. Run it again any time: it skips what's already done. The first start compiles the speech engine, which takes **10–25 minutes**.

**Windows** (x64 PCs). Open **PowerShell** and paste:

```powershell
powershell -ExecutionPolicy Bypass -Command "& ([scriptblock]::Create((irm https://raw.githubusercontent.com/TaniKroos/tududu/main/scripts/setup-windows.ps1)))"
```

It installs Git, Rust, Bun, CMake, the Vulkan SDK and the Visual Studio C++ Build Tools with `winget` (Windows may ask for permission a few times), clones Tududu into a `tududu` folder, and starts it.

**macOS.** Open **Terminal** and paste:

```bash
curl -fsSL https://raw.githubusercontent.com/TaniKroos/tududu/main/scripts/setup-macos.sh | bash
```

It installs the Xcode Command Line Tools, Homebrew, CMake, Rust and Bun if needed (it may ask for your Mac password), clones Tududu into a `tududu` folder, and starts it.

**Already cloned?** Run the script from the repository folder instead:

|         | Start the app                                                        | Build an installer | Set up only    |
| ------- | -------------------------------------------------------------------- | ------------------ | -------------- |
| Windows | `powershell -ExecutionPolicy Bypass -File scripts\setup-windows.ps1` | add `-Build`       | add `-NoRun`   |
| macOS   | `./scripts/setup-macos.sh`                                           | add `--build`      | add `--no-run` |

When the window opens, continue with [First run](#first-run-set-up-translation). To start Tududu again later, run the same command; setup is skipped and the app opens in seconds.

If the script stops with a message, follow what it says (often: open a new PowerShell or Terminal window and run it again). The manual steps below do the same thing by hand.

## All options

- [Option A: install a prebuilt app](#option-a-install-a-prebuilt-app)
- [Option B: build from source by hand](#option-b-build-from-source)
  - [macOS](#macos)
  - [Windows](#windows)
- [First run: set up translation](#first-run-set-up-translation)
- [Using Tududu](#using-tududu)
- [Troubleshooting](#troubleshooting)
- [Uninstalling](#uninstalling)

---

## Option A: install a prebuilt app

Every push to `main` builds installers for macOS (Apple Silicon and Intel) and Windows (x64 and ARM64).

1. Open the repository's **Actions** tab on GitHub and select the latest successful **Main Branch Build** run.
2. Under **Artifacts**, download the one for your computer (you must be signed in to GitHub):

   | Your computer                                           | Artifact                    | Inside                  |
   | ------------------------------------------------------- | --------------------------- | ----------------------- |
   | Mac with Apple Silicon (M1 or later)                    | `…-aarch64-apple-darwin`    | `.dmg`                  |
   | Intel Mac                                               | `…-x86_64-apple-darwin`     | `.dmg`                  |
   | Windows PC (most PCs)                                   | `…-x86_64-pc-windows-msvc`  | `.msi` and `-setup.exe` |
   | Windows on ARM (e.g. Surface Pro X, Snapdragon laptops) | `…-aarch64-pc-windows-msvc` | `.msi` and `-setup.exe` |

3. Install it. The builds aren't code-signed yet, so your system will warn you the first time:
   - **macOS:** drag Tududu to Applications, then run this once in Terminal before opening it:
     ```bash
     xattr -dr com.apple.quarantine /Applications/Tududu.app
     ```
   - **Windows:** if SmartScreen shows "Windows protected your PC", click **More info → Run anyway**.

Then continue with [First run](#first-run-set-up-translation).

---

## Option B: build from source

The first build compiles the speech engine and takes **10–25 minutes**. Later builds take seconds. You need about **10 GB** of free disk space.

### macOS

**1. Install the tools** (once):

```bash
xcode-select --install                                   # Apple's compilers
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y   # Rust
curl -fsSL https://bun.sh/install | bash                 # Bun (JavaScript tooling)
brew install cmake                                       # needs Homebrew: https://brew.sh
```

Close and reopen Terminal so `cargo` and `bun` are on your `PATH`.

**2. Clone and prepare:**

```bash
git clone https://github.com/TaniKroos/tududu.git
cd tududu
bun install
mkdir -p src-tauri/resources/models
curl -L -o src-tauri/resources/models/silero-vad-v6.2-F32.gguf \
  https://huggingface.co/handy-computer/silero-vad-v6.2-gguf/resolve/main/silero-vad-v6.2-F32.gguf
```

The last command downloads the small voice-detection model (about 1 MB) that the app needs to build.

**3. Run it:**

```bash
CMAKE_POLICY_VERSION_MINIMUM=3.5 bun run tauri dev
```

The Tududu window opens when the build finishes. Leave the Terminal open while you use it. Code changes rebuild and relaunch the app automatically.

To build an installable `.dmg` instead:

```bash
CMAKE_POLICY_VERSION_MINIMUM=3.5 bun run tauri build
```

The result is in `src-tauri/target/release/bundle/dmg/`.

### Windows

These steps are for normal (x64) Windows 10 or 11 PCs. On **Windows on ARM**, use the [prebuilt installer](#option-a-install-a-prebuilt-app): building from source there needs a separate clang-cl toolchain.

**1. Install the tools** (once). Open **PowerShell** and run:

```powershell
winget install Git.Git
winget install Rustlang.Rustup
winget install Oven-sh.Bun
winget install Kitware.CMake
winget install KhronosGroup.VulkanSDK
winget install Microsoft.VisualStudio.2022.BuildTools --override "--wait --passive --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
```

- The Visual Studio Build Tools provide the C++ compiler. If you already have Visual Studio 2022 with **Desktop development with C++**, you can skip that line.
- The Vulkan SDK lets Tududu use your graphics card.
- WebView2, which draws the app's window, is already part of Windows 10 and 11.

**Close PowerShell and open a new one** so the new tools (and the `VULKAN_SDK` variable) are picked up. Check everything is there:

```powershell
cargo --version; bun --version; cmake --version; echo $env:VULKAN_SDK
```

Each command should print a version, and the last should print a folder path.

**2. Clone and prepare:**

```powershell
git clone https://github.com/TaniKroos/tududu.git
cd tududu
bun install
mkdir src-tauri\resources\models -Force
curl.exe -L -o src-tauri\resources\models\silero-vad-v6.2-F32.gguf https://huggingface.co/handy-computer/silero-vad-v6.2-gguf/resolve/main/silero-vad-v6.2-F32.gguf
```

Type `curl.exe`, not `curl`: in PowerShell, plain `curl` is a different command.

**3. Run it:**

```powershell
$env:CMAKE_POLICY_VERSION_MINIMUM = "3.5"
bun run tauri dev
```

The Tududu window opens when the build finishes. Keep PowerShell open while you use it.

To build an installer instead:

```powershell
$env:CMAKE_POLICY_VERSION_MINIMUM = "3.5"
bun run tauri build
```

The `.msi` and `-setup.exe` are in `src-tauri\target\release\bundle\`.

---

## First run: set up translation

1. **Permissions.** Tududu asks for:
   - **Microphone:** allow it.
   - **Accessibility (macOS only):** needed so the shortcuts work and Tududu can paste into other apps. Turn Tududu on in System Settings → Privacy & Security → Accessibility. In development this may show as your terminal app or VS Code instead of Tududu.
2. **Choose the translation model.** Open the **Models** page and download **Canary 1B v2** (about 800 MB), then select it. It translates English into these languages:

   Bulgarian, Croatian, Czech, Danish, Dutch, Estonian, Finnish, French, German, Greek, Hungarian, Italian, Lithuanian, Maltese, Polish, Portuguese, Romanian, Russian, Slovak, Slovenian, Spanish, Swedish, Ukrainian.

   Other models transcribe English but can't translate into these languages.

3. **Pick your language.** Open **General → Translation** and choose one under **Translate To**.
4. **To translate text you read into English** (optional): on the **Models** page, under **Text Models**, download **Gemma 3 1B** (about 770 MB). It's a separate model because Canary only understands speech, not text.

---

## Using Tududu

Click into any text field (an email, a chat, a document), then **hold** a shortcut, speak, and **release**:

| What you want                                 | macOS                | Windows            |
| --------------------------------------------- | -------------------- | ------------------ |
| Speak English → paste in your chosen language | **⌥ Option + T**     | **Ctrl + Alt + T** |
| Speak English → paste English                 | **⌥ Option + Space** | **Ctrl + Space**   |
| Cancel a recording                            | **Esc**              | **Esc**            |

You can change the shortcuts under **General**. If the chosen language isn't one the model can translate into, Tududu pastes the English text instead.

### Translating selected text into English

Select text in any app (a message, an email, a web page), then press **⌥ Option + Shift + E** on macOS or **Ctrl + Alt + E** on Windows. The English appears in a small popup and is copied to your clipboard. Click the popup to close it early.

This uses the **Gemma 3 1B** text model and works for most languages, including ones Canary doesn't cover, such as Chinese, Hindi and Japanese. The first translation after starting Tududu takes a couple of seconds while the model loads; later ones take well under a second. The model unloads after the same idle time as the speech model (**Advanced → Unload Model**).

To check a translation says what you meant, set **General → Translation → Also Paste English** to **On a new line** or **In brackets**. Tududu then pastes, for example, `Hola, ¿cómo estás? (Hello, how are you?)`.

Everything you say is kept on the **History** page, where you can copy or replay it.

### Translating an audio file (optional)

A development build can translate a recording from the command line. The file must be a 16 kHz mono 16-bit WAV.

```bash
# macOS
src-tauri/target/debug/handy -f speech.wav --model handy-computer/canary-1b-v2-gguf/canary-1b-v2-Q5_K_M.gguf --translate-to de
```

```powershell
# Windows
src-tauri\target\debug\handy.exe -f speech.wav --model handy-computer/canary-1b-v2-gguf/canary-1b-v2-Q5_K_M.gguf --translate-to de
```

Use any language code from the list above (`de`, `fr`, `es`, `it`, `pt`, `nl`, `pl`, `sv`, `uk`, …). The model must already be downloaded through the app.

Text works the same way, with the text model downloaded:

```bash
src-tauri/target/debug/handy --translate-text "Hola, ¿qué tal?"
```

With Tududu running, `--translate-selection` translates the current selection, as the shortcut does, so other hotkey tools can trigger it.

---

## Troubleshooting

**The shortcut does nothing (macOS).**
Turn on Accessibility for Tududu (or, in development, for your terminal or VS Code) in System Settings → Privacy & Security → Accessibility. Then quit and reopen Tududu.

**Ctrl + Space does nothing (Windows).**
Some keyboard layouts and input methods use Ctrl + Space. Pick a different shortcut under **General**.

**Text doesn't appear in an app (Windows).**
Windows won't let a normal app type into a program running as administrator. Run that program normally, or run Tududu as administrator too.

**I get English instead of a translation.**
Check that **Canary 1B v2** is the selected model and that a language is chosen under **General → Translation**. A warning appears there if the model can't translate.

**Translate Selection says "Select some text first" even though text is selected.**
Tududu copies the selection with Cmd + C / Ctrl + C. Some places block copying (password fields, some protected documents). On macOS, Tududu also needs Accessibility permission, as for the other shortcuts.

**Building needs internet the first time.**
The build downloads llama.cpp's text engine (12–32 MB) from GitHub and checks it against a pinned checksum. To build offline, set `TUDUDU_SKIP_LLAMA_RUNTIME=1`; translating selected text is then unavailable in that build.

**`cmake` error mentioning `CMAKE_POLICY_VERSION_MINIMUM` or "Compatibility with CMake < 3.5".**
Set the variable before building, as in the run steps above.

**Windows build error about SPIRV headers.**
Reinstall the Vulkan SDK with all components, open a new PowerShell, and build again.

**Windows build errors about long paths (`MSB3491`, `FTK1011`, `MSB6003`).**
See the matching section in [BUILD.md](BUILD.md#windows-build-fails-with-path-limit-errors-msb3491--ftk1011--msb6003).

**Microphone not detected (Windows).**
Settings → Privacy & security → Microphone: turn on **Let desktop apps access your microphone**.

---

## Uninstalling

### Windows

`scripts\uninstall-windows.ps1` removes everything the setup put on your PC. It shows each item with its size and asks before removing it. First, see what it would remove (nothing is changed):

```powershell
powershell -ExecutionPolicy Bypass -File scripts\uninstall-windows.ps1 -DryRun
```

Then run it for real. It asks for administrator rights, opens in a new window, and asks about each item:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\uninstall-windows.ps1
```

| Removed by default (press Enter)                           | Kept by default (type `y` to remove)                             |
| ---------------------------------------------------------- | ---------------------------------------------------------------- |
| Tududu source folder(s), including build output            | Git, Rust, Bun, CMake, Vulkan SDK, Visual Studio C++ Build Tools |
| An installed Tududu app                                    |                                                                  |
| Settings, history and recordings                           |                                                                  |
| Downloaded speech and text models                          |                                                                  |
| Build leftovers and Windows Defender exclusions for Tududu |                                                                  |

Build tools stay unless you say otherwise, because other programs may use them. Tools that the setup script installed itself are the exception: for those, Enter means remove. Add `-All` to remove everything without questions, or `-Dir <folder>` if you cloned Tududu somewhere other than your user folder.

The script deletes its own folder along the way, so run it from a copy if you want to keep the script.

### macOS

Remove these by hand:

```bash
rm -rf ~/tududu                                              # or wherever you cloned it
rm -rf ~/Library/Application\ Support/com.smudgee.tududu      # settings, history, recordings
rm -rf ~/.cache/huggingface/hub/models--handy-computer--*     # speech models (shared with Handy)
rm -rf ~/.cache/huggingface/hub/models--ggml-org--gemma-3-1b-it-GGUF   # text model
```

If you installed the app, drag Tududu from Applications to the Trash. Rust (`rustup self uninstall`), Bun (`rm -rf ~/.bun`) and CMake (`brew uninstall cmake`) can be removed the same way if nothing else needs them.

For more build details, see [BUILD.md](BUILD.md).
