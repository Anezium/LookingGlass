#!/usr/bin/env python3
"""Read the actual exported process-parameter values from the linked ELF."""
from pathlib import Path
import struct,sys,json
b=Path(sys.argv[1]).read_bytes()
assert b[:7]==b'\x7fELF\x01\x01\x01'
h=struct.unpack_from('<HHIIIIIHHHHHH',b,16);shoff=h[5];shsize=h[10];shcount=h[11]
sections=[struct.unpack_from('<IIIIIIIIII',b,shoff+n*shsize) for n in range(shcount)]
wanted={'sceUserMainThreadStackSize':8*1024*1024,'_newlib_heap_size_user':192*1024*1024};found={}
for section in sections:
 if section[1]!=2:continue
 strings=sections[section[6]];names=b[strings[4]:strings[4]+strings[5]]
 for pos in range(section[4],section[4]+section[5],section[9]):
  name,address,size,info,other,index=struct.unpack_from('<IIIBBH',b,pos)
  symbol=names[name:names.find(b'\0',name)].decode()
  if symbol not in wanted:continue
  storage=sections[index];offset=storage[4]+address-storage[3]
  value=struct.unpack_from('<I',b,offset)[0]
  assert value==wanted[symbol],(symbol,value,wanted[symbol])
  found[symbol]=value
assert found==wanted,found
print(json.dumps(found,sort_keys=True))
