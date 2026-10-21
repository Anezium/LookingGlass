"""Strict residence-age scheduler receipts. No camera-time or campaign attestation.

The per-process HMAC key is withheld. Python checks reference consistency and
bounds only; it cannot verify a tag or attest that this native receipt is genuine.
"""
import native_timing_check as c
TOKEN=dict(ordinal=c.uint,viewer_seq=c.uint,submitted_us=c.uint,generation=c.uint,operation=c.uint,auth=c.array(c.octet,16))
def validate(p,m,world,generation,offset,frames,s,t):
    fields=dict(schema=c.literal('alice-native-window-v2'),expected_map=c.text,ready_offset_us=c.uint,
        ready_us=c.nullable(c.uint),generation=c.nullable(c.uint),start_us=c.nullable(c.uint),end_us=c.nullable(c.uint),review_deadline_us=c.nullable(c.uint),
        review_ack_us=c.nullable(c.uint),reviewed_token=c.nullable(lambda x:c.obj(x,TOKEN)),tokens=c.array(lambda x:c.obj(x,TOKEN)),token_total=c.uint,token_max_age_us=c.literal(60_000_000),
        acquired_begin_us=c.nullable(c.uint),seal_request_us=c.nullable(c.uint),start_state=c.nullable(lambda x:c.obj(x,c.STATE)),end_state=c.nullable(lambda x:c.obj(x,c.STATE)),
        fault=c.nullable(c.text),token_mac_kat=c.literal(True),token_authentication=c.literal('HMAC-SHA256/128; per-process random key withheld; native receipt UNATTESTED'),physical_image_time=c.literal('UNKNOWN; submitted token is not camera exposure time'),campaign_qualified=c.literal(False))
    c.obj(p,fields)
    c.need(p['fault'] is None,'native schedule rejected')
    c.need(p['expected_map']==world['map'] and p['generation']==generation and p['ready_us']==world['ready_us'] and p['ready_offset_us']==offset and p['start_us']==s and p['end_us']==t and p['review_deadline_us']==s-10_000_000,'schedule residence-age identity')
    c.need(m['schedule_token_size']>0 and m['schedule_storage_bytes']==128*m['schedule_token_size'],'schedule storage receipt')
    tokens=p['tokens'];total=p['token_total']
    c.need(0<len(tokens)==min(total,128)<=128,'schedule token ring count')
    c.need(len({x['ordinal'] for x in tokens})==len(tokens),'schedule duplicate token')
    c.need(len({x['viewer_seq'] for x in tokens})==len(tokens),'schedule reused frame reference')
    c.need(len({tuple(x['auth']) for x in tokens})==len(tokens),'schedule reused authentication tag')
    by_ordinal={x['ordinal']:x for x in tokens}
    c.need(all(x['viewer_seq']<by_ordinal[x['ordinal']+1]['viewer_seq'] and x['submitted_us']<=by_ordinal[x['ordinal']+1]['submitted_us'] for x in tokens if x['ordinal']+1 in by_ordinal),'schedule token sequence')
    for slot,x in enumerate(tokens):
        c.need(total-len(tokens)<x['ordinal']<=total and (x['ordinal']-1)%128==slot,'schedule token ring order')
        c.need(x['generation']==generation and world['ready_us']<=x['submitted_us']<=s-10_000_000 and 0<x['viewer_seq']<=m['viewer_seq'],'schedule token chronology')
    ack=p['review_ack_us'];token=p['reviewed_token']
    c.need(token is not None and 0<token['ordinal']<=total and ack is not None and world['ready_us']<=token['submitted_us']<ack<=s-10_000_000 and token['generation']==generation and token['operation']==frames[0]['begin_operation'] and 0<token['viewer_seq']<frames[0]['viewer_seq'] and ack-token['submitted_us']<=60_000_000,'missing/late/unbound review acknowledgement')
    for retained in tokens:
        if retained['ordinal']==token['ordinal']:
            c.need(retained==token,'reviewed token conflicts with retained reference')
        else:
            c.need(retained['viewer_seq']!=token['viewer_seq'] and retained['auth']!=token['auth'],'schedule reused reviewed reference')
    c.need(p['acquired_begin_us']==frames[0]['begin_us'] and s-12_000_000<=p['acquired_begin_us']<s-10_000_000,'scheduled left acquisition guard')
    c.need(p['seal_request_us']==frames[-1]['end_us']==m['capture_stop_us'] and t+2_000_000<=p['seal_request_us']<=t+3_000_000,'scheduled automatic seal')
    c.need(p['start_state'] is not None and p['end_state'] is not None,'missing scheduled boundary states')
    for state in [p['start_state'],p['end_state']]:
        for bits in [state['environment_bits'],state['pickup_bits'],*state['pose_bits']]:c.f32(bits)
