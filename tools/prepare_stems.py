"""DJCraft's optional local Demucs worker. Input/output: stereo float32 LE PCM.

No music leaves this machine. Only missing model weights are downloaded.
"""
import argparse
import contextlib
import json
import traceback
import threading
from pathlib import Path
import os
import sys
import time
from types import SimpleNamespace

import numpy as np
import torch
import torchaudio.functional as audio
from demucs.apply import apply_model
from demucs.pretrained import get_model
import demucs.apply as demucs_apply


MODELS = ("htdemucs", "htdemucs_ft")


def load_model(name):
    if name not in MODELS:
        raise ValueError("Unknown stem model")
    model = get_model(name).cpu().eval()
    # Written only after all official checkpoints loaded successfully.
    marker = Path(os.environ.get("TORCH_HOME", str(Path.home() / ".cache/torch"))).parent / ("model-" + name + ".ready")
    marker.write_text(name, encoding="utf-8")
    return model


def rocm_device():
    if not torch.version.hip or not torch.cuda.is_available():
        raise RuntimeError("AMD ROCm PyTorch and a compatible GPU are required")
    index = max(range(torch.cuda.device_count()),
                key=lambda i: torch.cuda.get_device_properties(i).total_memory)
    device = torch.device(f"cuda:{index}")
    print(f"Stem GPU: {torch.cuda.get_device_name(index)}", file=sys.stderr, flush=True)
    return device


def separate(model, wave, device):
    started = time.perf_counter()
    model.to(device)
    result = apply_model(model, wave, device=device, shifts=0, split=True,
                         overlap=0.25, progress=True, num_workers=0)[0]
    print(f"Stem inference: {time.perf_counter() - started:.3f}s ({device})",
          file=sys.stderr, flush=True)
    return result


def prepare(input_path, output_dir, sample_rate, model, device):
    def report(percent):
        print(f"DJCRAFT_PROGRESS {percent}", flush=True)

    report(0)
    torch.set_num_threads(max(1, min(4, (os.cpu_count() or 2) // 2)))
    torch.manual_seed(0)
    # Demucs 4.0.1 wraps completed chunk futures in tqdm. Replace only that
    # module's progress adapter with a machine-readable, completed-work count.
    completed = 0
    models = len(model.models) if hasattr(model, "models") else 1

    def progress_chunks(chunks, **kwargs):
        nonlocal completed
        total = len(chunks) * models
        for chunk in chunks:
            yield chunk
            completed += 1
            report(min(95, completed * 95 // max(1, total)))

    demucs_apply.tqdm = SimpleNamespace(tqdm=progress_chunks)
    samples = np.fromfile(input_path, dtype="<f4").reshape(-1, 2)
    frames = len(samples)
    wave = torch.from_numpy(samples.T.copy())
    if sample_rate != model.samplerate:
        wave = audio.resample(wave, sample_rate, model.samplerate)
    reference = wave.mean(0)
    mean, std = reference.mean(), reference.std().clamp_min(1e-8)
    with torch.inference_mode():
        separated = separate(model, ((wave - mean) / std)[None], device)
        separated = separated * std + mean
        if sample_rate != model.samplerate:
            separated = audio.resample(separated, model.samplerate, sample_rate)
    output_dir.mkdir(parents=True, exist_ok=True)
    for index, (name, source) in enumerate(zip(model.sources, separated)):
        source = source[:, :frames]
        if source.shape[-1] < frames:
            source = torch.nn.functional.pad(source, (0, frames - source.shape[-1]))
        result = source.T.contiguous().cpu().numpy().astype("<f4")
        if not np.isfinite(result).all():
            raise ValueError("Separator produced invalid samples")
        result.tofile(output_dir / (name + ".f32"))
        report(96 + index)


def serve(model_name):
    torch.set_num_threads(4)
    device = rocm_device()
    model = load_model(model_name)
    with torch.inference_mode():
        warm = separate(model, torch.zeros(1, 2, model.samplerate), device)
        if not torch.isfinite(warm).all():
            raise ValueError("ROCm warmup returned invalid samples")
    del warm
    print("DJCRAFT_READY", flush=True)
    for line in sys.stdin:
        try:
            request = json.loads(line)
            with open(request["log"], "w", encoding="utf-8") as log, contextlib.redirect_stderr(log):
                print(f"Stem GPU: {torch.cuda.get_device_name(device)}", file=sys.stderr)
                prepare(Path(request["input"]), Path(request["output"]),
                        request["sample_rate"], model, device)
            print("DJCRAFT_DONE", flush=True)
        except Exception:
            traceback.print_exc(file=sys.stderr)
            print("DJCRAFT_ERROR", flush=True)


if __name__ == "__main__":
    if "--download" in sys.argv:
        parser = argparse.ArgumentParser()
        parser.add_argument("--download", choices=MODELS, required=True)
        args = parser.parse_args()
        load_model(args.download)
        print("Model downloaded and verified", flush=True)
    else:
        parser = argparse.ArgumentParser()
        parser.add_argument("--serve", action="store_true", required=True)
        parser.add_argument("--device", choices=["rocm"], required=True)
        parser.add_argument("--model", choices=MODELS, required=True)
        args = parser.parse_args()

        def heartbeat():
            while True:
                time.sleep(10)
                print("DJCRAFT_BUSY", flush=True)

        threading.Thread(target=heartbeat, daemon=True).start()
        serve(args.model)
