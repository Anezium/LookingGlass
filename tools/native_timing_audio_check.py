"""Strict structural producer journal validation; never device/audio qualification."""
import math
import struct
import sys
from native_timing_check import obj,array,nullable,uint,u32,octet,text,boolean,literal,need
STAMP=dict(pre_us=nullable(uint),post_us=nullable(uint),fault=nullable(text))
BIRTH=dict(audio_id=u32,clock_id=text,creation=lambda x:obj(x,STAMP))
POLICY=dict(host=boolean,**{k:lambda x:need(type(x)is int and -(2**31)<=x<2**31,'producer policy integer') for k in ('id','priority_requested','priority_rc','affinity_requested','affinity_rc','info_rc','priority','affinity','stack')})
WORKER=dict(entry=lambda x:obj(x,STAMP),policy=lambda x:obj(x,POLICY),applied=boolean)
LIFE=dict(audio_birth=nullable(lambda x:obj(x,BIRTH)),worker_spawn_request=lambda x:obj(x,STAMP),worker_birth=nullable(lambda x:obj(x,WORKER)),producer_loop_closed=boolean,worker_exit=u32,thread_join=u32,reporter_status=u32,reporter_join=u32)
BLOCK=dict(seq=u32,begin_us=uint,end_us=uint,total_ms_bits=uint)
FENCE=dict(seq=u32,n=u32,begin_us=uint,end_us=uint,first_block=u32,last_block=u32,quantile_ms_bits=array(uint,3),legacy_id=u32,flags=u32)
RECEIPT={k:u32 for k in ('blocks_produced','blocks_lost','fences_produced','fences_lost','legacy_produced','legacy_lost','legacy_emitted','legacy_queue_dropped','flags')}
RECEIPT.update(attached_us=nullable(uint),start_consumed_us=nullable(uint),final_us=nullable(uint),clock_fault=nullable(text),final_partial=boolean,no_residual_interval=boolean,reason=text)
def number(x):need(type(x)in (float,int) and -sys.float_info.max<=x<=sys.float_info.max and math.isfinite(x),'producer nonfinite legacy value')
def nonnegative(x):number(x);need(x>=0,'producer negative legacy value')
WINDOW={k:uint for k in ('blocks','starved','rejected','flushes','queued','api_max_us','live_peak','gaps_over_block','dropped_reports')}
WINDOW.update(window_ms=lambda x:uint(x,2**128-1),mix=array(nonnegative,3),game_stage=array(u32,2),voices=array(uint,2),gap_max_ms=nonnegative,sleep_over_ms=nonnegative,log_ms=nonnegative,stages=nullable(lambda x:(need(type(x)is list and len(x)==2,'producer stage shape'),array(array(nonnegative,3),7)(x[0]),uint(x[1]))),music=nullable(array(number,6)))
WINDOW.update(api_max_us=u32,gaps_over_block=u32,dropped_reports=u32)
LEGACY=dict(seq=u32,fence_id=u32,pre_us=uint,post_us=uint,emitted=boolean,window=lambda x:obj(x,WINDOW))
STORAGE={k:uint for k in ('block_capacity','fence_capacity','legacy_capacity','scratch_capacity','block_size','fence_size','legacy_size','fixed_bytes','reserved_bytes')}
def bits_value(bits):
    uint(bits);v=struct.unpack('<d',struct.pack('<Q',bits))[0]
    need(math.isfinite(v) and v>=0,'producer invalid total_ms bits')
    return v

def stamp(x,reason):
    need(x is not None and x['fault'] is None and x['pre_us'] is not None and x['post_us'] is not None and x['pre_us']<=x['post_us'],reason)

