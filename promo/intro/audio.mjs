// Soundtrack for the intro, synthesized from the same timeline as the picture:
// band hiss, dial detents, the CW itself, a riser, the impact and a pad.
const SR = 48000;

export function renderAudio(TL) {
  const T = TL.T;
  const N = Math.ceil(TL.DURATION * SR);
  const dry = [new Float32Array(N), new Float32Array(N)];
  const wet = [new Float32Array(N), new Float32Array(N)]; // sent through the reverb

  let seed = 0x2f6b1d3;
  const rnd = () => { seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0; return seed / 2147483648 - 1; };
  const clamp = (x, a = 0, b = 1) => Math.min(b, Math.max(a, x));
  const seg = (t, a, b) => clamp((t - a) / (b - a));
  const coef = (fc) => 1 - Math.exp(-2 * Math.PI * fc / SR);
  const at = (t) => Math.round(t * SR);

  // band hiss, a little different in each ear
  for (let ch = 0; ch < 2; ch++) {
    let lp1 = 0, lp2 = 0, hp = 0;
    const a1 = coef(2800), ah = coef(250);
    for (let n = 0; n < N; n++) {
      const t = n / SR;
      const x = rnd();
      lp1 += a1 * (x - lp1); lp2 += a1 * (lp1 - lp2);
      hp += ah * (lp2 - hp);
      const band = lp2 - hp;
      let g = 0.16 * seg(t, 0, 0.5);
      g += 0.12 * seg(t, T.riseStart - 0.2, T.impact);
      if (t >= T.impact) g = 0.05 * (1 - seg(t, T.fadeOut - 0.7, T.fadeOut + 0.5));
      dry[ch][n] += band * g;
    }
  }

  // dial detents while the VFO spins
  TL.clicks.forEach((tc, i) => {
    const n0 = at(tc), len = at(0.008);
    const pan = i % 2 ? 0.8 : 1;
    for (let n = 0; n < len && n0 + n < N; n++) {
      const e = Math.exp(-n / (0.0008 * SR));
      const s = (rnd() * 0.5 + Math.sin(2 * Math.PI * 2600 * n / SR) * 0.5 + Math.sin(2 * Math.PI * 900 * n / SR) * 0.4) * e * 0.3;
      dry[0][n0 + n] += s * pan; dry[1][n0 + n] += s * (1.8 - pan);
    }
  });

  // the CW: 650 Hz once you are on the station, sweeping in pitch and
  // through the passband as the helm tunes it; 5 ms raised-cosine edges
  const RAMP = 0.005;
  for (const [a, b] of TL.keys) {
    let ph = 0;
    for (let n = at(a); n < at(b + RAMP) && n < N; n++) {
      const t = n / SR;
      const rise = t < a + RAMP ? 0.5 - 0.5 * Math.cos(Math.PI * (t - a) / RAMP) : 1;
      const fall = t > b ? 0.5 + 0.5 * Math.cos(Math.PI * clamp((t - b) / RAMP)) : 1;
      const dial = TL.freqAt(t);
      ph += 2 * Math.PI * (650 + TL.F1 - dial) / SR;
      const tone = Math.sin(ph) + 0.04 * Math.sin(2 * ph);
      const s = 0.34 * rise * fall * TL.passband(dial) * tone;
      dry[0][n] += s; dry[1][n] += s;
      wet[0][n] += s * 0.35; wet[1][n] += s * 0.35;
    }
  }

  // riser into the impact
  {
    const t0 = T.riseStart - 0.1, t1 = T.impact;
    const lp = [0, 0], hpS = [0, 0];
    let ph = 0;
    for (let n = at(t0); n < at(t1); n++) {
      const x = seg(n / SR, t0, t1);
      const a = coef(250 * Math.pow(36, x));
      ph += 2 * Math.PI * 110 * Math.pow(2, 3 * x) / SR;
      const sw = Math.sin(ph) * 0.1 * x * x;
      for (let ch = 0; ch < 2; ch++) {
        const r = rnd();
        lp[ch] += a * (r - lp[ch]);
        hpS[ch] += coef(3000) * (r - hpS[ch]);
        const s = lp[ch] * 0.5 * Math.pow(x, 2.2) + (r - hpS[ch]) * 0.16 * Math.pow(x, 3) + sw;
        dry[ch][n] += s;
        wet[ch][n] += s * 0.4;
      }
    }
  }

  // impact: sub drop, crash and a transient
  {
    const n0 = at(T.impact);
    let ph = 0;
    const lp = [0, 0], hpS = [0, 0];
    for (let n = 0; n < at(2.2) && n0 + n < N; n++) {
      const tau = n / SR;
      ph += 2 * Math.PI * (36 + 60 * Math.exp(-tau / 0.07)) / SR;
      const boom = Math.sin(ph) * 1.1 * Math.exp(-tau / 0.6) * (1 - Math.exp(-tau / 0.002));
      const click = tau < 0.004 ? rnd() * 0.6 * (1 - tau / 0.004) : 0;
      for (let ch = 0; ch < 2; ch++) {
        const r = rnd();
        lp[ch] += coef(1200) * (r - lp[ch]);
        hpS[ch] += coef(2500) * (r - hpS[ch]);
        const crash = (r - hpS[ch]) * 0.35 * Math.exp(-tau / 0.3) + lp[ch] * 0.6 * Math.exp(-tau / 0.09);
        dry[ch][n0 + n] += boom + click + crash;
        wet[ch][n0 + n] += crash * 0.8 + boom * 0.15;
      }
    }
  }

  // pad: A major add9, bright on the hit then mellowing, swelling under the logo
  {
    const notes = [110, 164.81, 220, 277.18, 329.63, 493.88];
    const n0 = at(T.impact), n1 = N;
    const voices = [];
    notes.forEach((f, i) => { for (const d of [-1, 1]) voices.push({ f: f * Math.pow(2, d * 4 / 1200), pan: 0.5 + d * 0.3 * ((i % 2) * 2 - 1), ph: Math.abs(rnd()) * 6.28 }); });
    for (let n = n0; n < n1; n++) {
      const t = n / SR, tau = t - T.impact;
      const env = (0.55 * Math.exp(-tau / 0.35) + 0.45 * (1 - Math.exp(-tau / 0.8))) * (1 - seg(t, T.fadeOut - 0.3, TL.DURATION));
      const bright = 0.25 + 0.5 * Math.exp(-tau / 0.5);
      let l = 0, r = 0;
      for (const v of voices) {
        let s = 0;
        for (let h = 1; h <= 7; h++) s += Math.sin(v.ph * h) / h * Math.exp(-(h - 1) * (1 - bright) * 0.9);
        v.ph += 2 * Math.PI * v.f / SR;
        l += s * (1 - v.pan); r += s * v.pan;
      }
      const g = 0.055 * env;
      dry[0][n] += l * g; dry[1][n] += r * g;
      wet[0][n] += l * g * 0.6; wet[1][n] += r * g * 0.6;
    }
  }

  // a bell per letter of the wordmark
  {
    const notes = [440, 554.37, 659.25, 880, 1108.73, 1318.51, 1760];
    notes.forEach((f, i) => {
      const n0 = at(T.wordStart + i * T.letterStep);
      const pan = 0.25 + 0.5 * i / (notes.length - 1);
      for (let n = 0; n < at(1.6) && n0 + n < N; n++) {
        const tau = n / SR;
        const s = (Math.sin(2 * Math.PI * f * tau) + 0.35 * Math.sin(2 * Math.PI * f * 2.76 * tau) * Math.exp(-tau / 0.12)) * 0.07 * Math.exp(-tau / 0.7) * (1 - Math.exp(-tau / 0.002));
        dry[0][n0 + n] += s * (1 - pan); dry[1][n0 + n] += s * pan;
        wet[0][n0 + n] += s * (1 - pan); wet[1][n0 + n] += s * pan;
      }
    });
  }

  // chip pops
  TL.chipLabels.forEach((_, i) => {
    const n0 = at(T.chips + i * T.chipStep + 0.05);
    for (let n = 0; n < at(0.06) && n0 + n < N; n++) {
      const tau = n / SR;
      const s = Math.sin(2 * Math.PI * (1500 + 400 * i) * tau * (1 - tau * 4)) * 0.06 * Math.exp(-tau / 0.015);
      dry[0][n0 + n] += s; dry[1][n0 + n] += s;
      wet[0][n0 + n] += s; wet[1][n0 + n] += s;
    }
  });

  // airy whoosh with the shine sweep
  {
    const t0 = T.shine - 0.05, t1 = T.shine + 0.8;
    const lp = [0, 0], hpS = [0, 0];
    for (let n = at(t0); n < at(t1); n++) {
      const x = seg(n / SR, t0, t1);
      const env = Math.sin(Math.PI * x) * 0.07;
      for (let ch = 0; ch < 2; ch++) {
        const r = rnd();
        lp[ch] += coef(9000) * (r - lp[ch]);
        hpS[ch] += coef(2500 + 3000 * x) * (lp[ch] - hpS[ch]);
        const s = (lp[ch] - hpS[ch]) * env * (ch ? x : 1 - x) * 2;
        dry[ch][n] += s; wet[ch][n] += s;
      }
    }
  }

  // small Schroeder reverb on the send
  const out = [new Float32Array(N), new Float32Array(N)];
  for (let ch = 0; ch < 2; ch++) {
    const combs = [1557, 1617, 1491, 1422, 1277, 1356].map((d) => ({ buf: new Float32Array(d + ch * 23), i: 0, lp: 0 }));
    const aps = [556, 441, 341].map((d) => ({ buf: new Float32Array(d + ch * 23), i: 0 }));
    for (let n = 0; n < N; n++) {
      const x = wet[ch][n] * 0.15;
      let y = 0;
      for (const c of combs) {
        const o = c.buf[c.i];
        c.lp = o * 0.7 + c.lp * 0.3;
        c.buf[c.i] = x + c.lp * 0.84;
        c.i = (c.i + 1) % c.buf.length;
        y += o;
      }
      for (const a of aps) {
        const o = a.buf[a.i];
        a.buf[a.i] = y + o * 0.5;
        a.i = (a.i + 1) % a.buf.length;
        y = o - y * 0.5;
      }
      out[ch][n] = dry[ch][n] + y * 0.9;
    }
  }

  // master: fade, soft clip, normalize
  let peak = 0;
  for (let ch = 0; ch < 2; ch++) {
    for (let n = 0; n < N; n++) {
      const t = n / SR;
      const fade = 1 - seg(t, TL.DURATION - 0.35, TL.DURATION);
      const v = Math.tanh(out[ch][n] * 1.1) * fade;
      out[ch][n] = v;
      peak = Math.max(peak, Math.abs(v));
    }
  }
  const norm = 0.9 / peak;
  const wav = Buffer.alloc(44 + N * 4);
  wav.write('RIFF', 0); wav.writeUInt32LE(36 + N * 4, 4); wav.write('WAVE', 8);
  wav.write('fmt ', 12); wav.writeUInt32LE(16, 16); wav.writeUInt16LE(1, 20); wav.writeUInt16LE(2, 22);
  wav.writeUInt32LE(SR, 24); wav.writeUInt32LE(SR * 4, 28); wav.writeUInt16LE(4, 32); wav.writeUInt16LE(16, 34);
  wav.write('data', 36); wav.writeUInt32LE(N * 4, 40);
  for (let n = 0; n < N; n++) {
    wav.writeInt16LE(Math.round(clamp(out[0][n] * norm, -1, 1) * 32767), 44 + n * 4);
    wav.writeInt16LE(Math.round(clamp(out[1][n] * norm, -1, 1) * 32767), 46 + n * 4);
  }
  return wav;
}
