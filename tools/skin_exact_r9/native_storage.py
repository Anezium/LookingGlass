"""Read census allocation requests from the linked ARM initializer, without console access."""
from pathlib import Path
import hashlib,json,re,subprocess,sys
O=Path(sys.argv[1]).resolve();SDK=Path('/Users/anezium/vitasdk/bin');elf=O/'packages/baseline/looking-glass.elf'
symbols=subprocess.check_output([str(SDK/'arm-vita-eabi-nm'),'-nC',str(elf)],text=True)
rows=[]
for line in symbols.splitlines():
 m=re.match(r'([0-9a-f]+) [Tt] (.+)',line)
 if m:rows.append((int(m[1],16),m[2]))
start,name=next(r for r in rows if 'try_initialize' in r[1] and 'phase_census::STATE' in r[1] and 'Box<' in r[1])
end=min(address for address,_ in rows if address>start)
cmd=[str(SDK/'arm-vita-eabi-objdump'),'-dC',f'--start-address={start}',f'--stop-address={end}',str(elf)]
assembly=subprocess.check_output(cmd,text=True);(O/'proof/NATIVE-CENSUS-INITIALIZER.txt').write_text(assembly,encoding='utf-8')
low=int(re.search(r'movw\s+r6, #(\d+)',assembly)[1]);high=int(re.search(r'movt\s+r6, #(\d+)',assembly)[1])
assert re.search(r'mov\s+r0, r6',assembly) and '<__wrap_malloc>' in assembly
assert re.search(r'movs\s+r0, #12',assembly)
request=low+(high<<16);assert request==1656632
result=dict(status='PASS',elf_sha256=hashlib.sha256(elf.read_bytes()).hexdigest(),symbol=name,
 state_request_bytes=request,tls_wrapper_request_bytes=12,total_requests_bytes=request+12,
 host_state_layout_bytes=1658232,storage='fixed 180 rows, 128 actor slots per row, 4096 history slots',
 activation='Source initializes State only when phase_census is enabled, before gameplay; no per-frame allocation',
 qualification='ARM malloc request sizes, not rounded/live newlib usage or hardware heap measurements',command=cmd)
(O/'proof/NATIVE-CENSUS-STORAGE.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8')
print(json.dumps(result,indent=2))
