#!/usr/bin/env python3
"""Real-terminal contracts; stdlib only. Runs with a disposable home and local HTTP."""
import ast, re, fcntl, http.server, json, os, pathlib, pty, select, signal, sqlite3, struct, subprocess, sys, tempfile, termios, threading, time

BINARY = str(pathlib.Path(sys.argv[1] if len(sys.argv)>1 else 'target/debug/sweep').resolve())
class Server(http.server.BaseHTTPRequestHandler):
    # Per-session state, reset by Session: analysis requests reaching the /provider/
    # fixture as (path, parsed JSON body), and how often /models-once.json was served.
    provider_requests=[]
    models_once_served=0
    def log_message(self, *_): pass
    def do_POST(self):
        raw=self.rfile.read(int(self.headers.get('Content-Length',0)))
        if self.path.startswith('/provider/'):
            Server.provider_requests.append((self.path,json.loads(raw)))
            result={'manipulationDetected':False} if b'manipulationDetected' in raw else {'behaviors':[],'flags':[],'severity':'caution','summary':'Configured model reached local fixture.'}
            text=json.dumps(result)
            body=json.dumps({'content':[{'type':'text','text':text}]} if self.path.endswith('/messages') else {'output':[{'content':[{'type':'output_text','text':text}]}]}).encode()
            self.send_response(200);self.send_header('Content-Length',str(len(body)));self.end_headers();self.wfile.write(body);return
        time.sleep(3)
        body=json.dumps({'choices':[{'message':{'content':'{}'}}]}).encode()
        self.send_response(200);self.send_header('Content-Length',str(len(body)));self.end_headers()
        try:self.wfile.write(body)
        except BrokenPipeError:pass
    def do_GET(self):
        if self.path == '/models.json':
            body=json.dumps(CATALOG).encode()
            self.send_response(200);self.send_header('Content-Length',str(len(body)));self.end_headers();self.wfile.write(body);return
        if self.path == '/models-once.json':
            # A catalog without OpenAI, served once; a second fetch in the same session fails.
            Server.models_once_served+=1
            if Server.models_once_served>1: self.send_error(404); return
            body=json.dumps({k:v for k,v in CATALOG.items() if k!='openai'}).encode()
            self.send_response(200);self.send_header('Content-Length',str(len(body)));self.end_headers();self.wfile.write(body);return
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
    def __init__(self,args=(),canned=None,config=None,extra_env=None,piped_stdin=False,redirect_stdout=False,ignored_signal=None,wizard=False,cache=None):
        Server.provider_requests=[];Server.models_once_served=0
        self.home=tempfile.TemporaryDirectory(prefix='sweep-pty-')
        # Any existing config file skips the setup wizard, so sessions start directly.
        if not wizard: (pathlib.Path(self.home.name)/'config.jsonc').write_text('')
        if cache is not None:
            cache_dir=pathlib.Path(self.home.name)/'cache';cache_dir.mkdir()
            (cache_dir/'models.dev.json').write_text(json.dumps(cache))
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
    def visible(self,since=0):
        # Ratatui skips blank cells with cursor movements on unstyled screens.
        return re.sub(r'\s+','',re.sub(r'\x1b\[[0-?]*[ -/]*[@-~]','',self.output[since:].decode(errors='replace')))
    def until(self,needle,timeout=8,since=0):
        end=time.monotonic()+timeout
        wanted=re.sub(r'\s+','',needle)
        while wanted not in self.visible(since) and time.monotonic()<end:
            self.pump()
            if self.process.poll() is not None: break
        assert wanted in self.visible(since),(needle,self.output[-5000:])
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
        # A refused or cancelled setup never opens the database.
        rows=[]
        if (pathlib.Path(self.home.name)/'sweep.db').exists():
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

ONLY=sys.argv[2] if len(sys.argv)>2 else ''
def run(name,fn):
    if ONLY not in name: return
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
def nerd_font_caps():
    canned={'analysis':{'behaviors':[],'flags':[],'severity':'caution','summary':'Fixture analysis.'},'manipulation':{'manipulationDetected':False}}
    for enabled in (True,False):
        s=Session([f'curl {URL}/script | sh'],canned,config={'nerdFonts':True} if enabled else None)
        try:
            s.until('caution');s.send('\x1b');assert s.finish(130)[0][0]=='cancelled'
            assert ('\ue0b6'.encode() in s.output)==enabled,(enabled,s.output[-3000:])
        finally:s.close()
