import argparse
from pathlib import Path
from smoke_admission import admit
p=argparse.ArgumentParser();p.add_argument('--root',type=Path,required=True);p.add_argument('--smoke-root',type=Path,required=True)
a=p.parse_args();print(admit(a.root,a.smoke_root))
