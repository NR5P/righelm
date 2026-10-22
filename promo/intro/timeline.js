// Shared timing for the RigHelm intro: the page (intro.html) draws from it and
// render.mjs builds the soundtrack from it, so picture and sound stay in sync.
//
// The helm is the VFO: every frequency in the intro comes from the wheel's angle.
(function (g) {
  const FPS = 60;
  const DURATION = 10.6;

  const UNIT = 0.04; // 30 WPM
  const MORSE = {
    R: '.-.', I: '..', G: '--.', H: '....', E: '.', L: '.-..', M: '--', C: '-.-.', Q: '--.-',
  };

  // Morse for `text` starting at `t`; returns key-down intervals and per-letter timing.
  function morse(text, t) {
    const keys = [];
    const chars = [];
    for (let i = 0; i < text.length; i++) {
      if (text[i] === ' ') { t += 4 * UNIT; continue; }
      const code = MORSE[text[i]];
      const start = t;
      const els = [];
      for (let j = 0; j < code.length; j++) {
        const len = (code[j] === '.' ? 1 : 3) * UNIT;
        keys.push([t, t + len]);
        els.push({ sym: code[j], a: t, b: t + len });
        t += len;
        if (j < code.length - 1) t += UNIT;
      }
      chars.push({ ch: text[i], code, start, end: t, els });
      t += 3 * UNIT;
    }
    return { keys, chars, end: t - 3 * UNIT };
  }

  const WORD = 'RIGHELM';
  // The station is already calling CQ while you steer onto it.
  const cq = morse('CQ CQ', 0.25);
  const callKeys = cq.keys.filter(([a]) => a < 1.75);
  const main = morse(WORD, 2.05);
  const keys = callKeys.concat(main.keys);
  const chars = main.chars;

  const T = {
    spinStart: 0.25,
    spinEnd: 1.45,
    settleEnd: 1.85,
    morseStart: 2.05,
    morseEnd: main.end,
    riseStart: 4.55,
    impact: 5.2,
    spin2End: 5.55,
    wordStart: 5.75,
    letterStep: 0.06,
    tagline: 6.25,
    chips: 6.6,
    chipStep: 0.1,
    shine: 7.3,
    fadeOut: 9.9,
  };

  // One full turn of the helm moves the VFO 10 kHz.
  const HZ_PER_TURN = 10000;
  const F0 = 14010000;
  const F1 = 14025000; // the station
  const OVERSHOOT = 400; // steer past it, then correct back onto it
  const TAU = Math.PI * 2;
  const clamp = (x, a = 0, b = 1) => Math.min(b, Math.max(a, x));
  const seg = (t, a, b) => clamp((t - a) / (b - a));
  const easeInOutCubic = (x) => (x < 0.5 ? 4 * x * x * x : 1 - Math.pow(-2 * x + 2, 3) / 2);
  const easeOutBack = (x, s = 1.70158) => 1 + (s + 1) * Math.pow(x - 1, 3) + s * Math.pow(x - 1, 2);

  const thetaPast = ((F1 + OVERSHOOT - F0) / HZ_PER_TURN) * TAU;
  const thetaOn = ((F1 - F0) / HZ_PER_TURN) * TAU; // 1.5 turns: lands on the station
  const thetaEnd = 2 * TAU; // the riser spins it on to upright for the logo

  // Wheel angle in radians, clockwise.
  function theta(t) {
    if (t < T.spinEnd) return thetaPast * easeInOutCubic(seg(t, T.spinStart, T.spinEnd));
    if (t < T.riseStart) return thetaPast + (thetaOn - thetaPast) * easeInOutCubic(seg(t, T.spinEnd, T.settleEnd));
    const spin = thetaOn + (thetaEnd - thetaOn) * easeOutBack(seg(t, T.riseStart + 0.1, T.spin2End), 1.2);
    return spin + Math.sin((t - 6.2) * 1.3) * 0.035 * seg(t, 6.2, 7.2);
  }
  function freqAt(t) {
    return Math.round((F0 + (theta(t) / TAU) * HZ_PER_TURN) / 10) * 10;
  }

  // A detent click every 15 degrees of travel.
  const DETENT = TAU / 24;
  const clicks = [];
  {
    let last = Math.floor(theta(0) / DETENT);
    for (let t = 0; t < DURATION; t += 0.0005) {
      const d = Math.floor(theta(t) / DETENT);
      if (d !== last) { clicks.push(t); last = d; }
    }
  }

  // How much of [t0, t1) the key is down.
  function keyCover(t0, t1) {
    let s = 0;
    for (const [a, b] of keys) s += Math.max(0, Math.min(b, t1) - Math.max(a, t0));
    return s;
  }

  // Receiver passband: how loud the station is with the dial at `dial`.
  function passband(dial) {
    const off = (dial - F1) / 330;
    return Math.exp(-0.5 * off * off);
  }

  const chipLabels = ['Linux · macOS · Windows', 'Written in Rust', 'LAN remote', 'rigctld server'];

  g.TIMELINE = {
    FPS, DURATION, UNIT, WORD, keys, chars, T, F0, F1, theta, freqAt, clicks, keyCover, passband, chipLabels,
  };
})(typeof window !== 'undefined' ? window : globalThis);
