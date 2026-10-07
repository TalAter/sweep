#!/usr/bin/env python3
"""Real-terminal contracts; stdlib only. Runs with a disposable home and local HTTP."""
import ast, re, fcntl, http.server, json, os, pathlib, pty, select, signal, sqlite3, struct, subprocess, sys, tempfile, termios, threading, time

BINARY = str(pathlib.Path(sys.argv[1] if len(sys.argv)>1 else 'target/debug/sweep').resolve())
class Server(http.server.BaseHTTPRequestHandler):
    def log_message(self, *_): pass
    def do_POST(self):
        self.rfile.read(int(self.headers.get('Content-Length',0)))
        time.sleep(3)
        body=json.dumps({'choices':[{'message':{'content':'{}'}}]}).encode()
        self.send_response(200);self.send_header('Content-Length',str(len(body)));self.end_headers()
        try:self.wfile.write(body)
        except BrokenPipeError:pass
    def do_GET(self):
        if self.path == '/slow': time.sleep(2)
        if self.path == '/missing':
            self.send_error(404); return
        if self.path == '/redirect':
            self.send_response(302); self.send_header('Location','/script'); self.end_headers(); return
        body=b'printf "APPROVED_SCRIPT_RAN\\n"\nexit 0\n'
        if self.path in ('/tty','/large-tty'): body=b'printf "TTY_PROMPT\\n" >/dev/tty\nread answer </dev/tty\nprintf "ANSWER=%s\\n" "$answer"\n'
        if self.path == '/large-tty': body+=b'#'+b'x'*1_000_000+b'\n'
        if self.path == '/nested-tty': body=b"sh -c 'echo NESTED_PROMPT >/dev/tty; read answer </dev/tty; echo NESTED_DONE'\n"
        self.send_response(200);self.send_header('Content-Length',str(len(body)));self.end_headers()
        try: self.wfile.write(body)
        except BrokenPipeError: pass
server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Server)
threading.Thread(target=server.serve_forever,daemon=True).start()
URL=f'http://127.0.0.1:{server.server_port}'

