#!/usr/bin/env python3
"""Soundtrack for the band scope video, synthesized to fit the recording.

    python3 soundtrack.py <events.txt> <video.mp4> <out.wav>

A calm synth bed in A minor (pads, bass, arpeggio, a light pulse), a whoosh
under the title and end cards, a soft tick on every click and scroll, and a
little CW after the CW zero-beat. The events file (written by RigHelm's demo
mode) says when things happened: "<seconds> click|scroll|caption|card <kind>".
"""
import subprocess
import sys
import wave

import numpy as np

SR = 48_000
rng = np.random.default_rng(73)


def duration(video):
    out = subprocess.run(["ffprobe", "-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0", video], capture_output=True, text=True, check=True)
    return float(out.stdout.strip())


def events(path):
    ev = []
    for line in open(path):
        parts = line.split()
        if len(parts) >= 2:
            ev.append((float(parts[0]), parts[1], parts[2] if len(parts) > 2 else ""))
    return ev


def lowpass(x, cutoff, order=2):
    """Gentle low-pass in the frequency domain (whole signal at once)."""
    f = np.fft.rfftfreq(len(x), 1 / SR)
    h = 1 / np.sqrt(1 + (f / cutoff) ** (2 * order))
    return np.fft.irfft(np.fft.rfft(x) * h, len(x))


def highpass(x, cutoff, order=2):
    f = np.fft.rfftfreq(len(x), 1 / SR)
    h = 1 / np.sqrt(1 + (cutoff / np.maximum(f, 1e-3)) ** (2 * order))
    return np.fft.irfft(np.fft.rfft(x) * h, len(x))


def saw(freq, n, phase=0.0):
    t = np.arange(n) / SR
    return 2 * ((freq * t + phase) % 1.0) - 1


def place(buf, at, sound, gain=1.0):
    i = int(at * SR)
    if i >= len(buf):
        return
    j = min(len(buf), i + len(sound))
    buf[i:j] += sound[: j - i] * gain


def note(name):
    names = {"C": -9, "D": -7, "E": -5, "F": -4, "G": -2, "A": 0, "B": 2}
    return 440.0 * 2 ** ((names[name[0]] + 12 * (int(name[1:]) - 4)) / 12)


