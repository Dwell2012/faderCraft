# Windows ASIO output

Open Settings > Audio, choose ASIO, select the installed device, and choose Driver default or a supported buffer size. Restart DJCraft to apply. The running-output line reports the opened backend, device, sample rate and requested buffer. Unsupported devices/buffers produce an audio error; switch back to System audio and restart to recover. No silent fallback to another output is performed.

The driver must already be installed. This feature does not install drivers. Settings > Audio exposes separate master and headphone stereo output pairs on the selected device. Headphones are disabled by default; enabling them sends the active deck pre-fader/post-EQ and effects, at 50% software level. Selecting or loading a deck switches cue to it. Master and cue pairs must be distinct and supported at the driver's default sample rate; invalid choices report an error rather than sharing a pair. Changes require restart. The driver's control panel is not exposed. Buffer requests are validated against the reported range; the driver remains responsible for supported granularity and actual latency.

For CONNECT 6, select its LEWITT ASIO driver, master 1/2 and headphone cue 3/4 (or another distinct supported pair). Configure speakers and the desired headphone socket's source in LEWITT CONTROL CENTER. Software channel pairs are not hardwired headphone sockets. Hardware playback/routing still requires listening verification. See the [LEWITT guide](https://www.lewitt-audio.com/support/online-guides/connect6).

## Mixer controls

The mixer has headphone volume and cue/master blend knobs to the right of deck B's
trim/EQ. Blend 0 is the active deck only; 100 is the master only. At the midpoint
both sources are at unity; each half fades only the opposite source. Volume affects
only the headphone bus. On the left of deck A's trim/EQ, sampler volume controls
all sampler pads together before the master fader. Knobs support vertical drag,
mousewheel (Shift for fine adjustment), and double-click reset. Levels are session
controls: headphone gain starts at 50%, cue mix at cue-only, sampler at unity.

## Building

Windows builds enable CPAL's `asio` feature. `asio-sys` locates Steinberg's SDK using CPAL_ASIO_DIR or its temporary asio_sdk cache. It requires the existing MSVC and libclang toolchain. If the automatic PowerShell extraction fails, extract the official SDK separately and set CPAL_ASIO_DIR to the directory containing common and host.

## SDK licensing

The Rust asio-sys wrapper is MIT, but the separately downloaded Steinberg ASIO SDK has separate terms. This development build uses the proprietary Steinberg ASIO licence option, not GPL. The 2025 SDK LICENSE.txt permits local use and requires a signed Steinberg agreement before publishing software under that option. Do not redistribute the SDK in this repository. Cargo's licence check covers Rust dependencies, not the externally downloaded SDK. Review the actual SDK licence and obtain the publishing agreement before distributing an ASIO-enabled binary. Project code remains MIT.

Startup checks ASIO in a short-lived hidden process before opening it in the UI. A failed or timed-out check leaves Settings available for recovery. The installed Realtek ASIO driver crashed this local probe with 0xC0000094 on both default and 256-frame requests; UX2 was registered but unavailable. Controller hardware playback has not yet been verified.
