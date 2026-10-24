"""Compatibility entry point: use the current r5 helper-only resealer."""
from pathlib import Path
import runpy
if __name__=='__main__':runpy.run_path(str(Path(__file__).with_name('reseal_a1b.py')),run_name='__main__')