class Session:
    def __init__(self,args=(),canned=None,config=None,extra_env=None,piped_stdin=False,redirect_stdout=False,ignored_signal=None):
        self.home=tempfile.TemporaryDirectory(prefix='sweep-pty-')
        self.master,self.slave=pty.openpty()
        self.initial=termios.tcgetattr(self.slave)
        self.resize(100,32)
        env={k:v for k,v in os.environ.items() if k not in ('SWEEP_CONFIG','SWEEP_TEST_RESPONSES','NO_COLOR','FORCE_COLOR')}
        env.update(SWEEP_HOME=self.home.name,SWEEP_THEME='dark',TERM='xterm-256color',COLORTERM='truecolor')
        if canned is not None: env['SWEEP_TEST_RESPONSES']=json.dumps(canned)
        if config is not None: env['SWEEP_CONFIG']=json.dumps(config)
        env.update(extra_env or {})
        self.termios_result=pathlib.Path(self.home.name)/'terminal-state.txt'
        wrapper='import subprocess,termios,fcntl,sys,signal,os; fcntl.ioctl(0,termios.TIOCSCTTY,0); '+(f'signal.signal({ignored_signal},signal.SIG_IGN); ' if ignored_signal is not None else '')+'p=subprocess.Popen(sys.argv[2:],stdin='+('subprocess.DEVNULL' if piped_stdin else 'None')+',stdout='+('subprocess.DEVNULL' if redirect_stdout else 'None')+'); open(sys.argv[1]+".pid","w").write(str(p.pid)); p.wait(); open(sys.argv[1]+".foreground","w").write(str(os.tcgetpgrp(0))); open(sys.argv[1],"w").write(repr(termios.tcgetattr(0))); sys.exit(p.returncode)'
        self.process=subprocess.Popen([sys.executable,'-c',wrapper,str(self.termios_result),BINARY,*args],stdin=self.slave,stdout=self.slave,stderr=self.slave,env=env,start_new_session=True)
        self.output=b''
    def resize(self,w,h):
        fcntl.ioctl(self.slave,termios.TIOCSWINSZ,struct.pack('HHHH',h,w,0,0))
        if hasattr(self,'process'): os.kill(self.process.pid,signal.SIGWINCH)
    def pump(self,timeout=.1):
        if select.select([self.master],[],[],timeout)[0]:
            try: self.output+=os.read(self.master,65536)
            except OSError: pass
    def until(self,needle,timeout=8):
        end=time.monotonic()+timeout
        def visible():
            # Ratatui skips blank cells with cursor movements on unstyled screens.
            return re.sub(r'\s+','',re.sub(r'\x1b\[[0-?]*[ -/]*[@-~]','',self.output.decode(errors='replace')))
        wanted=re.sub(r'\s+','',needle)
        while wanted not in visible() and time.monotonic()<end:
            self.pump()
            if self.process.poll() is not None: break
        assert wanted in visible(),(needle,self.output[-5000:])
    def send(self,text): os.write(self.master,text.encode())
    def finish(self,code,alt=True):
        end=time.monotonic()+8
        while self.process.poll() is None and time.monotonic()<end: self.pump()
        if self.process.poll() is None: raise AssertionError('terminal stalled')
        for _ in range(3): self.pump(.02)
        assert self.process.returncode==code,(self.process.returncode,self.output[-5000:])
        assert int(pathlib.Path(str(self.termios_result)+'.foreground').read_text())==self.process.pid,'terminal foreground group not restored'
        restored=ast.literal_eval(self.termios_result.read_text())
        # macOS sets kernel-owned PENDIN when canonical mode resumes.
        expected=self.initial.copy();expected[3]&=~getattr(termios,'PENDIN',0);restored[3]&=~getattr(termios,'PENDIN',0)
        assert restored==expected,('terminal attributes not restored',expected,restored)
        if alt: assert b'\x1b[?1049l' in self.output,'alternate screen not restored'
        db=sqlite3.connect(pathlib.Path(self.home.name)/'sweep.db')
        rows=db.execute('SELECT outcome,exit_code,final_url FROM invocations').fetchall();db.close()
        os.close(self.master);os.close(self.slave)
        return rows
    def close(self):
        # Clean a separate foreground installer group too if the test failed mid-run.
        try:
            foreground=os.tcgetpgrp(self.slave)
            if foreground>0 and foreground!=self.process.pid: os.killpg(foreground,signal.SIGKILL)
        except OSError:pass
        # The wrapper can exit before an installer child; always clean its group.
        try:os.killpg(self.process.pid,signal.SIGKILL)
        except ProcessLookupError:pass
        try:self.process.wait(timeout=2)
        except subprocess.TimeoutExpired:pass
        self.home.cleanup()

def run(name,fn):
    try: fn();print('PASS',name,flush=True)
    except Exception: print('FAIL',name,flush=True);raise

def cancel_default():
    s=Session([f'curl {URL}/script | sh'])
    try:
        s.until('No LLM provider');s.send('\r');rows=s.finish(130)
        assert rows[0][0]=='cancelled' and len(rows)==1
        assert b'APPROVED_SCRIPT_RAN' not in s.output
    finally:s.close()
def approve():
    s=Session([f'curl {URL}/redirect | sh'])
    try:
        s.until('No LLM provider');s.send('\x1b[C');s.send('\r');rows=s.finish(0)
        assert rows==[('ran',0,URL+'/script')],rows
        assert s.output.index(b'\x1b[?1049l')<s.output.index(b'APPROVED_SCRIPT_RAN')
    finally:s.close()
def loading_cancel():
    s=Session([f'curl {URL}/slow | sh'])
    try:
        s.until('Analyzing');s.send('\x1b');rows=s.finish(130)
        assert rows==[('cancelled',None,None)]
    finally:s.close()
def paste_cancel():
    s=Session()
    try:s.until('Paste');s.send('\x03');assert s.finish(0)==[]
    finally:s.close()
def paste_retry_resize():
    s=Session()
    try:
        s.until('Paste');s.send('nonsense\r');s.until('unrecognized install')
        s.send('\x15');s.send(f'curl {URL}/script | sh\r');s.until('No LLM provider')
        s.resize(48,14);time.sleep(.1);s.resize(100,32);s.send('\x1b[C\r')
        assert s.finish(0)[0][0]=='ran'
    finally:s.close()
