"""Separate transition GPU/pixel oracle; never used for allocation census numbers."""
from pathlib import Path
import sys
# Use the same actual viewer scenario driver, with GPU readback explicitly enabled.
p=Path(__file__).with_name('host_proof.py')
s=p.read_text().replace("LOOKING_GLASS_SAH_SILENT_AUDIO='1'", "LOOKING_GLASS_SAH_SILENT_AUDIO='1',LOOKING_GLASS_SKIN_PROOF='1'")
s=s.replace("if action!='quit':cmd+=['--frames','8']", "if action!='quit':cmd+=['--frames','8','--capture',str(d/'final.png')]")
exec(compile(s,str(p),'exec'))
