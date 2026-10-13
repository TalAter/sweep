#!/usr/bin/env python3
"""Help presentation in a real PTY, with disposable homes and no network."""
import ast, fcntl, os, pathlib, pty, re, select, struct, subprocess, sys, tempfile, termios, time

binary = str(pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else 'target/debug/sweep').resolve())

def check(name, width, height, overrides, color, animation, resize=False):
    with tempfile.TemporaryDirectory() as home:
        master, slave = pty.openpty()
        try:
            initial = termios.tcgetattr(slave)
            os.set_blocking(master, False)
            fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack('HHHH', height, width, 0, 0))
            env = {k: v for k, v in os.environ.items() if k not in ('NO_COLOR', 'FORCE_COLOR', 'CI', 'SWEEP_NO_ANIMATION', 'SWEEP_CONFIG')}
            env.update(SWEEP_HOME=home, SWEEP_THEME='dark', TERM='xterm-256color', COLORTERM='truecolor')
            env.update(overrides)
            process = subprocess.Popen([binary, '--help'], stdout=slave, stderr=subprocess.PIPE, env=env)
            data = b''
            deadline = time.monotonic() + 5
            while process.poll() is None:
                if time.monotonic() > deadline:
                    process.kill()
                    raise AssertionError((name, "help stalled"))
                if select.select([master], [], [], .1)[0]:
                    chunk = os.read(master, 65536)
                    if not chunk: break
                    data += chunk
                    if resize:
                        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack('HHHH', 10, 20, 0, 0))
            while select.select([master], [], [], .05)[0]:
                chunk = os.read(master, 65536)
                if not chunk: break
                data += chunk
            assert process.wait(timeout=3) == 0, name
            assert process.stderr.read() == b'', name
            assert (b'\x1b[' in data) == color, (name, data)
            movements = re.findall(rb'\x1b\[\d+A', data)
            if not resize:
                assert bool(movements) == animation, (name, data)
            assert b'Commands:' in data and b'Examples:' in data, name
            assert b'\x1b[?25l' not in data and b'\x1b[?1049h' not in data, name
            assert termios.tcgetattr(slave) == initial, name
            assert not os.listdir(home), name
            if name in ('truecolor shine', 'light theme shine'):
                first_frame = re.search(rb'\x1b\[\d+A', data)
                assert first_frame
                before = data[:first_frame.start()]
                assert screen(before) == screen(data), 'shine changed help contents or cursor position'
                if os.environ.get('SWEEP_HELP_CAPTURE'):
                    pathlib.Path(os.environ['SWEEP_HELP_CAPTURE'] + ('.light' if name == 'light theme shine' else '.dark')).write_bytes(before)
            print('PASS', name, flush=True)
        finally:
            os.close(master)
            os.close(slave)

def screen(data):
    rows = [[' '] * 110 for _ in range(45)]
    row = col = 0
    for token in re.findall(r'\x1b\[[0-9;]*[A-Za-z]|[^\x1b]', data.decode()):
        if token.startswith('\x1b['):
            code = token[-1]
            value = int(token[2:-1] or '1') if code in 'ABC' else 0
            if code == 'A': row -= value
            if code == 'B': row += value
            if code == 'C': col += value
        elif token == '\r': col = 0
        elif token == '\n': row += 1
        else:
            rows[row][col] = token
            col += 1
    return rows, row, col

check('truecolor shine', 110, 45, {}, True, True)
check('light theme shine', 110, 45, {'SWEEP_THEME': 'light'}, True, True)
check('false CI allows shine', 110, 45, {'CI': 'false'}, True, True)
check('CI static', 110, 45, {'CI': '1'}, True, False)
check('animation override', 110, 45, {'SWEEP_NO_ANIMATION': '1'}, True, False)
check('no color overrides forced color', 110, 45, {'NO_COLOR': '', 'FORCE_COLOR': '3'}, False, False)
check('basic color static', 110, 45, {'FORCE_COLOR': '1'}, True, False)
check('256 color static', 110, 45, {'FORCE_COLOR': '2'}, True, False)
check('short terminal static', 110, 10, {}, True, False)
check('narrow terminal static', 30, 45, {}, True, False)
check('unknown dimensions static', 0, 0, {}, True, False)
check('dumb terminal plain', 110, 45, {'TERM': 'dumb'}, False, False)
check('resize during shine', 110, 45, {}, True, False, resize=True)


def check_probe_signal(sig):
    import signal
    with tempfile.TemporaryDirectory() as home:
        master, slave = pty.openpty()
        process = None
        try:
            initial = termios.tcgetattr(slave)
            fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack('HHHH', 45, 110, 0, 0))
            env = {k: v for k, v in os.environ.items() if k not in ('NO_COLOR', 'FORCE_COLOR', 'SWEEP_THEME', 'SWEEP_CONFIG')}
            env.update(SWEEP_HOME=home, TERM='xterm-256color', COLORTERM='truecolor', CI='1')
            wrapper = 'import fcntl,termios,subprocess,sys,pathlib; fcntl.ioctl(0,termios.TIOCSCTTY,0); p=subprocess.Popen([sys.argv[2],"--help"]); pathlib.Path(sys.argv[1]+"/pid").write_text(str(p.pid)); code=p.wait(); pathlib.Path(sys.argv[1]+"/modes").write_text(repr(termios.tcgetattr(0))); sys.exit(code)'
            process = subprocess.Popen([sys.executable, '-c', wrapper, home, binary], stdin=slave, stdout=slave, stderr=slave, env=env, start_new_session=True)
            data = b''
            deadline = time.monotonic() + 5
            while b'\x1b]11;' not in data and time.monotonic() < deadline:
                if select.select([master], [], [], .1)[0]:
                    data += os.read(master, 65536)
            assert b'\x1b]11;' in data, ('no appearance probe', data)
            assert termios.tcgetattr(slave) != initial, 'test must interrupt active probe'
            os.kill(int(pathlib.Path(home, "pid").read_text()), sig)
            process.wait(timeout=5)
            restored = ast.literal_eval(pathlib.Path(home, "modes").read_text())
            initial[3] &= ~getattr(termios, 'PENDIN', 0)
            restored[3] &= ~getattr(termios, 'PENDIN', 0)
            assert restored == initial, ('probe left terminal modes changed', sig)
            assert process.returncode == 130, (sig, process.returncode)
            assert sorted(os.listdir(home)) == ['modes', 'pid']
            print('PASS help probe signal', signal.Signals(sig).name, flush=True)
        finally:
            if process is not None and process.poll() is None:
                process.kill()
                process.wait()
            os.close(master)
            os.close(slave)

import signal
for sig in (signal.SIGINT, signal.SIGTERM, signal.SIGHUP):
    check_probe_signal(sig)