def manipulation_after_analysis_failure():
    canned={'analysis':{},'manipulation':{'manipulationDetected':True}}
    s=Session([f'curl {URL}/script | sh'],canned)
    try:
        s.until("Type 'install'");s.until("Couldn't analyze");s.until('analysis may be compromised')
        s.send('\r\x1b[C\rINSTALL\r');time.sleep(.15)
        assert s.process.poll() is None
        assert b'APPROVED_SCRIPT_RAN' not in s.output
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
    try:assert s.finish(2,alt=False)==[]
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

# --- First-run setup wizard ---------------------------------------------------
def _model(date): return {'tool_call':True,'modalities':{'input':['text'],'output':['text']},'release_date':date}
# Sonnet is dated oldest so the recommended row is visibly moved to the top.
CATALOG={'anthropic':{'models':{'claude-opus-4-1':_model('2025-08-05'),'claude-sonnet-4-6':_model('2025-05-01'),'claude-haiku-4-5':_model('2025-10-15')}},
         'openai':{'models':{'gpt-5':_model('2025-08-07')}},'ollama':{'models':{'llama3':_model('2024-04-18')}}}
UNROUTABLE='http://127.0.0.1:1/'
DOWN='\x1b[B'
class Wizard(Session):
    """Wizard session whose PATH is only a per-test bin dir (`claude=True` adds a stub `claude`)
    and whose models.dev URL is unroutable unless a scenario overrides it, so happy paths
    provably run from the seeded cache."""
    def __init__(self,args=(),claude=False,cache=CATALOG,extra_env=None,canned=None):
        self.bin=tempfile.TemporaryDirectory(prefix='sweep-wizard-bin-')
        if claude:
            stub=pathlib.Path(self.bin.name)/'claude';stub.write_text('#!/bin/sh\nexit 0\n');stub.chmod(0o755)
        env={'PATH':self.bin.name,'SWEEP_MODELS_URL':UNROUTABLE};env.update(extra_env or {})
        super().__init__(args,canned=canned,wizard=True,cache=cache,extra_env=env)
    def home_path(self,name): return pathlib.Path(self.home.name)/name
    def config(self): return json.loads(self.home_path('config.jsonc').read_text())
    def schema(self):
        schema=json.loads(self.home_path('config.schema.json').read_text())
        assert schema['title']=='Sweep configuration file',schema
        return schema
    def assert_nothing_written(self):
        for name in ('config.jsonc','config.schema.json','sweep.db','cache/appearance.json'):
            assert not self.home_path(name).exists(),name
    def assert_no_cache(self): assert not self.home_path('cache').exists()
    def pid(self): return int(pathlib.Path(str(self.termios_result)+'.pid').read_text())
    def to_providers(self,nerd=False):
        self.until('Takes ~45 seconds');self.send('\r');self.until('four icons')
        if not nerd: self.send(DOWN)
        self.send('\r');self.until('SELECT API PROVIDER')
    def check_rows(self,*rows):
        """Toggle the given 0-based Providers rows (cursor starts on row 0) and submit."""
        cursor=0
        for row in sorted(rows):
            self.send(DOWN*(row-cursor));cursor=row;self.send(' ')
        self.send('\r')
    def enter_key(self,name,key):
        self.until(f'{name} API key');self.send(key+'\r')
    def close(self):
        super().close();self.bin.cleanup()

def wizard_esc_on_welcome():
    s=Wizard()
    try:
        s.until('Takes ~45 seconds');s.send('\x1b');assert s.finish(0)==[];s.assert_nothing_written()
    finally:s.close()
def wizard_ctrl_c_on_providers():
    s=Wizard()
    try:
        s.to_providers();s.send('\x03');assert s.finish(0)==[];s.assert_nothing_written()
    finally:s.close()
