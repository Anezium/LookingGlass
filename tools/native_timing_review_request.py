#!/usr/bin/env python3
"""Prepare an exclusive native ACK file AFTER separate image return/inspection.
No upload, input, automatic approval or image inspection. Run identity and five
submitted-frame reference fields and tag must come from the same inspected overlay.
The key is withheld: this helper validates format, never HMAC authenticity.
"""
import argparse,json
from pathlib import Path

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--run-id',required=True)
    p.add_argument('--token',type=int,nargs=5,required=True,metavar=('ORDINAL','FRAME','NATIVE_US','GENERATION','OPERATION'))
    p.add_argument('--auth',required=True,help='32 hexadecimal characters from the same submitted frame')
    p.add_argument('--output',type=Path,required=True)
    a=p.parse_args()
    try:run=bytes.fromhex(a.run_id)
    except ValueError:p.error('run-id must be32 hexadecimal characters')
    if len(run)!=16 or run==bytes(16):p.error('run-id must be nonzero128bit identity from submitted image')
    if not all(0<=x<2**64 for x in a.token) or not all(a.token[i]>0 for i in (0,1,3)):p.error('invalid token fields')
    try:auth=bytes.fromhex(a.auth)
    except ValueError:p.error('invalid authentication tag')
    if len(auth)!=16:p.error('authentication tag must be16bytes')
    fields=dict(zip(('ordinal','viewer_seq','submitted_us','generation','operation'),a.token))
    fields['auth']=list(auth)
    with a.output.open('x',encoding='utf-8')as f:json.dump(dict(run_id=list(run),token=fields),f,separators=(',',':'));f.write('\n')
if __name__=='__main__':main()
