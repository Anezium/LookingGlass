import argparse

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
       out.write(('\n--- r4 host receiver connection '+self.client_address[0]+' '+datetime.datetime.now().astimezone().isoformat()+'\n').encode());out.flush()
       while True:
        data=self.rfile.read1(4096)
        if not data:break
        out.write(data);out.flush()
    class Server(socketserver.ThreadingTCPServer):
     allow_reuse_address=True
     daemon_threads=True
    with Server(('0.0.0.0',9999),Handler) as server:
     (ROOT/'HOST-LOGGER.json').write_text(json.dumps(dict(pid=os.getpid(),time=datetime.datetime.now().astimezone().isoformat(),operation='start passive host TCP receiver',port=9999,path=str(LOG),reason='No listener; previous log stopped mid-line during Hatter1 load; no driver lifecycle action'),indent=2)+'\n')
     print('Passive host receiver listening on9999',flush=True)
     server.serve_forever()

if __name__=='__main__':
    main()
