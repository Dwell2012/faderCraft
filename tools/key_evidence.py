"""Exploratory pitch evidence, independent of faderCraft's key-profile matcher.

Requires NumPy and mono little-endian float32 from the key_pcm example.
Usage: python tools/key_evidence.py input.f32 new-output-directory [sample-rate]
       python tools/key_evidence.py --self-test
Scores are not probabilities. This does not determine a definitive musical key.
"""
import csv
from pathlib import Path
import sys

import numpy as np

NOTES = ("C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B")
FRAME, HOP = 16384, 4096


def peaks(samples, rate):
    size = len(samples)
    amplitude = np.abs(np.fft.rfft(samples * np.hanning(size)))
    frequency = np.fft.rfftfreq(size, 1 / rate)
    strongest = amplitude.max()
    bins = np.where((amplitude[1:-1] > amplitude[:-2])
                    & (amplitude[1:-1] >= amplitude[2:]))[0] + 1
    bins = bins[(frequency[bins] >= 65) & (frequency[bins] < 2500)
                & (amplitude[bins] > strongest * 0.025)]
    result = []
    for index in bins:
        a, b, c = np.log(np.maximum(amplitude[index - 1:index + 2], 1e-12))
        bend = a - 2 * b + c
        offset = np.clip(0.5 * (a - c) / bend, -0.5, 0.5) if bend < 0 else 0
        hz = (index + offset) * rate / size
        midi = 69 + 12 * np.log2(hz / 440)
        note = round(midi)
        cents = 100 * (midi - note)
        if abs(cents) <= 30:
            result.append((note, hz, float(amplitude[index] / strongest), cents))
    return result


def independent(peaks):
    """Exclude likely upper harmonics; genuine octave/fifth notes may also go.

    This heuristic neither separates instruments nor establishes chord roots.
    """
    result = []
    for peak in peaks:
        _, hz, level, _ = peak
        explained = False
        for _, lower, lower_level, _ in peaks:
            harmonic = round(hz / lower)
            if (lower < hz / 1.8 and lower_level > 0.35 * level
                    and 2 <= harmonic <= 8
                    and abs(1200 * np.log2(hz / (lower * harmonic))) < 25):
                explained = True
                break
        if not explained:
            result.append(peak)
    return result


def self_test():
    rate = 22050
    times = np.arange(FRAME) / rate
    for third, expected in [(55, 7), (56, 8)]:
        signal = np.zeros(FRAME)
        for midi in [52, third, 59]:
            hz = 440 * 2 ** ((midi - 69) / 12)
            for harmonic in range(1, 5):
                signal += np.sin(2 * np.pi * hz * harmonic * times) / harmonic
        detected = {peak[0] % 12 for peak in independent(peaks(signal, rate))}
        assert {4, expected, 11} <= detected, detected
        assert (8 if expected == 7 else 7) not in detected, detected
    assert peaks(np.zeros(FRAME), rate) == []
    print("Synthetic E-minor/E-major harmonic controls and silence passed")


def main():
    if sys.argv[1:] == ["--self-test"]:
        self_test()
        return
    if len(sys.argv) not in (3, 4):
        raise SystemExit(__doc__)
    samples = np.fromfile(sys.argv[1], dtype="<f4")
    rate = int(sys.argv[3]) if len(sys.argv) == 4 else 22050
    if rate <= 0 or not np.isfinite(samples).all():
        raise SystemExit("Expected a positive sample rate and finite PCM")
    output = Path(sys.argv[2])
    output.mkdir(exist_ok=False)
    scores = np.zeros((int(len(samples) / rate // 30) + 1, 12))
    with (output / "independent-peaks.csv").open("w", newline="") as stream:
        writer = csv.writer(stream)
        writer.writerow(("seconds", "midi", "hz", "relative_level", "cents"))
        for start in range(0, len(samples) - FRAME + 1, HOP):
            frame = independent(peaks(samples[start:start + FRAME], rate))
            melodic = sorted((p for p in frame if 180 < p[1] < 1000),
                             key=lambda p: p[2], reverse=True)[:3]
            seconds = start / rate
            for note, hz, level, cents in melodic:
                writer.writerow((seconds, note, hz, level, cents))
                scores[int(seconds // 30), note % 12] += level
    for segment, values in enumerate(scores):
        end = min((segment + 1) * 30, len(samples) / rate)
        ranking = np.argsort(values)[-5:][::-1]
        print(f"{segment * 30:3}-{end:.1f}s: "
              + ", ".join(f"{NOTES[n]} {values[n]:.1f}" for n in ranking))


if __name__ == "__main__":
    main()
