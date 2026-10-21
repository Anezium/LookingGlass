"""Strict optional counter-only extension. No producer-fence/audio qualification.

Read order is authoritative. Only the prescribed Binding and AfterViewer pair
can prove a containing superset; requested snapshots are retained, never picked
for a more favorable answer. Future callers can use zero_proof explicitly with
predeclared request ordinals. No intentional-flush subtraction exists.
"""
from native_timing_check import obj, array, nullable, uint, u32, octet, text, boolean, literal, need
FIELDS = ('PcmStarvedBlocks', 'MusicUnderrunFrames')
BINDING = dict(lifetime_id=nullable(array(octet,16)), clock_id=text, width_bits=literal(32),
               activation_pre_us=nullable(uint), activation_post_us=nullable(uint))
SNAPSHOT = dict(ordinal=uint, field=text, tag=text, lifetime_id=nullable(array(octet,16)),
                pre_us=nullable(uint), value=u32, post_us=nullable(uint), busy=boolean, wrapped=boolean)

def zero_proof(before, after, s, t):
    obj(before,SNAPSHOT);obj(after,SNAPSHOT);uint(s);uint(t)
    need(s<t, 'counter proof window')
    need(before['field']==after['field'] and before['field'] in FIELDS and before['ordinal']<after['ordinal'], 'counter proof field/order')
    need(before['lifetime_id'] == after['lifetime_id'] and before['lifetime_id'] not in (None,[0]*16), 'counter lifetime change/unknown')
    for snap in (before,after):
        need(snap['pre_us'] is not None and snap['post_us'] is not None and snap['pre_us'] <= snap['post_us'], 'counter missing/backward bracket')
        need(not snap['busy'] and not snap['wrapped'], 'counter concurrent read/wrap fault')
    need(after['value'] >= before['value'], 'counter reset/wrap')
    if not (before['post_us'] <= s and after['pre_us'] >= t):
        return dict(guard='UNKNOWN', reason='counter brackets do not contain window')
    delta = after['value'] - before['value']
    return dict(guard='ZERO_ON_CONTAINING_SUPERSET' if delta == 0 else 'FAIL', delta=delta,
                positive_inside_window='UNKNOWN' if delta else 'zero',
                before_ordinal=before['ordinal'], after_ordinal=after['ordinal'])

def validate(data, clock_id, s, t):
    obj(data,dict(status=literal('PARTIAL_COUNTERS_ONLY_UNQUALIFIED'),
        pcm=nullable(lambda x:obj(x,BINDING)), music=nullable(lambda x:obj(x,BINDING)),
        snapshots=array(lambda x:obj(x,SNAPSHOT)), attempted=uint,lost=uint,overflow=boolean,
        capacity=literal(16),snapshot_size=uint,shared_bytes=uint,flush_subset=literal(None)))
    need(not data['overflow'] and data['lost']==0 and data['attempted']==len(data['snapshots'])<16, 'counter snapshot loss/saturation')
    # Actual target layout is reported and checked by the Rust budget assertion;
    # bound every dimension here as well. This does not attest source or device.
    need(48<=data['snapshot_size']<=128 and 0<=data['shared_bytes']<=512, 'counter storage dimensions')
    present=sum(data[k] is not None for k in ('pcm','music'))
    need((data['shared_bytes']==0)==(present==0), 'counter shared storage')
    previous=None
    for ordinal,row in enumerate(data['snapshots'],1):
        need(row['ordinal']==ordinal and row['field'] in FIELDS and row['tag'] in ('Binding','Requested','AfterViewer'), 'counter snapshot identity/order')
        need(row['pre_us'] is not None and row['post_us'] is not None and row['pre_us']<=row['post_us'], 'counter missing/backward bracket')
        if previous:
            need(previous['post_us']<=row['pre_us'], 'counter snapshot order')
        previous=row
    result={}
    for field,key in zip(FIELDS,('pcm','music')):
        binding=data[key]
        snaps=[row for row in data['snapshots'] if row['field']==field]
        if binding is None:
            need(not snaps, 'counter absent binding')
            result[field]=dict(guard='UNKNOWN',reason='counter route absent')
            continue
        need(binding['clock_id']==clock_id, 'counter clock domain')
        need(binding['lifetime_id'] not in (None,[0]*16), 'counter lifetime change/unknown')
        a,b=binding['activation_pre_us'],binding['activation_post_us']
        need(a is not None and b is not None and a<=b, 'counter activation bracket')
        previous=None
        for row in snaps:
            need(row['lifetime_id']==binding['lifetime_id'], 'counter lifetime change/unknown')
            need(row['pre_us'] is not None and row['post_us'] is not None and b<=row['pre_us']<=row['post_us'], 'counter missing/backward bracket')
            need(not row['busy'] and not row['wrapped'], 'counter concurrent read/wrap fault')
            if previous:
                need(previous['post_us']<=row['pre_us'], 'counter snapshot order')
                need(previous['value']<=row['value'], 'counter reset/wrap')
            previous=row
        if not snaps:
            result[field]=dict(guard='UNKNOWN',reason='counter snapshots missing');continue
        need(snaps[0]['tag']=='Binding' and sum(row['tag']=='Binding' for row in snaps)==1, 'counter binding snapshot order')
        final=[row for row in snaps if row['tag']=='AfterViewer']
        need(len(final)<=1 and (not final or final[0] is snaps[-1]), 'counter final snapshot order')
        result[field]=zero_proof(snaps[0],final[0],s,t) if final else dict(guard='UNKNOWN',reason='counter after-viewer snapshot missing')
    return result
