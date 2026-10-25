"""Separate transition GPU/pixel oracle; never used for allocation census numbers."""
from pathlib import Path
import sys
# Use the same actual viewer scenario driver, with GPU readback explicitly enabled.
p=Path(__file__).with_name('host_proof.py')
s=p.read_text().replace("LOOKING_GLASS_SAH_SILENT_AUDIO='1'", "LOOKING_GLASS_SAH_SILENT_AUDIO='1',LOOKING_GLASS_SKIN_PROOF='1'")
s=s.replace("if action!='quit':cmd+=['--frames','8']", "if action!='quit':cmd+=['--frames','8','--capture',str(d/'final.png')]")
exec(compile(s,str(p),'exec'))
# Fresh same-implementation control, using the identical immutable executable.
_,control=run('retry-1-before-control','hedge3','hedge3','retry',True,True)
shutil.copyfile(control/'final.png',O/'host/retry-1-before/control.png')
shutil.copyfile(control/'run.log',O/'host/retry-1-before/control.log')