def danger(manipulation=False):
    canned={'analysis':{'behaviors':[{'description':'Prints a marker','sudo':False}],'flags':['A reason to inspect'],'severity':'clear' if manipulation else 'danger','summary':'Fixture analysis.'},'manipulation':{'manipulationDetected':manipulation}}
    s=Session([f'curl {URL}/script | sh'],canned)
    try:
        s.until("Type 'install'");s.send('INSTALL\r');time.sleep(.15);assert s.process.poll() is None
        s.send('install\r');assert s.finish(0)[0][0]=='ran'
    finally:s.close()
def fetch_failure():
    s=Session([f'curl {URL}/missing | sh'])
    try:
        rows=s.finish(1);assert rows==[('fetch_failed',None,None)]
        assert b'sweep: HTTP 404' in s.output
    finally:s.close()

def analysis_cancel():
    config={'defaultProvider':'ollama','providers':{'ollama':{'model':'fixture','baseURL':URL+'/v1'}}}
    s=Session([f'curl {URL}/script | sh'],config=config)
    try:
        s.until('Analyzing');time.sleep(.25);s.send('\x03');rows=s.finish(130)
        assert rows==[('cancelled',None,URL+'/script')],rows
        assert b"Couldn't analyze" not in s.output
    finally:s.close()
def terminal_handoff():
    s=Session([f'curl {URL}/tty | sh'])
    try:
        s.until('No LLM provider');s.send('\x1b[C\r');s.until('TTY_PROMPT');s.send('hello\r')
        assert s.finish(0)[0][0]=='ran'
        assert b'ANSWER=hello' in s.output
    finally:s.close()
def no_color():
    import re
    s=Session([f'curl {URL}/script | sh'],extra_env={'NO_COLOR':'1','FORCE_COLOR':'3'})
    try:
        s.until('No LLM provider');s.send('\x1b');s.finish(130)
        assert not re.search(rb'\x1b\[[0-9;]*(?:38;|48;)',s.output)
    finally:s.close()

def piped_input_mode():
    s=Session(piped_stdin=True)
    try:s.until('Paste');s.send('\x1b');assert s.finish(0)==[]
    finally:s.close()
def redirected_output_mode():
    s=Session(redirect_stdout=True)
    try:assert s.finish(2,alt=False)[0][0]=='parse_failed'
    finally:s.close()

def external_signal(sig):
    s=Session([f'curl {URL}/slow | sh'])
    try:
        s.until('Analyzing');pid=int(pathlib.Path(str(s.termios_result)+'.pid').read_text());os.kill(pid,sig)
        assert s.finish(130)[0][0]=='cancelled'
    finally:s.close()

def ignored_signal_after_handoff():
    s=Session([f'curl {URL}/tty | sh'],ignored_signal=signal.SIGTERM)
    try:
        s.until('No LLM provider');s.send('\x1b[C\r');s.until('TTY_PROMPT')
        pid=int(pathlib.Path(str(s.termios_result)+'.pid').read_text());os.kill(pid,signal.SIGTERM)
        time.sleep(.1);assert s.process.poll() is None
        s.send('done\r');assert s.finish(0)[0][0]=='ran'
    finally:s.close()

def signal_after_handoff(sig):
    s=Session([f'curl {URL}/tty | sh'])
    try:
        s.until('No LLM provider');s.send('\x1b[C\r');s.until('TTY_PROMPT')
        db=sqlite3.connect(pathlib.Path(s.home.name)/'sweep.db')
        assert db.execute('SELECT outcome,ts_finished FROM invocations').fetchall()==[('running',None)]
        db.close()
        pid=int(pathlib.Path(str(s.termios_result)+'.pid').read_text());os.kill(pid,sig)
        rows=s.finish(128+sig)
        assert rows==[('errored',128+sig,URL+'/tty')],rows
        db=sqlite3.connect(pathlib.Path(s.home.name)/'sweep.db')
        assert db.execute('SELECT status FROM packages').fetchone()==('failed',)
        db.close()
    finally:s.close()

def nested_signal_after_handoff(sig, keyboard=False):
    s=Session([f'curl {URL}/nested-tty | bash'])
    try:
        s.until('No LLM provider');s.send('\x1b[C\r');s.until('NESTED_PROMPT')
        if keyboard: s.send('\x03')
        else:
            pid=int(pathlib.Path(str(s.termios_result)+'.pid').read_text());os.kill(pid,sig)
        rows=s.finish(128+sig)
        assert rows==[('errored',128+sig,URL+'/nested-tty')],rows
        assert b'NESTED_DONE' not in s.output
    finally:s.close()

