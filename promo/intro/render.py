#!/usr/bin/env python3
"""Render the intro without Node (Linux): the same as render.mjs, using only
Python, Chrome and ffmpeg. Chrome draws intro.html frame by frame and also
synthesizes the soundtrack (audio.mjs runs in the page).

    python3 render.py            -> righelm-intro.mp4 (1080p60)

CHROME picks the browser (default: google-chrome).
"""
import base64
import http.client
import json
import os
import pathlib
import random
import shutil
import socket
import struct
import subprocess
import sys
import tempfile
import time
import wave

HERE = pathlib.Path(__file__).resolve().parent
OUT = HERE / "righelm-intro.mp4"
WAV = HERE / "righelm-intro.wav"


class WebSocket:
    """Just enough of a WebSocket client for the DevTools protocol."""

    def __init__(self, url):
        rest = url.split("://", 1)[1]
        hostport, path = rest.split("/", 1)
        host, port = hostport.split(":")
        self.sock = socket.create_connection((host, int(port)))
        key = base64.b64encode(os.urandom(16)).decode()
        self.sock.sendall(
            f"GET /{path} HTTP/1.1\r\nHost: {hostport}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n"
            f"Sec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n".encode()
        )
        head = b""
        while b"\r\n\r\n" not in head:
            head += self.sock.recv(1)
        if b" 101 " not in head.split(b"\r\n", 1)[0]:
            raise RuntimeError(head.decode(errors="replace"))

    def _recv(self, n):
        buf = bytearray()
        while len(buf) < n:
            chunk = self.sock.recv(n - len(buf))
            if not chunk:
                raise ConnectionError("DevTools closed the connection")
            buf += chunk
        return bytes(buf)

    def send(self, text):
        data = text.encode()
        head = bytearray([0x81])
        n = len(data)
        if n < 126:
            head.append(0x80 | n)
        elif n < 65536:
            head.append(0x80 | 126)
            head += struct.pack(">H", n)
        else:
            head.append(0x80 | 127)
            head += struct.pack(">Q", n)
        mask = os.urandom(4)
        head += mask
        self.sock.sendall(bytes(head) + bytes(b ^ mask[i % 4] for i, b in enumerate(data)))

    def recv(self):
        message = bytearray()
        while True:
            b0, b1 = self._recv(2)
            n = b1 & 0x7F
            if n == 126:
                n = struct.unpack(">H", self._recv(2))[0]
            elif n == 127:
                n = struct.unpack(">Q", self._recv(8))[0]
            payload = self._recv(n)
            opcode = b0 & 0x0F
            if opcode == 0x9:  # ping
                continue
            message += payload
            if b0 & 0x80:
                return message.decode()


class DevTools:
    def __init__(self, url):
        self.ws = WebSocket(url)
        self.next_id = 0

    def call(self, method, **params):
        self.next_id += 1
        self.ws.send(json.dumps({"id": self.next_id, "method": method, "params": params}))
        while True:
            m = json.loads(self.ws.recv())
            if m.get("id") == self.next_id:
                if "error" in m:
                    raise RuntimeError(m["error"])
                return m["result"]

    def evaluate(self, expression):
        r = self.call("Runtime.evaluate", expression=expression, returnByValue=True, awaitPromise=True)
        if "exceptionDetails" in r:
            raise RuntimeError(r["exceptionDetails"].get("exception", {}).get("description", r["exceptionDetails"]))
        return r["result"].get("value")


def audio_source():
    """audio.mjs as a plain script whose renderAudio returns base64 PCM
    (interleaved 16-bit stereo) instead of a Node Buffer."""
    src = (HERE / "audio.mjs").read_text().replace("export function renderAudio", "function renderAudio")
    start = src.index("  const wav = Buffer.alloc")
    end = src.index("  return wav;", start) + len("  return wav;")
    pcm = """  const pcm = new Int16Array(N * 2);
  for (let n = 0; n < N; n++) {
    pcm[2 * n] = Math.round(clamp(out[0][n] * norm, -1, 1) * 32767);
    pcm[2 * n + 1] = Math.round(clamp(out[1][n] * norm, -1, 1) * 32767);
  }
  const bytes = new Uint8Array(pcm.buffer);
  let s = '';
  for (let i = 0; i < bytes.length; i += 0x8000) s += String.fromCharCode.apply(null, bytes.subarray(i, i + 0x8000));
  return btoa(s);"""
    return src[:start] + pcm + src[end:]


def main():
    chrome = os.environ.get("CHROME", "google-chrome")
    port = 9300 + random.randrange(500)
    profile = tempfile.mkdtemp(prefix="righelm-intro-")
    page = (HERE / "intro.html").as_uri() + "?render"
    proc = subprocess.Popen(
        [chrome, "--headless=new", f"--remote-debugging-port={port}", f"--user-data-dir={profile}", "--window-size=1920,1080",
         "--hide-scrollbars", "--force-device-scale-factor=1", "--no-first-run", "--no-default-browser-check", "--mute-audio", page],
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
    )
    try:
        target = None
        for _ in range(100):
            time.sleep(0.1)
            try:
                c = http.client.HTTPConnection("127.0.0.1", port, timeout=1)
                c.request("GET", "/json/list")
                targets = json.loads(c.getresponse().read())
                target = next((t for t in targets if t["type"] == "page" and t["url"].startswith("file:")), None)
                if target:
                    break
            except OSError:
                pass
        if not target:
            raise RuntimeError("Chrome did not come up")
        dt = DevTools(target["webSocketDebuggerUrl"])
        for _ in range(100):
            if dt.evaluate('typeof window.renderAt === "function"'):
                break
            time.sleep(0.1)
        dt.evaluate("document.fonts.ready.then(() => true)")
        tl = dt.evaluate("({fps: TIMELINE.FPS, duration: TIMELINE.DURATION})")

        # Soundtrack.
        dt.evaluate(audio_source() + "\n;true")
        pcm = base64.b64decode(dt.evaluate("renderAudio(TIMELINE)"))
        with wave.open(str(WAV), "wb") as w:
            w.setnchannels(2)
            w.setsampwidth(2)
            w.setframerate(48000)
            w.writeframes(pcm)
        print("audio ->", WAV)

        # Picture.
        ff = subprocess.Popen(
            ["ffmpeg", "-y", "-loglevel", "error", "-f", "image2pipe", "-framerate", str(tl["fps"]), "-c:v", "png", "-i", "-",
             "-i", str(WAV), "-c:v", "libx264", "-preset", "slow", "-crf", "15", "-pix_fmt", "yuv420p", "-tune", "film",
             "-c:a", "aac", "-b:a", "256k", "-movflags", "+faststart", "-shortest", str(OUT)],
            stdin=subprocess.PIPE,
        )
        total = round(tl["duration"] * tl["fps"])
        for f in range(total):
            url = dt.evaluate(f"renderAt({f / tl['fps']}); document.getElementById('c').toDataURL('image/png')")
            ff.stdin.write(base64.b64decode(url[url.index(",") + 1:]))
            if f % 60 == 0:
                print(f"\rframe {f}/{total}", end="", flush=True)
        ff.stdin.close()
        if ff.wait() != 0:
            raise RuntimeError("ffmpeg failed")
        print(f"\nvideo -> {OUT}")
    finally:
        proc.terminate()
        proc.wait()
        shutil.rmtree(profile, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
