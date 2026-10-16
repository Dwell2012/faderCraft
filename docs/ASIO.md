# Windows ASIO output

Open Settings > Audio, choose ASIO, select the installed device, and choose Driver default or a supported buffer size. Restart DJCraft to apply. The running-output line reports the opened backend, device, sample rate and requested buffer. Unsupported devices/buffers produce an audio error; switch back to System audio and restart to recover. No silent fallback to another output is performed.

The driver must already be installed. This feature does not install drivers. Master audio is routed to the first two channels; headphone routing and the driver's control panel are not yet exposed. Sample rate follows the driver's default configuration. Buffer requests are validated against the reported range; the driver remains responsible for supported granularity and actual latency.

## Building

Windows builds enable CPAL's `asio` feature. `asio-sys` locates Steinberg's SDK using CPAL_ASIO_DIR or its temporary asio_sdk cache. It requires the existing MSVC and libclang toolchain. If the automatic PowerShell extraction fails, extract the official SDK separately and set CPAL_ASIO_DIR to the directory containing common and host.

## SDK licensing

The Rust asio-sys wrapper is MIT, but the separately downloaded Steinberg ASIO SDK has separate terms. This development build uses the proprietary Steinberg ASIO licence option, not GPL. The 2025 SDK LICENSE.txt permits local use and requires a signed Steinberg agreement before publishing software under that option. Do not redistribute the SDK in this repository. Cargo's licence check covers Rust dependencies, not the externally downloaded SDK. Review the actual SDK licence and obtain the publishing agreement before distributing an ASIO-enabled binary. Project code remains MIT.

Startup checks ASIO in a short-lived hidden process before opening it in the UI. A failed or timed-out check leaves Settings available for recovery. The installed Realtek ASIO driver crashed this local probe with 0xC0000094 on both default and 256-frame requests; UX2 was registered but unavailable. Controller hardware playback has not yet been verified.