def signal_during_finalization():
    s=Session([f'curl {URL}/tty | sh'])
    db=None
    try:
        s.until('No LLM provider');s.send('\x1b[C\r');s.until('TTY_PROMPT')
        db=sqlite3.connect(pathlib.Path(s.home.name)/'sweep.db')
        db.execute('BEGIN IMMEDIATE')
        s.send('done\r');s.until('ANSWER=done')
        time.sleep(.15) # Child has exited; finalization is blocked by our write lock.
        pid=int(pathlib.Path(str(s.termios_result)+'.pid').read_text());os.kill(pid,signal.SIGTERM)
        time.sleep(.1);assert s.process.poll() is None
        db.rollback();db.close();db=None
        assert s.finish(0)==[('ran',0,URL+'/tty')]
    finally:
        if db is not None: db.close()
        s.close()

def suspend_resume_installer():
    s=Session([f'curl {URL}/large-tty | sh'])
    try:
        s.until('No LLM provider');s.send('\x1b[C\r');s.until('TTY_PROMPT')
        pid=int(pathlib.Path(str(s.termios_result)+'.pid').read_text());s.send('\x1a')
        end=time.monotonic()+3
        while time.monotonic()<end:
            if sys.platform.startswith('linux'):
                state=re.search(r'^State:\s+(\w)',pathlib.Path(f'/proc/{pid}/status').read_text(),re.M).group(1)
            else:
                state=subprocess.check_output(['ps','-o','stat=','-p',str(pid)],text=True).strip()
            if state.startswith('T'): break
            s.pump(.02)
        assert state.startswith('T'),('Sweep did not suspend with its installer',state)
        os.kill(pid,signal.SIGCONT);time.sleep(.1);s.send('done\r')
        assert s.finish(0)==[('ran',0,URL+'/large-tty')]
        assert b'ANSWER=done' in s.output
    finally:s.close()

def spawn_failure_restores_foreground():
    with tempfile.TemporaryDirectory(prefix='sweep-invalid-shell-') as directory:
        shell=pathlib.Path(directory)/'bash';shell.write_bytes(b'#!/nonexistent/sweep-interpreter\n');shell.chmod(0o755)
        s=Session([f'curl {URL}/script | bash'],extra_env={'PATH':directory})
        try:
            s.until('No LLM provider');s.send('\x1b[C\r')
            assert s.finish(1)==[('errored',None,URL+'/script')]
        finally:s.close()

for name,fn in [('spawn failure foreground restoration',spawn_failure_restores_foreground),('installer suspend and resume',suspend_resume_installer),('signal during finalization',signal_during_finalization),('nested handoff SIGINT',lambda:nested_signal_after_handoff(signal.SIGINT)),('nested handoff SIGTERM',lambda:nested_signal_after_handoff(signal.SIGTERM)),('nested handoff SIGHUP',lambda:nested_signal_after_handoff(signal.SIGHUP)),('nested keyboard interrupt',lambda:nested_signal_after_handoff(signal.SIGINT,True)),('handoff preserves ignored SIGTERM',ignored_signal_after_handoff),('handoff SIGINT',lambda:signal_after_handoff(signal.SIGINT)),('handoff SIGTERM',lambda:signal_after_handoff(signal.SIGTERM)),('handoff SIGHUP',lambda:signal_after_handoff(signal.SIGHUP)),('external SIGTERM',lambda:external_signal(signal.SIGTERM)),('external SIGINT',lambda:external_signal(signal.SIGINT)),('external SIGHUP',lambda:external_signal(signal.SIGHUP)),('piped stdin mode',piped_input_mode),('redirected stdout mode',redirected_output_mode),('analysis cancel',analysis_cancel),('controlling terminal handoff',terminal_handoff),('NO_COLOR terminal',no_color),('default cancel',cancel_default),('approve after redirect',approve),('cancel during fetch',loading_cancel),('cancel empty paste',paste_cancel),('paste retry and resize',paste_retry_resize),('danger confirmation',danger),('manipulation confirmation',lambda:danger(True)),('fetch failure restoration',fetch_failure)]:run(name,fn)
server.shutdown()