def validate(a,m,s,t,raw=None):
    obj(a,dict(schema=literal('alice-producer-v1'),status=text,terminal=u32,run_id=nullable(array(octet,16)),clock_id=nullable(text),audio_id=nullable(u32),worker_id=nullable(u32),topology=nullable(u32),lifecycle=nullable(lambda x:obj(x,LIFE)),counter_lifetimes=array(nullable(array(octet,16)),2),binding=nullable(lambda x:obj(x,STAMP)),start_request_us=nullable(uint),seal_request=nullable(lambda x:obj(x,STAMP)),wait_us=uint,wait_limit_us=uint,completed_identity=nullable(lambda x:obj(x,dict(run_id=nullable(array(octet,16)),clock_id=text,audio_id=u32,worker_id=u32))),repeat_seal=boolean,receipt=nullable(lambda x:obj(x,RECEIPT)),blocks=array(lambda x:obj(x,BLOCK)),fences=array(lambda x:obj(x,FENCE)),legacy=array(lambda x:obj(x,LEGACY)),storage=lambda x:obj(x,STORAGE),estimator=literal('sorted[n/2],sorted[(n-1)*95/100],sorted[n-1]; exact legacy total_ms f64 bits')))
    need(a['status']=='Delivered' and a['terminal']==1,'producer completion unavailable')
    need(a['run_id']==m['run_id'] and a['clock_id']==m['clock_id'] and a['audio_id'] not in (0,None) and a['worker_id'] not in (0,None) and a['audio_id']!=a['worker_id'] and a['topology'] not in (0,None),'producer identity')
    need(not a['repeat_seal'] and a['wait_limit_us']<=2_000_000,'producer seal request/wait')
    stamp(a['binding'],'producer binding clock');stamp(a['seal_request'],'producer seal clock')
    life=a['lifecycle'];need(life is not None and life['audio_birth'] is not None and life['worker_birth'] is not None,'producer birth missing')
    birth=life['audio_birth'];worker=life['worker_birth']
    need(birth['audio_id']==a['audio_id'] and birth['clock_id']==a['clock_id'],'producer birth identity')
    for x in (birth['creation'],life['worker_spawn_request'],worker['entry']):stamp(x,'producer birth clock')
    need(birth['creation']['post_us']<=life['worker_spawn_request']['pre_us']<=life['worker_spawn_request']['post_us']<=worker['entry']['pre_us']<=worker['entry']['post_us'],'producer creation/entry/binding order')
    p=worker['policy'];need(p['host']==(a['clock_id']!='vita-newlib-clock4-process-us'),'producer policy clock domain');need(worker['applied'] and (p['host'] or (p['priority_rc']>=0 and p['affinity_rc']>=0 and p['info_rc']>=0 and p['priority']==p['priority_requested'] and p['affinity']==p['affinity_requested'])),'producer policy rejected')
    need(life['worker_exit'] in (0,1) and life['thread_join'] in (0,1) and life['reporter_status'] in (0,1,2) and life['reporter_join'] in (0,1),'producer exit failure')
    need((life['worker_exit']==1)==life['producer_loop_closed'] and (life['thread_join']==0 or life['producer_loop_closed']),'producer loop/thread lifetime')
    r=a['receipt'];need(r is not None,'producer final receipt missing')
    need(r['flags']==0 and r['clock_fault'] is None,'producer fault/loss')
    need(r['reason'] in ('DiagnosticSeal','WorkerStop'),'producer seal before start/abort')
    need(r['attached_us'] is not None and r['start_consumed_us'] is not None and r['final_us'] is not None and a['start_request_us'] is not None,'producer request/attachment missing')
    need(a['binding']['pre_us']<=r['attached_us']<=r['start_consumed_us'] and worker['entry']['post_us']<=r['attached_us'] and a['binding']['post_us']<=a['start_request_us']<=r['start_consumed_us']<=r['final_us'],'producer activation order')
    if r['reason']=='DiagnosticSeal':need(a['seal_request']['pre_us']<=r['final_us'],'producer seal chronology')
    else:need(life['producer_loop_closed'],'producer stop receipt')
    need(a['completed_identity']=={k:a[k] for k in ('run_id','clock_id','audio_id','worker_id')},'producer completed identity')
    need(a['counter_lifetimes']==([raw[k]['lifetime_id'] if raw[k] is not None else None for k in ('pcm','music')] if raw is not None else [None,None]),'producer counter association')
    st=a['storage'];need((st['block_capacity'],st['fence_capacity'],st['legacy_capacity'],st['scratch_capacity'])==(8192,128,128,512),'producer capacity contract')
    need(st['block_size']==24 and 48<=st['fence_size']<=112 and 300<=st['legacy_size']<=512 and 32768<=st['fixed_bytes']<=65536,'producer storage dimensions')
    reserved=st['block_capacity']*st['block_size']+st['fence_capacity']*st['fence_size']+st['legacy_capacity']*st['legacy_size']+st['scratch_capacity']*8+st['fixed_bytes']
    need(st['reserved_bytes']==reserved==m['audio_producer_storage_bytes'],'producer storage receipt')
    blocks,fences,legacy=a['blocks'],a['fences'],a['legacy']
    for name,rows,cap in (('blocks',blocks,8192),('fences',fences,128),('legacy',legacy,128)):
        need(r[name+'_produced']==len(rows)+r[name+'_lost'] and r[name+'_lost']==0 and len(rows)<cap,'producer count/loss/saturation')
    need(r['legacy_produced']==r['legacy_emitted']+r['legacy_queue_dropped'],'producer legacy outcome counts')
    prev=None
    for i,b in enumerate(blocks,1):
        need(b['seq']==i and b['begin_us']<=b['end_us'] and (prev is None or prev['end_us']<=b['begin_us']),'producer block order')
        bits_value(b['total_ms_bits']);prev=b
    prev=None;used=0
    for i,f in enumerate(fences,1):
        need(f['seq']==i and f['begin_us']<f['end_us'] and (prev is None or f['begin_us']==prev['end_us']),'producer fence adjacency')
        need(f['flags']==((1 if i==1 else 0)|(2 if f['legacy_id']==0 else 0)|(4 if f['n']==0 else 0)),'producer fence flags')
        members=[]
        while used<len(blocks) and blocks[used]['end_us']<=f['end_us']:
            b=blocks[used];need(f['begin_us']<b['end_us'],'producer block membership');members.append(b);used+=1
        need(f['n']==len(members)<=512 and (f['first_block'],f['last_block'])==((members[0]['seq'],members[-1]['seq']) if members else (0,0)),'producer fence block IDs/N')
        if members:
            ordered=sorted(members,key=lambda b:-1 if b['total_ms_bits']==2**63 else b['total_ms_bits'])
            expected=[ordered[k]['total_ms_bits'] for k in (len(ordered)//2,(len(ordered)-1)*95//100,len(ordered)-1)]
            need(f['quantile_ms_bits']==expected,'producer quantile bits')
        else:need(f['quantile_ms_bits']==[0,0,0],'producer empty quantile')
        prev=f
    need(used==len(blocks) and bool(fences) and fences[0]['begin_us']==r['start_consumed_us'] and fences[-1]['end_us']==r['final_us'],'producer final endpoints/membership')
    need(r['final_partial']==(fences[-1]['legacy_id']==0) and r['no_residual_interval']==(not r['final_partial']),'producer final partial receipt')
    need(all(f['legacy_id']!=0 for f in fences[:-1]),'producer partial position')
    need(len(legacy)==len(fences)-int(r['final_partial']),'producer legacy receipt')
    for i,l in enumerate(legacy,1):
        need(l['seq']==i and l['fence_id']==i and fences[i-1]['legacy_id']==i and l['pre_us']==fences[i-1]['end_us']<=l['post_us']<=r['final_us'],'producer legacy association/bracket')
        w=l['window'];need(max(w['mix'])==w['mix'][2] and (w['blocks']!=0 or w['mix']==[0,0,0]) and (w['stages'] is None or w['stages'][1]==w['blocks']),'producer legacy units/N')
        f=fences[i-1]
        need(w['blocks']>=f['n'] if i==1 else w['blocks']==f['n'],'producer legacy block count')
        if w['blocks']==f['n']:
            need([struct.unpack('<Q',struct.pack('<d',v))[0] for v in w['mix']]==f['quantile_ms_bits'],'producer legacy quantile bits')
        if w['music'] is not None:
            music=w['music'];need(all(music[k]>=0 for k in (0,1,2,3,5)) and (music[4]==-1 or music[4]>=0) and all(float(music[k]).is_integer() for k in (2,3,5)),'producer legacy music units')
        if i>1:need(legacy[i-2]['post_us']<=l['pre_us'],'producer legacy order')
    need(len(legacy)==len(fences)-int(r['final_partial']) and sum(l['emitted'] for l in legacy)==r['legacy_emitted'],'producer legacy receipt')
    selected=[f for f in fences if f['begin_us']<t and f['end_us']>s]
    need(selected and selected[0]['begin_us']<=s and selected[-1]['end_us']>=t,'producer window coverage')
    need(all(f['n']>0 for f in selected),'producer empty intersecting fence')
    need(all(bits_value(f['quantile_ms_bits'][1])<=8.0 for f in selected),'producer intersecting window p95 exceeds 8ms')
    return dict(structurally_complete=True,provenance='UNATTESTED; self-consistent hash only',selected_fence_ids=[f['seq'] for f in selected],boundary_fence_ids=[f['seq'] for f in selected if f['begin_us']<s or f['end_us']>t],single_block_fence_ids=[f['seq'] for f in selected if f['n']==1],N=[f['n'] for f in selected],p95_ms=[bits_value(f['quantile_ms_bits'][1]) for f in selected],legacy_queue_drops=r['legacy_queue_dropped'],physical_output='UNKNOWN',decoder_stop='UNKNOWN',thread_returned=True if life['thread_join']==1 else 'UNKNOWN')
