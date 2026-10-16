# Direct WASAPI output

Choose **WASAPI exclusive** in Settings > Audio, choose an output device and
buffer size, then restart DJCraft. No ASIO driver or additional installer is needed.
Exclusive mode reserves that output for DJCraft; other applications cannot share it.
Use System / WASAPI shared when you need other applications to play on the same device.

The engine prefers the device's Windows mix sample rate, then tries 48, 44.1 and
96 kHz with stereo float or integer PCM formats supported in exclusive mode.
Decoded tracks are resampled by the existing engine to the negotiated rate.
Requested buffers are clamped/aligned to the driver's requirements. Currently
running reports the actual frame count and buffer duration, not round-trip latency.
Driver default uses the device's default period. Smaller buffers can cause dropouts.

Rendering runs on a dedicated COM thread with event-driven exclusive timing and
Windows Pro Audio MMCSS scheduling when available. Sample buffers are preallocated;
format conversion and mixer rendering do not allocate. Shutdown stops the stream,
releases the device and reverts thread scheduling. Failed startup leaves the UI
available to select another backend; there is no silent switch to shared output.
A stopped/disconnected stream is reported when Settings is opened; restart to recover.

The first implementation requires a supported stereo format. Separate master and
headphone outputs, a sample-rate selector and live device switching are not included.
Some USB drivers work poorly with exclusive event timing; shared output or the
hardware ASIO driver remain alternatives.

Implementation uses the MIT-licensed wasapi crate and Windows bindings, with no
ASIO SDK in this output path. Existing optional ASIO support retains its own terms.

Validation: `cargo run -p djcraft-engine --example wasapi_exclusive -- 256`
opens silent output for two seconds and releases it. An optional second argument
selects the exact device name. Unit tests cover signed PCM, float, clipping and
24-bit samples in 32-bit containers.