def main():
    ev_path, video, out = sys.argv[1:4]
    dur = duration(video)
    ev = events(ev_path)
    n = int(dur * SR) + SR
    t = np.arange(n) / SR

    beat = 0.6  # 100 bpm
    bar = 4 * beat
    chords = [  # Am, F, C, G
        (["A3", "C4", "E4"], "A2"),
        (["F3", "A3", "C4"], "F2"),
        (["C4", "E4", "G4"], "C3"),
        (["G3", "B3", "D4"], "G2"),
    ]

    # Pads: detuned saws per chord, crossfaded bar to bar, then softened.
    pad = np.zeros(n)
    bass = np.zeros(n)
    k = 0
    start = 0.0
    while start < dur + bar:
        tones, root = chords[k % 4]
        i0, i1 = int(start * SR), int(min(n, (start + bar + 0.5) * SR))
        seg = i1 - i0
        if seg <= 0:
            break
        env = np.ones(seg)
        fade = min(int(0.5 * SR), seg // 2)
        env[:fade] = np.linspace(0, 1, fade)
        env[-fade:] = np.minimum(env[-fade:], np.linspace(1, 0, fade))
        s = np.zeros(seg)
        for tn in tones:
            f = note(tn)
            for cents in (-6, 0, 7):
                s += saw(f * 2 ** (cents / 1200), seg, rng.random())
        pad[i0:i1] += s * env / 9
        fb = note(root)
        b = np.sin(2 * np.pi * fb * np.arange(seg) / SR) + 0.3 * np.sin(4 * np.pi * fb * np.arange(seg) / SR)
        bass[i0:i1] += b * env
        start += bar
        k += 1
    # A slow filter sweep breathes with the music.
    pad = lowpass(pad, 900) * (0.85 + 0.15 * np.sin(2 * np.pi * t / (4 * bar)))
    bass = lowpass(bass, 180)

    # Arpeggio: plucks on eighth notes through the chord, up an octave.
    arp = np.zeros(n)
    pluck_len = int(0.45 * SR)
    pt = np.arange(pluck_len) / SR
    step = beat / 2
    s = 0.0
    idx = 0
    while s < dur:
        tones, _ = chords[int(s // bar) % 4]
        order = [0, 1, 2, 1, 2, 0, 2, 1]
        f = note(tones[order[idx % 8]]) * 2
        p = (np.sin(2 * np.pi * f * pt) + 0.25 * np.sin(4 * np.pi * f * pt)) * np.exp(-pt * 7)
        p[:200] *= np.linspace(0, 1, 200)
        place(arp, s, p, 0.9 if idx % 2 == 0 else 0.6)
        s += step
        idx += 1

    # Pulse: a soft low thump on each beat, and airy ticks between.
    pulse = np.zeros(n)
    hat = np.zeros(n)
    kl = int(0.35 * SR)
    kt = np.arange(kl) / SR
    kick = np.sin(2 * np.pi * (48 + 60 * np.exp(-kt * 25)) * kt) * np.exp(-kt * 9)
    hl = int(0.06 * SR)
    noise = highpass(rng.standard_normal(hl * 4), 7000)[:hl] * np.exp(-np.arange(hl) / SR * 70)
    s = 0.0
    while s < dur:
        place(pulse, s, kick)
        place(hat, s + beat / 2, noise)
        s += beat

    # Sections: quiet under the title, full in the middle, out with the end card.
    def ramp(points):
        xs, ys = zip(*points)
        return np.interp(t, xs, ys)

    end_card = max([e[0] for e in ev if e[1] == "card"] + [dur - 5])
    intro = 4.4
    music = (
        pad * ramp([(0, 0.0), (0.8, 0.5), (intro, 0.6), (end_card, 0.6), (dur, 0.0)])
        + bass * ramp([(0, 0.0), (intro - 0.6, 0.0), (intro + 1, 0.45), (end_card, 0.45), (dur - 0.5, 0.0)])
        + arp * ramp([(0, 0.0), (intro, 0.0), (intro + 2, 0.16), (end_card - 1, 0.16), (end_card + 2, 0.06), (dur, 0.0)])
        + pulse * ramp([(0, 0.0), (intro + 5, 0.0), (intro + 7, 0.35), (end_card - 0.5, 0.35), (end_card + 0.5, 0.0)])
        + hat * ramp([(0, 0.0), (intro + 5, 0.0), (intro + 7, 0.05), (end_card - 0.5, 0.05), (end_card + 0.5, 0.0)])
    )

    sfx = np.zeros(n)
    # Whoosh under the cards.
    wl = int(1.6 * SR)
    w = lowpass(rng.standard_normal(wl), 2500) * np.sin(np.linspace(0, np.pi, wl)) ** 2
    # A soft tick for clicks, a smaller one for scroll steps.
    cl = int(0.05 * SR)
    ct = np.arange(cl) / SR
    tick = (np.sin(2 * np.pi * 1800 * ct) * 0.6 + np.sin(2 * np.pi * 3100 * ct) * 0.3) * np.exp(-ct * 120)
    for at, kind, arg in ev:
        if kind == "card":
            place(sfx, max(0.0, at - 0.3), w, 0.22)
        elif kind == "click":
            place(sfx, at, tick, 0.35)
        elif kind == "scroll":
            place(sfx, at, tick, 0.18)

    # CW after the CW zero-beat: the station calling CQ, at the pitch.
    # (The zero-beat click is the last one before the first zoom scroll.)
    scrolls = [at for at, kind, _ in ev if kind == "scroll"]
    before = [at for at, kind, _ in ev if kind == "click" and scrolls and at < scrolls[0]]
    cw_at = before[-1] + 0.7 if before else None
    if cw_at is not None:
        unit = 0.06  # 20 WPM
        code = {"C": "-.-.", "Q": "--.-", "D": "-..", "E": "."}
        s = cw_at
        for word in ["CQ", "CQ", "DE"]:
            for ch in word:
                for sym in code[ch]:
                    ln = unit * (3 if sym == "-" else 1)
                    m = int(ln * SR)
                    mt = np.arange(m) / SR
                    tone = np.sin(2 * np.pi * 700 * mt)
                    edge = min(int(0.005 * SR), m // 2)
                    tone[:edge] *= np.linspace(0, 1, edge)
                    tone[-edge:] *= np.linspace(1, 0, edge)
                    place(sfx, s, tone, 0.10)
                    s += ln + unit
                s += 2 * unit
            s += 4 * unit

    mix = music * 0.55 + sfx
    mix = np.tanh(mix * 1.2) / np.tanh(1.2)  # gentle limiting
    mix = mix[: int(dur * SR)]
    mix *= 0.89 / max(1e-9, np.max(np.abs(mix)))
    # A little width: the arpeggio leans right, its echo left.
    left = mix + np.roll(arp[: len(mix)] * 0.03, int(0.18 * SR))
    right = mix + arp[: len(mix)] * 0.02
    st = np.stack([left, right], axis=1)
    st *= 0.89 / max(1e-9, np.max(np.abs(st)))
    data = (st * 32767).astype("<i2").tobytes()
    with wave.open(out, "wb") as f:
        f.setnchannels(2)
        f.setsampwidth(2)
        f.setframerate(SR)
        f.writeframes(data)
    print("audio ->", out, f"{dur:.1f} s")


if __name__ == "__main__":
    main()