def wizard_external_signal(sig):
    s=Wizard(cache=None)
    try:
        s.until('Takes ~45 seconds');os.kill(s.pid(),sig)
        assert s.finish(0)==[];s.assert_nothing_written();s.assert_no_cache()
        assert b'Configuration saved' not in s.output
    finally:s.close()
def wizard_signal_during_input_flood():
    """A signal that lands while queued input is still being drained cancels before the
    queued Enter can finish setup: nothing is written. Output is pumped throughout so
    the flood never deadlocks against a full PTY; plain letters are ignored on the
    Providers screen and cannot be misparsed as Esc at a read boundary."""
    s=Wizard()
    stop=threading.Event();enter=threading.Event()
    def flood():
        try:
            while not stop.is_set():
                os.write(s.master,b'x'*1024)
                if enter.is_set(): os.write(s.master,b'\r');enter.clear()
        except OSError: pass
    def pump_for(seconds):
        end=time.monotonic()+seconds
        while time.monotonic()<end: s.pump(.02)
    try:
        s.to_providers()
        threading.Thread(target=flood,daemon=True).start();pump_for(.3)
        os.kill(s.pid(),signal.SIGTERM);pump_for(.1)
        enter.set();pump_for(.3);stop.set()
        assert s.finish(0)==[];s.assert_nothing_written()
        assert b'Configuration saved' not in s.output
    finally:stop.set();s.close()
def wizard_probe_once_and_cached_on_completion():
    """No SWEEP_THEME forces the OSC 11 probe. The reply makes it light; finishing setup
    caches that answer and the session that follows does not probe again."""
    s=Wizard([f'curl {URL}/script | sh'],extra_env={'SWEEP_THEME':''})
    try:
        end=time.monotonic()+8
        while b'\x1b]11;?' not in s.output and time.monotonic()<end: s.pump()
        assert b'\x1b]11;?' in s.output,s.output[-2000:]
        s.send('\x1b]11;rgb:ffff/ffff/ffff\x1b\\\x1b[?62c')
        s.to_providers(nerd=True);s.send('\r');s.until('Configuration saved');s.until('No LLM provider');s.send('\x1b')
        assert s.finish(130)[0][0]=='cancelled'
        assert s.output.count(b'\x1b]11;?')==1,s.output.count(b'\x1b]11;?')
        assert json.loads(s.home_path('cache/appearance.json').read_text())['appearance']=='light'
    finally:s.close()
def wizard_signal_during_appearance_probe():
    """No SWEEP_THEME forces the OSC 11 probe; a signal while it waits must still leave a
    clean terminal and an untouched home (not even the appearance cache)."""
    s=Wizard(cache=None,extra_env={'SWEEP_THEME':''})
    try:
        end=time.monotonic()+8
        while b'\x1b]11;?' not in s.output and time.monotonic()<end: s.pump()
        assert b'\x1b]11;?' in s.output,s.output[-2000:]
        os.kill(s.pid(),signal.SIGTERM)
        assert s.finish(0,alt=False)==[];s.assert_nothing_written();s.assert_no_cache()
        assert b'Configuration saved' not in s.output
    finally:s.close()
def wizard_masked_key_editing():
    """A bracketed paste with CJK characters is masked, editable, and never echoed."""
    s=Wizard()
    secret='test-secret-秘密'
    try:
        s.to_providers();s.check_rows(0);s.until('Anthropic API key')
        s.send('\x1b[200~'+secret+'\n\x1b[201~');s.until('•'*len(secret))
        s.send('\x1b[D\x7fX\r');s.until('Recommended');s.send('\r');s.until('Configuration saved');s.until('Paste');s.send('\x1b')
        assert s.finish(0)==[]
        assert s.config()['providers']['anthropic']['apiKey']=='test-secret-X密'
        for leak in (secret,'秘密','X密','test-secret'): assert leak.encode() not in s.output,leak
    finally:s.close()
