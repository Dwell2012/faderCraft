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


def select_device(preference):
    if preference == "cpu":
        return "cpu"
    try:
        if torch.cuda.is_available():
            # Probe allocation too: a detected card can still have a broken driver.
            torch.empty(1, device="cuda")
            torch.set_float32_matmul_precision("high")
            print(f"Stem GPU: {torch.cuda.get_device_name(0)}", file=sys.stderr, flush=True)
            return "cuda"
        if hasattr(torch.backends, "mps") and torch.backends.mps.is_available():
            torch.empty(1, device="mps")
            return "mps"
    except RuntimeError as error:
        print(f"GPU unavailable; using CPU: {error}", file=sys.stderr, flush=True)
    return "cpu"


def separate(model, wave, device, model_name="htdemucs"):
    def run(selected):
        print(f"Stem device: {selected}; CPU threads: {torch.get_num_threads()}",
              file=sys.stderr, flush=True)
        # Keep whole-track buffers in system RAM; Demucs transfers only chunks.
        started = time.perf_counter()
        model.to(selected)
        result = apply_model(model, wave, device=selected, shifts=0, split=True,
                             overlap=0.25, progress=True, num_workers=0)[0]
        print(f"Stem inference: {time.perf_counter() - started:.3f}s ({selected})",
              file=sys.stderr, flush=True)
        return result

    try:
        return run(device)
    except RuntimeError as error:
        if device == "cpu":
            raise
        print(f"GPU separation failed; retrying on CPU: {error}", file=sys.stderr, flush=True)
    try:
        model.cpu()
    except RuntimeError:
        # A lost CUDA context may no longer allow copying weights off the GPU.
        model = load_model(model_name)
    if device == "cuda":
        try:
            torch.cuda.empty_cache()
        except RuntimeError:
            pass
    torch.manual_seed(0)
    return run("cpu")


def prepare(input_path, output_dir, sample_rate, preference="auto", model=None, device=None, model_name="htdemucs"):
    def report(percent):
        print(f"DJCRAFT_PROGRESS {percent}", flush=True)

    report(0)
    torch.set_num_threads(max(1, min(4, (os.cpu_count() or 2) // 2)))
    torch.manual_seed(0)
    device = device or select_device(preference)
    model = model if model is not None else load_model(model_name)
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
        separated = separate(model, ((wave - mean) / std)[None], device, model_name)
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


def serve(preference, model_name="htdemucs"):
    """One model and process for the application lifetime; stdin EOF shuts down."""
    torch.set_num_threads(max(1, min(4, (os.cpu_count() or 2) // 2)))
    device = select_device(preference)
    model = load_model(model_name)

    def warm(selected):
        # Exercise real inference, including CUDA kernels, before dismissing splash.
        with torch.inference_mode():
            separate(model, torch.zeros(1, 2, model.samplerate), selected, model_name)
        return selected

    warm(device)
    print("DJCRAFT_READY", flush=True)
    for line in sys.stdin:
        try:
            request = json.loads(line)
            requested_model = request.get("model", "htdemucs")
            if requested_model not in MODELS:
                raise ValueError("Unknown stem model")
            if requested_model != model_name:
                model.cpu()
                # Release the previous model before loading the next to bound VRAM.
                del model
                if device == "cuda":
                    torch.cuda.empty_cache()
                model_name = requested_model
                model = load_model(model_name)
            selected = select_device(request["device"])
            # Device changes are applied per job; weights remain cached in process.
            with open(request["log"], "w", encoding="utf-8") as log, contextlib.redirect_stderr(log):
                prepare(Path(request["input"]), Path(request["output"]),
                        request["sample_rate"], request["device"], model, selected, model_name)
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
    elif "--serve" in sys.argv:
        parser = argparse.ArgumentParser()
        parser.add_argument("--serve", action="store_true")
        parser.add_argument("--device", choices=["auto", "cpu"], default="auto")
        parser.add_argument("--model", choices=MODELS, default="htdemucs")
        args = parser.parse_args()
        # Downloads can exceed a startup timeout; report liveness while they run.
        def heartbeat():
            while True:
                time.sleep(10)
                print("DJCRAFT_BUSY", flush=True)
        threading.Thread(target=heartbeat, daemon=True).start()
        serve(args.device, args.model)
    else:
        parser = argparse.ArgumentParser()
        parser.add_argument("input", type=Path)
        parser.add_argument("output", type=Path)
        parser.add_argument("sample_rate", type=int)
        parser.add_argument("--device", choices=["auto", "cpu"], default="auto")
        parser.add_argument("--model", choices=MODELS, default="htdemucs")
        args = parser.parse_args()
        prepare(args.input, args.output, args.sample_rate, args.device, model_name=args.model)
