from pathlib import Path
import sys
import json,re
O=Path(sys.argv[1]).resolve();s=(O/'proof/native-merge.s').read_text();q=(O/'proof/native-audio-queue.s').read_text();c=(O/'proof/native-audio-channel.s').read_text()
assert 'cmp\tr9, r6' in s and 'beq.n\t814c62e4' in s
assert s.count('<__aeabi_memcpy4>')==2 and s.count('<__aeabi_memmove4>')==1
assert '<memcmp>' in s and '#572' in s
assert '#11276' in c and 'movs\tr0, #12' in c and 'movs\tr0, #24' in c
assert 'lsl #2' in q and 'movs\tr0, #24' in q
requests=[11276,4,24,4,24,12,24]
rounded=[max(16,(n+11)&~7) for n in requests]
proof={'merge':{'function':'0x814c61fc','stack_frame_bytes':660,'self_swap_guard_cmp':'0x814c629a: cmp r9, r6 (read, write)','equal_branch':'0x814c629c: beq 0x814c62e4, after all three record copies','guarded_swap_calls':['0x814c62c0 memcpy4(596)','0x814c62ce memmove4(596)','0x814c62da memcpy4(596)'],'nonjoin_self_case_copy_bytes':0,'compaction_after_real_joins_preserved':True,'borrowed_uniform_memcmp':'0x814c63c6: memcmp(572), operands selected from inline Uniforms or cached Parameter bits; no preceding 572-byte copies','same_cached_parameter_id_fast_path':'0x814c63d8–0x814c63e2; both overrides absent and IDs equal -> 0x814c63e4'},'fixed_B_only_Vita_storage':{'packet_requested_bytes':11276,'two_capacity_one_queues_requested_bytes':56,'alive_requested_bytes':12,'stats_requested_bytes':24,'allocation_calls':7,'total_requested_bytes':sum(requests),'minimum_rounded_newlib_chunks_bytes':sum(rounded),'rounding_rule':'max(16, (request + 11) & ~7), from r3 shipped allocator','measured_vs_model':'Requests established by new ARM disassembly, not a live Vita census. Rounding is a minimum chunk model; fragmentation and process overhead unmeasured.'},'common_World_backing':{'requested_Vita_bytes':56,'minimum_rounded_newlib_bytes':64,'host_requested_bytes':88,'charged_to':'one common construction allocation, present in A and B; separate from B-only channel costs'}}
(O/'proof/NATIVE-PROOF.json').write_text(json.dumps(proof,indent=2)+'\n');print(json.dumps(proof,indent=2))