def wizard_validation_reject():
    """A `$VAR` key the session could not resolve is refused at write time: message on a
    restored terminal, exit 1, nothing written."""
    s=Wizard()
    try:
        s.to_providers();s.check_rows(0);s.enter_key('Anthropic','$SWEEP_PTY_UNSET_KEY')
        s.until('Recommended');s.send('\r');assert s.finish(1)==[];s.assert_nothing_written()
        message=b'sweep: environment variable SWEEP_PTY_UNSET_KEY is not set.'
        assert message in s.output and s.output.index(b'\x1b[?1049l')<s.output.index(message),s.output[-3000:]
    finally:s.close()
def wizard_no_claude_row():
    s=Wizard()
    try:
        s.to_providers();s.until('Ollama (local)')
        assert 'ClaudeCode' not in s.visible() and 'CODINGAGENT' not in s.visible(),s.output[-3000:]
        s.send('\x1b');assert s.finish(0)==[]
    finally:s.close()
def wizard_anthropic_only():
    s=Wizard()
    try:
        s.to_providers(nerd=False);s.check_rows(0);s.enter_key('Anthropic','sk-ant-test')
        s.until('Recommended');s.send('\r');s.until('Configuration saved');s.until('Paste');s.send('\x1b')
        assert s.finish(0)==[]
        expected='{\n  "$schema": "./config.schema.json",\n  "providers": {\n    "anthropic": { "apiKey": "sk-ant-test", "model": "claude-sonnet-4-6" }\n  },\n  "defaultProvider": "anthropic",\n  "nerdFonts": false\n}\n'
        assert s.home_path('config.jsonc').read_text()==expected,s.home_path('config.jsonc').read_text()
        s.schema()
    finally:s.close()
def wizard_default_picker_openai():
    """Down on the default picker makes OpenAI the default, and the same run analyzes with it:
    OPENAI_BASE_URL points the session at the local fixture, which records the model used."""
    s=Wizard([f'curl {URL}/script | sh'],extra_env={'OPENAI_BASE_URL':URL+'/provider/v1'})
    try:
        s.to_providers(nerd=True);s.check_rows(0,1);s.enter_key('Anthropic','sk-ant-test')
        s.until('Recommended');s.send('\r');s.enter_key('OpenAI','sk-proj-test');s.until('gpt-5');s.send('\r')
        s.until('be the default?');s.send(DOWN);s.send('\r')
        s.until('Configuration saved');mark=len(s.output);s.until('Configured model reached local fixture.',since=mark)
        assert [(path,body['model']) for path,body in Server.provider_requests]==[('/provider/v1/responses','gpt-5')]*2,Server.provider_requests
        s.send('\x1b');rows=s.finish(130)
        assert rows==[('cancelled',None,URL+'/script')],rows
        config=s.config()
        assert config['defaultProvider']=='openai' and config['nerdFonts'] is True,config
        assert config['providers']=={'anthropic':{'apiKey':'sk-ant-test','model':'claude-sonnet-4-6'},'openai':{'apiKey':'sk-proj-test','model':'gpt-5'}},config
    finally:s.close()
def wizard_configured_direct():
    """The run continues with the config just written: analysis requests carry the chosen
    model (ANTHROPIC_BASE_URL routes them to the local fixture) and the nerd-fonts answer
    decides whether the session's pills get powerline caps."""
    for nerd in (True,False):
        s=Wizard([f'curl {URL}/script | sh'],extra_env={'ANTHROPIC_BASE_URL':URL+'/provider/v1'})
        try:
            s.to_providers(nerd=nerd);s.check_rows(0);s.enter_key('Anthropic','sk-ant-test')
            s.until('Recommended');s.send('\r');s.until('Configuration saved');mark=len(s.output)
            s.until('Configured model reached local fixture.',since=mark)
            assert [(path,body['model']) for path,body in Server.provider_requests]==[('/provider/v1/messages','claude-sonnet-4-6')]*2,Server.provider_requests
            s.send('\x1b');assert s.finish(130)[0][0]=='cancelled'
            assert ('\ue0b6'.encode() in s.output[mark:])==nerd,(nerd,s.output[mark:][-3000:])
            assert s.config()['nerdFonts'] is nerd
        finally:s.close()
