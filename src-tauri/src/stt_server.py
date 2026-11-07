#!/usr/bin/env python3
"""whisp STT daemon: persistent faster-whisper sidecar.

Loads the model ONCE at startup, then serves transcriptions over a unix
socket so dictation never pays the model-load cost again.

Wire protocol (both directions): u32 big-endian length + payload bytes.

- Request payload ``b"__ping__"`` -> responds ``b"__pong__"`` (healthcheck).
- Otherwise: 1 byte language-name length N, N bytes utf-8 language
  (``"auto"``/empty = auto-detect), then WAV bytes (16 kHz mono s16).
- Response payload: utf-8 transcript text. Server-side failures are
  reported as ``b"__WHISP_ERROR__:" + message``.

VAD uses faster-whisper's built-in filter (same settings as the one-shot
sidecar). The daemon exits on its own when its parent process dies (ppid
watchdog) and removes the socket file, so it never outlives the app.
"""

import argparse
import os
import socket
import struct
import sys
import threading
import time
import wave

PING = b"__ping__"
PONG = b"__pong__"
MODEL_QUERY = b"__model__"
ERROR_PREFIX = b"__WHISP_ERROR__:"
# Taille max d'une frame (64 Mo : ~10 min de WAV 16k mono s16 = ~19 Mo,
# marge large ; protège contre un length corrompu qui allouerait des Go).
MAX_MSG = 64 * 1024 * 1024


def recv_exact(conn: socket.socket, n: int) -> bytes:
    buf = bytearray()
    while len(buf) < n:
        chunk = conn.recv(n - len(buf))
        if not chunk:
            raise ConnectionError("peer closed")
        buf += chunk
    return bytes(buf)


def read_frame(conn: socket.socket) -> bytes:
    (ln,) = struct.unpack(">I", recv_exact(conn, 4))
    if ln > MAX_MSG:
        raise ValueError(f"frame too large: {ln}")
    return recv_exact(conn, ln)


def write_frame(conn: socket.socket, payload: bytes) -> None:
    conn.sendall(struct.pack(">I", len(payload)) + payload)


def decode_wav_16k_mono(raw: bytes):
    import io

    import numpy as np

    with wave.open(io.BytesIO(raw), "rb") as w:
        if w.getnchannels() != 1 or w.getsampwidth() != 2 or w.getframerate() != 16000:
            raise ValueError("need 16k mono s16 wav")
        frames = w.readframes(w.getnframes())
    return np.frombuffer(frames, dtype=np.int16).astype(np.float32) / 32768.0


def handle(conn: socket.socket, model, model_name: str) -> None:
    try:
        payload = read_frame(conn)
    except (ConnectionError, ValueError):
        return
    if payload == PING:
        write_frame(conn, PONG)
        return
    if payload == MODEL_QUERY:
        write_frame(conn, model_name.encode("utf-8"))
        return
    if payload.startswith(b"__partial__"):
        try:
            audio = decode_wav_16k_mono(payload[len(b"__partial__"):])
            # Fast path: tiny model, greedy, no VAD (keep every word).
            segs, info = handle.tiny.transcribe(audio, language=handle.tiny_lang, beam_size=1)
            if handle.tiny_lang is None and info.language_probability > 0.5:
                handle.tiny_lang = info.language
            write_frame(conn, ("".join(s.text for s in segs)).strip().encode("utf-8"))
        except Exception as e:
            write_frame(conn, ERROR_PREFIX + str(e).encode("utf-8"))
        return
    try:
        if len(payload) < 1:
            raise ValueError("empty request")
        ln = payload[0]
        language = payload[1 : 1 + ln].decode("utf-8")
        audio = decode_wav_16k_mono(payload[1 + ln :])
        lang = None if language in ("auto", "") else language
        # Sticky language: full auto-detect costs ~2 s on CPU (a 30 s
        # window each call). Dictation is monolingual, so detect once and
        # reuse the winner (`fr` here with p=0.96). Re-detect every 5 min
        # in case the user switched languages.
        lang = None if language in ("auto", "") else language
        use_lang = lang if lang else handle.last_lang
        now = time.monotonic()
        if lang is None and now - handle.last_detect > 300:
            use_lang = None
        segments, info = model.transcribe(
            audio,
            language=use_lang,
            # Greedy (beam 1) : ~20 % plus vite, qualité identique sur
            # dictée claire (mesuré : 7.3s vs 9.0s sur 7s FR, medium CPU).
            beam_size=1,
            vad_filter=True,
            vad_parameters=dict(min_silence_duration_ms=400),
        )
        text = "".join(s.text for s in segments)
        if lang is None and info.language_probability > 0.5:
            handle.last_lang = info.language
            handle.last_detect = now
        write_frame(conn, text.strip().encode("utf-8"))
    except Exception as e:  # noqa: BLE001 - must reply, never hang
        write_frame(conn, ERROR_PREFIX + str(e).encode("utf-8"))


handle.last_lang = None  # type: ignore[attr-defined]
handle.last_detect = 0.0  # type: ignore[attr-defined]


def _device() -> tuple:
    # CPU int8 verrouillé : ctranslate2 4.8.5 exige cublas 12, l'host a
    # CUDA 13 (libcublas.so.13) → toute tentative CUDA échoue au load
    # (mesuré : `libcublas.so.12 not found`). `small` CPU = 2.2 s / 7.8 s
    # audio, largement assez rapide — pas de GPU à gagner ici.
    return ("cpu", "int8")


def serve(sock_path: str, model_name: str) -> int:
    from faster_whisper import WhisperModel

    device, compute = _device()
    try:
        model = WhisperModel(model_name, device=device, compute_type=compute)
    except Exception as e:
        print(f"[whisp-stt] {device} failed ({e}), CPU fallback", flush=True)
        device, compute = "cpu", "int8"
        model = WhisperModel(model_name, device=device, compute_type=compute)
    # Realtime companion: tiny is ~10x faster, good enough for live previews.
    try:
        handle.tiny = WhisperModel("tiny", device=device, compute_type=compute)
    except Exception:
        handle.tiny = WhisperModel("tiny", device="cpu", compute_type="int8")
    handle.tiny_lang = None

    os.makedirs(os.path.dirname(sock_path) or ".", exist_ok=True)
    try:
        os.unlink(sock_path)
    except FileNotFoundError:
        pass
    srv = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    srv.bind(sock_path)
    srv.listen(8)
    print(f"[whisp-stt] ready model={model_name} sock={sock_path}", flush=True)

    while True:
        try:
            conn, _ = srv.accept()
        except OSError:
            break
        try:
            conn.settimeout(300.0)
            with conn:
                handle(conn, model, model_name)
        except OSError:
            continue
    return 0


def main() -> int:
    ap = argparse.ArgumentParser(description="whisp persistent STT sidecar")
    ap.add_argument("--model", default="small")
    ap.add_argument("--sock", default=os.path.expanduser("~/.cache/whisp/stt.sock"))
    args = ap.parse_args()

    start_ppid = os.getppid()

    def watchdog() -> None:
        # Die (and clean the socket) when the app that spawned us is gone.
        while True:
            time.sleep(5.0)
            if os.getppid() != start_ppid:
                try:
                    os.unlink(args.sock)
                except OSError:
                    pass
                os._exit(0)  # noqa: PGH090 - watchdog thread, must force exit

    threading.Thread(target=watchdog, name="ppid-watchdog", daemon=True).start()
    try:
        return serve(args.sock, args.model)
    except KeyboardInterrupt:
        return 0
    finally:
        try:
            os.unlink(args.sock)
        except OSError:
            pass


if __name__ == "__main__":
    sys.exit(main())
