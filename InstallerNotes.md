# Windows installer notes

## Automatic stem setup

Offer stem setup during installation. Detect the available hardware, select the fastest supported backend that passes an inference check, and download only its Fast four-stem model.

| Hardware | Fast backend |
| --- | --- |
| Supported NVIDIA GPU | CUDA, once DJCraft has a verified CUDA backend |
| Supported AMD Radeon | ROCm-enabled PyTorch |
| CPU or no usable GPU backend | ONNX CPU |

- Test each eligible GPU backend on a short audio excerpt. Check that inference actually ran on the GPU and returned four finite, correctly sized stems. If more than one GPU passes, choose the faster backend.
- If GPU setup or inference fails, explain the failure and finish with Fast ONNX CPU. Save the verified selection as the app's stem default.
- Keep Detailed as an optional download in Audio settings.

## Packaging

- Give the Radeon runtime a private Python 3.12 installation and pin the AMD PyTorch, torchaudio and Demucs packages. Fetch the GPU runtime only when needed. The working Radeon runtime occupies about 5.8 GiB installed before model weights.
- Check driver compatibility and obtain the pinned runtime packages from their official sources. Verify redistribution terms and include required notices before shipping an offline bundle.
- A supported RX 9070 XT should try Fast ROCm first. The installer and CUDA backend are future work.