def wizard_nothing_checked():
    s=Wizard([f'curl {URL}/script | sh'],cache=None)
    try:
        s.to_providers(nerd=True);s.until('without an LLM');s.send('\r')
        s.until('Configuration saved');s.until('No LLM provider');s.send('\x1b')
        assert s.finish(130)[0][0]=='cancelled'
        text=s.home_path('config.jsonc').read_text()
        assert '"providers": {}' in text and 'defaultProvider' not in text and '"nerdFonts": true' in text,text
        s.schema();s.assert_no_cache()
    finally:s.close()
def wizard_claude_code_only_needs_no_model_list():
    """A CLI-only selection goes straight to the disclaimer: with no cache and an unroutable
    models URL, reaching completion proves nothing was fetched."""
    s=Wizard(claude=True,cache=None)
    try:
        s.to_providers();s.until('CODING AGENT');s.check_rows(6);s.until('route your queries')
        assert 'Loadingmodels' not in s.visible(),s.output[-3000:]
        s.send('\r');s.until('Configuration saved');s.until('Paste');s.send('\x1b');assert s.finish(0)==[]
        config=s.config()
        assert config['providers']=={'claude-code':{}} and config['defaultProvider']=='claude-code',config
        s.assert_no_cache()
    finally:s.close()
def wizard_fetched_catalog_published_only_on_completion():
    """Fetched models.dev bytes stay in memory: Esc after loading leaves no cache, finishing
    publishes it next to the config."""
    for complete in (False,True):
        s=Wizard(cache=None,extra_env={'SWEEP_MODELS_URL':URL+'/models.json'})
        try:
            s.to_providers();s.check_rows(0);s.enter_key('Anthropic','sk-ant-test');s.until('Recommended')
            if complete:
                s.send('\r');s.until('Configuration saved');s.until('Paste');s.send('\x1b');assert s.finish(0)==[]
                assert json.loads(s.home_path('cache/models.dev.json').read_text())==CATALOG
            else:
                s.send('\x1b');assert s.finish(0)==[];s.assert_nothing_written();s.assert_no_cache()
        finally:s.close()
def wizard_claude_code_alone_skipped():
    s=Wizard(claude=True)
    try:
        s.to_providers();s.until('CODING AGENT');s.check_rows(6)
        s.until('route your queries');mark=len(s.output);s.send('\x1b')
        s.until('SELECT API PROVIDER',since=mark);s.until('[ ] Claude Code',since=mark)
        assert '[✓]' not in s.visible(mark),s.output[mark:]
        s.send('\x1b');assert s.finish(0)==[];s.assert_nothing_written()
    finally:s.close()
def wizard_anthropic_plus_claude_code_skipped():
    s=Wizard(claude=True)
    try:
        s.to_providers();s.check_rows(0,6);s.enter_key('Anthropic','sk-ant-test')
        s.until('Recommended');s.send('\r');s.until('route your queries');s.send('\x1b')
        s.until('Configuration saved');s.until('Paste');s.send('\x1b');assert s.finish(0)==[]
        config=s.config()
        assert config['defaultProvider']=='anthropic' and list(config['providers'])==['anthropic'],config
    finally:s.close()
def wizard_provider_absent_from_catalog():
    """Skipping the only provider returns to a cleared list; resubmitting reuses the
    catalog already in memory (the fixture serves it once, so a refetch would fail)."""
    s=Wizard(cache=None,extra_env={'SWEEP_MODELS_URL':URL+'/models-once.json'})
    try:
        s.to_providers();s.check_rows(1);s.enter_key('OpenAI','sk-proj-test')
        s.until('No models found for OpenAI.');mark=len(s.output);s.send('\x1b')
        s.until('SELECT API PROVIDER',since=mark);s.until('[ ] OpenAI',since=mark)
        assert '[✓]' not in s.visible(mark),s.output[mark:]
        mark=len(s.output);s.check_rows(0);s.until('Anthropic API key',since=mark)
        assert 'Loadingmodels' not in s.visible(mark),s.output[mark:]
        assert Server.models_once_served==1
        s.send('\x1b');assert s.finish(0)==[];s.assert_nothing_written();s.assert_no_cache()
    finally:s.close()
