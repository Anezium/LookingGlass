import argparse

def listener_provenance():
    import subprocess
    check=subprocess.run(['/usr/sbin/lsof','-nP','-iTCP:9999','-sTCP:LISTEN','-Fp'],capture_output=True,text=True)
    assert check.returncode in (0,1), 'Could not inspect port 9999 listener'
    pids=sorted({int(line[1:]) for line in check.stdout.splitlines() if line.startswith('p')})
    if not pids:
        assert check.returncode==1 and not check.stdout.strip(), 'Ambiguous listener inspection'
        return dict(status='ABSENT',port=9999,listeners=[])
    assert check.returncode==0
    listeners=[]
    for pid in pids:
        opened=subprocess.run(['/usr/sbin/lsof','-nP','-p',str(pid),'-Fn'],capture_output=True,text=True)
        assert opened.returncode==0 and 'n/Users/anezium/vita-j0/work/vita.log' in opened.stdout.splitlines(), 'Port 9999 listener does not have the required vita.log open; do not launch or replace it'
        listeners.append(dict(pid=pid,path='/Users/anezium/vita-j0/work/vita.log'))
    return dict(status='PASS',port=9999,listeners=listeners)

def main():
    parser=argparse.ArgumentParser(description='Start the passive host receiver only after ownership and listener checks')
    parser.add_argument('arguments',nargs='*')
    parser.parse_args()
    import socketserver,datetime,json,os
    from pathlib import Path
    ROOT=Path(__file__).resolve().parent
    LOG=Path('/Users/anezium/vita-j0/work/vita.log')
    class Handler(socketserver.StreamRequestHandler):
     def handle(self):
      with LOG.open('ab') as out:
       out.write(('\n--- r5 host receiver connection '+self.client_address[0]+' '+datetime.datetime.now().astimezone().isoformat()+'\n').encode());out.flush()
       while True:
        data=self.rfile.read1(4096)
        if not data:break
        out.write(data);out.flush()
    class Server(socketserver.ThreadingTCPServer):
     allow_reuse_address=True
     daemon_threads=True
    with LOG.open('ab') as log_provenance, Server(('0.0.0.0',9999),Handler) as server:
     (ROOT/'HOST-LOGGER.json').write_text(json.dumps(dict(pid=os.getpid(),time=datetime.datetime.now().astimezone().isoformat(),operation='start passive host TCP receiver',port=9999,path=str(LOG),reason='No listener after admission; no driver lifecycle action'),indent=2)+'\n')
     print('Passive host receiver listening on9999',flush=True)
     server.serve_forever()

if __name__=='__main__':
    main()
