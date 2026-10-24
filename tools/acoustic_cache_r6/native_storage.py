"""Inspect the sealed ARM ELF locally; no target execution."""
from pathlib import Path
import json,re,subprocess,sys
O=Path(sys.argv[1]);elf=O/'packages/baseline/looking-glass.elf';sdk=Path('/Users/anezium/vitasdk/bin')
nm=subprocess.check_output([str(sdk/'arm-vita-eabi-nm'),'-S','-C',str(elf)],text=True)
for tag,needle in [('native-audio-new','<looking_glass::audio::Audio>::new'),('native-census-tls','::try_initialize::<looking_glass::phase_census::STATE')]:
 rows=[line.split(None,3) for line in nm.splitlines() if needle in line];assert len(rows)==1,(tag,rows)
 address,size,_,symbol=rows[0];start=int(address,16)
 data=subprocess.check_output([str(sdk/'arm-vita-eabi-objdump'),'-d',f'--start-address={start}',f'--stop-address={start+int(size,16)}',str(elf)],text=True)
 (O/'proof'/f'{tag}.s').write_text(data)
cache=(O/'proof/native-audio-new.s').read_text();census=(O/'proof/native-census-tls.s').read_text()
assert re.search(r'movw\s+r0, #31236',cache) and '<__wrap_malloc>' in cache
assert re.search(r'movw\s+r0, #8392',census) and re.search(r'movt\s+r0, #5',census)
(O/'proof/NATIVE-STORAGE.json').write_text(json.dumps(dict(cache_allocation_request_bytes=31236,census_tls_allocation_request_bytes=336072,allocation_scope='cache once at Audio construction when enabled; census once at startup when enabled. Requests from sealed ARM ELF, not rounded/live newlib usage.',evidence=['proof/native-audio-new.s','proof/native-census-tls.s']),indent=2)+'\n')
print('PASS sealed ARM allocation requests: cache 31236 bytes; census TLS 336072 bytes')
