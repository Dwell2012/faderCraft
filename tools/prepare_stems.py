"""DJCraft's optional local Demucs worker. Input/output: stereo float32 LE PCM.

No music leaves this machine. Only missing model weights are downloaded.
"""
import argparse
from pathlib import Path
import os
from types import SimpleNamespace

import numpy as np
import torch
import torchaudio.functional as audio
from demucs.apply import apply_model
from demucs.pretrained import get_model
import demucs.apply as demucs_apply


def prepare(input_path, output_dir, sample_rate):
    def report(percent):
        print(f"DJCRAFT_PROGRESS {percent}", flush=True)

    report(0)
    torch.set_num_threads(max(1, min(4, (os.cpu_count() or 2) // 2)))
    torch.manual_seed(0)
    model = get_model("htdemucs").cpu().eval()
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
        separated = apply_model(model, ((wave - mean) / std)[None],
                                device="cpu", shifts=0, split=True,
                                overlap=0.25, progress=True, num_workers=0)[0]
        separated = separated * std + mean
        if sample_rate != model.samplerate:
            separated = audio.resample(separated, model.samplerate, sample_rate)
    output_dir.mkdir(parents=True, exist_ok=True)
    for index, (name, source) in enumerate(zip(model.sources, separated)):
        source = source[:, :frames]
        if source.shape[-1] < frames:
            source = torch.nn.functional.pad(source, (0, frames - source.shape[-1]))
        result = source.T.contiguous().numpy().astype("<f4")
        if not np.isfinite(result).all():
            raise ValueError("Separator produced invalid samples")
        result.tofile(output_dir / (name + ".f32"))
        report(96 + index)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("input", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("sample_rate", type=int)
    args = parser.parse_args()
    prepare(args.input, args.output, args.sample_rate)