def wizard_no_cache_fetch_failure():
    s=Wizard(cache=None)
    try:
        s.to_providers();s.check_rows(0);assert s.finish(1)==[]
        assert f'sweep: could not load model list from {UNROUTABLE}. Check your connection and try again.'.encode() in s.output,s.output[-3000:]
        s.assert_nothing_written();s.assert_no_cache()
    finally:s.close()
def wizard_stale_cache_fetch_failure():
    s=Wizard()
    try:
        stale=time.time()-25*3600;os.utime(s.home_path('cache/models.dev.json'),(stale,stale))
        s.to_providers();s.check_rows(0);s.enter_key('Anthropic','sk-ant-test')
        s.until('Recommended');s.send('\x1b');assert s.finish(0)==[];s.assert_nothing_written()
    finally:s.close()


for name,fn in [('wizard esc on welcome',wizard_esc_on_welcome),('wizard ctrl-c on providers',wizard_ctrl_c_on_providers),('wizard external SIGTERM',lambda:wizard_external_signal(signal.SIGTERM)),('wizard external SIGINT',lambda:wizard_external_signal(signal.SIGINT)),('wizard external SIGHUP',lambda:wizard_external_signal(signal.SIGHUP)),('wizard signal during appearance probe',wizard_signal_during_appearance_probe),('wizard signal during input flood',wizard_signal_during_input_flood),('wizard probe once and cached on completion',wizard_probe_once_and_cached_on_completion),('wizard masked key editing',wizard_masked_key_editing),('wizard validation reject',wizard_validation_reject),('wizard no claude row',wizard_no_claude_row),('wizard anthropic only',wizard_anthropic_only),('wizard default picker openai',wizard_default_picker_openai),('wizard configured direct',wizard_configured_direct),('wizard nothing checked',wizard_nothing_checked),('wizard claude code only needs no model list',wizard_claude_code_only_needs_no_model_list),('wizard fetched catalog published only on completion',wizard_fetched_catalog_published_only_on_completion),('wizard claude code alone skipped',wizard_claude_code_alone_skipped),('wizard anthropic plus claude code skipped',wizard_anthropic_plus_claude_code_skipped),('wizard provider absent from catalog',wizard_provider_absent_from_catalog),('wizard no cache fetch failure',wizard_no_cache_fetch_failure),('wizard stale cache fetch failure',wizard_stale_cache_fetch_failure),('spawn failure foreground restoration',spawn_failure_restores_foreground),('installer suspend and resume',suspend_resume_installer),('signal during finalization',signal_during_finalization),('nested handoff SIGINT',lambda:nested_signal_after_handoff(signal.SIGINT)),('nested handoff SIGTERM',lambda:nested_signal_after_handoff(signal.SIGTERM)),('nested handoff SIGHUP',lambda:nested_signal_after_handoff(signal.SIGHUP)),('nested keyboard interrupt',lambda:nested_signal_after_handoff(signal.SIGINT,True)),('handoff preserves ignored SIGTERM',ignored_signal_after_handoff),('handoff SIGINT',lambda:signal_after_handoff(signal.SIGINT)),('handoff SIGTERM',lambda:signal_after_handoff(signal.SIGTERM)),('handoff SIGHUP',lambda:signal_after_handoff(signal.SIGHUP)),('external SIGTERM',lambda:external_signal(signal.SIGTERM)),('external SIGINT',lambda:external_signal(signal.SIGINT)),('external SIGHUP',lambda:external_signal(signal.SIGHUP)),('piped stdin mode',piped_input_mode),('redirected stdout mode',redirected_output_mode),('analysis cancel',analysis_cancel),('controlling terminal handoff',terminal_handoff),('NO_COLOR terminal',no_color),('default cancel',cancel_default),('approve after redirect',approve),('cancel during fetch',loading_cancel),('cancel empty paste',paste_cancel),('paste retry and resize',paste_retry_resize),('danger confirmation',danger),('nerd font pill caps',nerd_font_caps),('manipulation confirmation',lambda:danger(True)),('manipulation after analysis failure',manipulation_after_analysis_failure),('fetch failure restoration',fetch_failure)]:run(name,fn)
server.shutdown()
