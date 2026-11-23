#!/usr/bin/env python3
"""Execute the installer's generated launcher without packages or a compositor."""
import os
from pathlib import Path
import signal
import socket
import subprocess
import tempfile

repo = Path(__file__).resolve().parents[1]
with tempfile.TemporaryDirectory() as directory:
    root = Path(directory)
    commands = root / 'bin'
    commands.mkdir()
    runtime = root / 'runtime'
    runtime.mkdir()
    wayland = socket.socket(socket.AF_UNIX)
    wayland.bind(str(runtime / 'wayland-0'))
    scripts = {
        'curl': 'exit 0',
        'cage': 'while [ "$1" != "--" ]; do shift; done; shift; exec "$@"',
        'swayidle': 'exec sleep 10',
        'oxide-kiosk-idle-watcher': 'exec sleep 10',
        'browser': 'printf "%s\\n" "$@" >> "$BROWSER_ARGS"; sleep 0.2; exit 23',
    }
    for name, body in scripts.items():
        path = commands / name
        path.write_text('#!/bin/sh\n' + body + '\n')
        path.chmod(0o755)
    for mode, port, expected in [
        ('full', '8000', 'http://127.0.0.1:8000/kiosk?panel=1&idle=600'),
        ('viz', '80', 'http://127.0.0.1/viz.html'),
    ]:
        args_file = root / f'{mode}-args'
        env = dict(os.environ, OXIDE_INSTALLER_TEST='1', BIN_DIR=str(commands),
                   LISTEN=f'127.0.0.1:{port}', KIOSK_MODE=mode,
                   KIOSK_IDLE_SECONDS='600', XDG_RUNTIME_DIR=str(runtime),
                   OXIDE_KIOSK_BROWSER=str(commands / 'browser'),
                   BROWSER_ARGS=str(args_file), PATH=f'{commands}:{os.environ["PATH"]}')
        rendered = subprocess.check_output(
            ['bash', '-c', 'installer=$1; shift; source "$installer"; render_kiosk_session', 'render', str(repo / 'install.sh')],
            env=env)
        session = root / f'{mode}-session'
        session.write_bytes(rendered)
        subprocess.run(['sh', '-n', str(session)], check=True)
        child = subprocess.Popen(['sh', str(session)], env=env, start_new_session=True,
                                 stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        try:
            try:
                result = child.wait(timeout=5)
            except subprocess.TimeoutExpired:
                raise AssertionError(f'{mode}: launcher did not supervise browser exit')
            arguments = args_file.read_text().splitlines()
            assert result == 23, f'{mode}: browser exit was lost: {result}'
            assert arguments == [
                '--enable-features=UseOzonePlatform', '--ozone-platform=wayland',
                '--kiosk', '--noerrdialogs', '--disable-dev-shm-usage',
                '--password-store=basic', expected,
            ], f'{mode}: browser received incorrect arguments: {arguments}'
        finally:
            try:
                os.killpg(child.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            child.wait()
    wayland.close()
print('generated kiosk launcher preserves URL and browser exit in full/viz modes')
